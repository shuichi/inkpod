import AppKit
import Foundation

public struct WorkspaceID: Codable, Hashable, Sendable {
    public let rawValue: UUID

    public init(rawValue: UUID = UUID()) {
        self.rawValue = rawValue
    }
}

enum StartupWorkspaceItem: Equatable, Sendable {
    case recovery(RecoveryCandidate)
    case document(URL)
}

@MainActor
public final class ApplicationCoordinator: ObservableObject {
    let coreHost: CoreHost
    let rendererHost: MetalRendererHost
    let fileAccessBroker: FileAccessBroker
    let fileIdentityRegistry: FileIdentityRegistry
    let bookmarkStore: FileBookmarkStore
    let previousDocumentsStore: FileBookmarkStore
    let batchFolderBookmarkStore: FileBookmarkStore
    let recoveryStore: RecoveryStore
    let clipboardBroker: ClipboardBroker
    public let shortcutController: ShortcutController
    public let languageController: AppLanguageController
    public let sequenceEndpointPolicyController: SequenceEndpointPolicyController
    @Published private(set) var recentURLs: [URL]
    @Published private(set) var restorePreviousDocumentsAtStartup: Bool

    private var workspaces: [WorkspaceID: WorkspaceModel] = [:]
    private struct WindowTarget {
        let workspaceID: WorkspaceID
        let lifecycleGeneration: UInt64
    }
    private var windowTargets: [Int: WindowTarget] = [:]
    private var nextSurfaceID: UInt64 = 1
    private var nextSurfaceGeneration: UInt64 = 1
    private var shuttingDown = false
    private let shortcutMonitor = ShortcutEventMonitor()
    private let helpPresenter = HelpPresenter()
    private var settingsPresenter: (() -> Void)?
    private let restorePreference: PreviousDocumentRestorePreference
    private var startupItems: [StartupWorkspaceItem]
    private var startupItemByWorkspace: [WorkspaceID: StartupWorkspaceItem] = [:]
    private var pendingViewTransfers: [WorkspaceID: WorkspaceViewTransfer] = [:]
    private var sessionOwners: [CoreSessionTarget: Set<WorkspaceID>] = [:]
    private var openWorkspace: ((WorkspaceID) -> Void)?
    private var openBatchWindow: ((BatchWindowID) -> Void)?
    private var batchContexts: [BatchWindowID: CommandTargetContext] = [:]
    private var batchModels: [BatchWindowID: BatchWindowModel] = [:]
    private var didScheduleStartupWindows = false

    public init() {
        coreHost = CoreHost()
        rendererHost = MetalRendererHost()
        fileAccessBroker = FileAccessBroker()
        fileIdentityRegistry = FileIdentityRegistry()
        bookmarkStore = FileBookmarkStore()
        previousDocumentsStore = FileBookmarkStore(
            key: "inkpod.macos.previous-documents.v1",
            maximumCount: 64
        )
        batchFolderBookmarkStore = FileBookmarkStore(
            key: "inkpod.macos.batch-folders.v1",
            maximumCount: 32
        )
        recoveryStore = RecoveryStore()
        clipboardBroker = ClipboardBroker(coreHost: coreHost)
        recentURLs = bookmarkStore.resolvedRecentURLs()
        let restorePreference = PreviousDocumentRestorePreference()
        self.restorePreference = restorePreference
        let restoreEnabled = restorePreference.load()
        restorePreviousDocumentsAtStartup = restoreEnabled
        let recoveryCandidates = recoveryStore.candidates()
        startupItems = recoveryCandidates.map(StartupWorkspaceItem.recovery)
        if restoreEnabled {
            let recoverySourcePaths = Set(recoveryCandidates.compactMap(\.originalPath))
            startupItems.append(contentsOf: previousDocumentsStore.resolvedRecentURLs()
                .filter { !recoverySourcePaths.contains($0.path) }
                .map(StartupWorkspaceItem.document))
        }
        shortcutController = ShortcutController()
        languageController = AppLanguageController()
        sequenceEndpointPolicyController = SequenceEndpointPolicyController()
    }

    func workspace(for id: WorkspaceID) -> WorkspaceModel {
        if let existing = workspaces[id] {
            return existing
        }
        let workspace = WorkspaceModel(id: id, application: self)
        workspaces[id] = workspace
        return workspace
    }

    func claimSession(_ target: CoreSessionTarget, for workspaceID: WorkspaceID) {
        sessionOwners[target, default: []].insert(workspaceID)
    }

    func releaseSession(_ target: CoreSessionTarget, for workspaceID: WorkspaceID) {
        guard var owners = sessionOwners[target] else { return }
        owners.remove(workspaceID)
        if owners.isEmpty {
            sessionOwners.removeValue(forKey: target)
            _ = coreHost.closeSession(target)
        } else {
            sessionOwners[target] = owners
        }
    }

    func hasOtherReadyWorkspace(than workspaceID: WorkspaceID) -> Bool {
        workspaces.contains { id, workspace in
            id != workspaceID && workspace.phase == .ready
        }
    }

    func openNewWorkspaceWindow() {
        openWorkspace?(WorkspaceID())
    }

    func transferActiveView(
        from sourceID: WorkspaceID,
        copy: Bool,
        newWindow: Bool
    ) {
        guard let source = workspaces[sourceID] else { return }
        Task { @MainActor [weak self, weak source] in
            guard let self, let source,
                  let transfer = await source.prepareViewTransfer(copy: copy)
            else { return }
            if newWindow {
                let destinationID = WorkspaceID()
                pendingViewTransfers[destinationID] = transfer
                claimSession(transfer.session.target, for: destinationID)
                if transfer.removesSource {
                    source.completeMovedViewTransfer(transfer.view.id)
                }
                openWorkspace?(destinationID)
                return
            }
            guard let destination = workspaces
                .filter({ $0.key != sourceID && $0.value.phase == .ready })
                .sorted(by: { $0.key.rawValue.uuidString < $1.key.rawValue.uuidString })
                .first?.value,
                destination.adoptViewTransfer(transfer)
            else {
                if copy { _ = await coreHost.closeView(transfer.view.coreTarget).value() }
                return
            }
            if transfer.removesSource {
                source.completeMovedViewTransfer(transfer.view.id)
            }
        }
    }

    func takePendingViewTransfer(for workspaceID: WorkspaceID) -> WorkspaceViewTransfer? {
        pendingViewTransfers.removeValue(forKey: workspaceID)
    }

    func allocateSurfaceTarget() -> CoreSurfaceTarget? {
        guard !shuttingDown,
              nextSurfaceID > 0,
              nextSurfaceGeneration > 0,
              nextSurfaceID < UInt64.max,
              nextSurfaceGeneration < UInt64.max
        else {
            return nil
        }
        let target = CoreSurfaceTarget(
            id: CoreSurfaceID(rawValue: nextSurfaceID),
            generation: CoreSurfaceGeneration(rawValue: nextSurfaceGeneration)
        )
        nextSurfaceID += 1
        nextSurfaceGeneration += 1
        return target
    }

    func workspaceDidStop(_ id: WorkspaceID) {
        workspaces.removeValue(forKey: id)
        windowTargets = windowTargets.filter { $0.value.workspaceID != id }
        let affected = batchContexts.filter { $0.value.workspaceID == id }.map(\.key)
        for batchID in affected { batchModels[batchID]?.stop() }
    }

    func recordRecent(url: URL, identity: FileIdentity) {
        do {
            try bookmarkStore.record(url: url, identity: identity)
        } catch {
            return
        }
        recentURLs = bookmarkStore.resolvedRecentURLs()
        NSDocumentController.shared.noteNewRecentDocumentURL(url)
    }

    func focusSession(_ target: CoreSessionTarget) {
        workspaces.values.first { $0.projection?.target == target }?.focusWindow()
    }

    func openRecent(_ url: URL, context: CommandTargetContext?) {
        guard let context, let workspace = workspaces[context.workspaceID] else { return }
        workspace.openRecentURL(url, context: context)
    }

    func registerWindow(
        _ window: NSWindow,
        workspaceID: WorkspaceID,
        lifecycleGeneration: UInt64
    ) {
        windowTargets[window.windowNumber] = WindowTarget(
            workspaceID: workspaceID,
            lifecycleGeneration: lifecycleGeneration
        )
    }

    func unregisterWindow(_ window: NSWindow, workspaceID: WorkspaceID) {
        guard windowTargets[window.windowNumber]?.workspaceID == workspaceID else { return }
        windowTargets.removeValue(forKey: window.windowNumber)
    }

    func commandContext(for window: NSWindow?) -> CommandTargetContext? {
        guard let window,
              let target = windowTargets[window.windowNumber],
              let workspace = workspaces[target.workspaceID],
              let context = workspace.commandContext,
              context.lifecycleGeneration == target.lifecycleGeneration
        else {
            return nil
        }
        return context
    }

    func installSettingsPresenter(_ presenter: @escaping () -> Void) {
        settingsPresenter = presenter
    }

    func installWorkspaceOpener(_ opener: @escaping (WorkspaceID) -> Void) {
        openWorkspace = opener
    }

    func installBatchWindowOpener(_ opener: @escaping (BatchWindowID) -> Void) {
        openBatchWindow = opener
    }

    public func batchContext(for id: BatchWindowID) -> CommandTargetContext? {
        batchContexts[id]
    }

    func currentBatchContext(for workspaceID: WorkspaceID) -> CommandTargetContext? {
        workspaces[workspaceID]?.commandContext
    }

    func registerBatchModel(_ model: BatchWindowModel) {
        batchModels[model.id] = model
    }

    func batchWindowDidClose(_ id: BatchWindowID) {
        batchModels.removeValue(forKey: id)?.stop()
        batchContexts.removeValue(forKey: id)
    }

    private func presentBatchWindow(context: CommandTargetContext) -> CommandRouteResult {
        guard !shuttingDown, let openBatchWindow else { return .failed(.invalidRequest) }
        let id = BatchWindowID()
        batchContexts[id] = context
        openBatchWindow(id)
        return .started
    }

    func startupItem(for workspaceID: WorkspaceID) -> StartupWorkspaceItem? {
        if let assigned = startupItemByWorkspace.removeValue(forKey: workspaceID) {
            return assigned
        }
        guard !didScheduleStartupWindows else { return nil }
        didScheduleStartupWindows = true
        guard !startupItems.isEmpty else { return nil }
        let first = startupItems.removeFirst()
        for item in startupItems {
            let id = WorkspaceID()
            startupItemByWorkspace[id] = item
            openWorkspace?(id)
        }
        startupItems.removeAll(keepingCapacity: false)
        return first
    }

    func toggleRestorePreviousDocumentsAtStartup() -> Bool {
        let replacement = !restorePreviousDocumentsAtStartup
        guard restorePreference.save(replacement) else { return false }
        restorePreviousDocumentsAtStartup = replacement
        return true
    }

    func commandState(
        _ command: InkpodCommandID,
        context: CommandTargetContext?
    ) -> CommandState {
        let descriptor = CommandCatalog.descriptor(for: command)
        if command == .windowBatch {
            guard !shuttingDown, let context,
                  let workspace = workspaces[context.workspaceID],
                  workspace.commandContext == context
            else { return CommandState(enabled: false) }
            return CommandState(enabled: true)
        }
        if command == .windowJobProgress {
            guard !shuttingDown, let context else { return CommandState(enabled: false) }
            return CommandState(enabled: batchModels.contains { id, model in
                batchContexts[id]?.workspaceID == context.workspaceID && model.isRunning
            })
        }
        if descriptor.targetScope == .application {
            let checked = switch command {
            case .languageSystem: languageController.selection == .system
            case .languageJapanese: languageController.selection == .japanese
            case .languageEnglish: languageController.selection == .english
            default: false
            }
            return CommandState(enabled: !shuttingDown, checked: checked)
        }
        guard let context, let workspace = workspaces[context.workspaceID] else {
            return CommandState(enabled: false)
        }
        var state = workspace.commandState(command, context: context)
        if command == .fileRestorePrevious {
            state = CommandState(
                enabled: state.enabled,
                checked: restorePreviousDocumentsAtStartup
            )
        }
        return state
    }

    func historyProjection(for context: CommandTargetContext?) -> CoreHistoryProjection? {
        guard let context, let workspace = workspaces[context.workspaceID],
              workspace.commandContext == context
        else {
            return nil
        }
        return workspace.history
    }

    func jumpHistory(to cursor: UInt64, context: CommandTargetContext?) {
        guard let context, let workspace = workspaces[context.workspaceID] else { return }
        workspace.jumpHistory(to: cursor, context: context)
    }

    @discardableResult
    func route(
        _ command: InkpodCommandID,
        context: CommandTargetContext?
    ) -> CommandRouteResult {
        guard commandState(command, context: context).enabled else { return .noOp }
        switch command {
        case .appExit:
            NSApp.terminate(nil)
            return .started
        case .helpAbout, .helpManual, .helpFileFormat, .helpWebPage,
             .helpAcknowledgements:
            return helpPresenter.present(command, localizer: languageController)
                ? .started : .failed(.invalidRequest)
        case .shortcutReset:
            return switch shortcutController.reset() {
            case .applied: .started
            case .noOp: .noOp
            case .persistenceFailure: .failed(.invalidRequest)
            default: .invalid
            }
        case .shortcutEdit:
            guard let settingsPresenter else { return .failed(.invalidRequest) }
            settingsPresenter()
            return .presentedInput
        case .languageSystem:
            return languageController.select(.system) ? .started : .noOp
        case .languageJapanese:
            return languageController.select(.japanese) ? .started : .noOp
        case .languageEnglish:
            return languageController.select(.english) ? .started : .noOp
        case .windowBatch:
            guard let context else { return .stale }
            return presentBatchWindow(context: context)
        case .windowJobProgress:
            guard let context, let openBatchWindow,
                  let id = batchModels.keys.sorted(by: {
                      $0.rawValue.uuidString < $1.rawValue.uuidString
                  }).first(where: {
                      batchContexts[$0]?.workspaceID == context.workspaceID
                          && batchModels[$0]?.isRunning == true
                  })
            else { return .noOp }
            openBatchWindow(id)
            return .started
        default:
            guard let context, let workspace = workspaces[context.workspaceID] else {
                return .stale
            }
            return workspace.execute(command, context: context)
        }
    }

    public func startInputMonitoring() {
        shortcutMonitor.start(application: self)
    }

    @discardableResult
    public func shutdown(confirmingDirty: Bool = true) async -> Bool {
        guard !shuttingDown else { return false }
        let pendingWorkspaces = workspaces.values.sorted {
            $0.id.rawValue.uuidString < $1.id.rawValue.uuidString
        }
        if confirmingDirty {
            for workspace in pendingWorkspaces {
                guard await workspace.resolveDirtyBeforeClose() else { return false }
            }
        }
        try? previousDocumentsStore.replace(
            urls: pendingWorkspaces.compactMap(\.documentURL)
        )
        shuttingDown = true
        shortcutMonitor.stop()
        let pendingBatchModels = Array(batchModels.values)
        for model in pendingBatchModels { model.stop() }
        for model in pendingBatchModels { await model.waitUntilStopped() }
        batchModels.removeAll(keepingCapacity: false)
        batchContexts.removeAll(keepingCapacity: false)
        for workspace in pendingWorkspaces {
            await workspace.stop(removingFromApplication: false)
        }
        workspaces.removeAll(keepingCapacity: false)
        await clipboardBroker.shutdown()
        _ = rendererHost.shutdown()
        let task = coreHost.shutdown()
        _ = await task.value()
        _ = coreHost.waitUntilStopped(timeout: 10)
        return true
    }
}

extension WorkspaceID {
    var coreDocumentUUID: CoreDocumentUUID {
        let bytes = rawValue.uuid
        let high = UInt64(bytes.0) << 56 | UInt64(bytes.1) << 48
            | UInt64(bytes.2) << 40 | UInt64(bytes.3) << 32
            | UInt64(bytes.4) << 24 | UInt64(bytes.5) << 16
            | UInt64(bytes.6) << 8 | UInt64(bytes.7)
        let low = UInt64(bytes.8) << 56 | UInt64(bytes.9) << 48
            | UInt64(bytes.10) << 40 | UInt64(bytes.11) << 32
            | UInt64(bytes.12) << 24 | UInt64(bytes.13) << 16
            | UInt64(bytes.14) << 8 | UInt64(bytes.15)
        return CoreDocumentUUID(high: high, low: low)
    }
}
