import AppKit
import Foundation
import SwiftUI
import UniformTypeIdentifiers

public struct BatchWindowID: Codable, Hashable, Sendable {
    public let rawValue: UUID
    public init(rawValue: UUID = UUID()) { self.rawValue = rawValue }
}

private enum BatchPendingAction {
    case preview(CoreBatchRunScope)
    case run(CoreBatchRunOptions)
}

private struct BatchOperationEditorDraft: Identifiable {
    let id = UUID()
    var operation: CoreBatchOperation
    let perRun: Bool
}

private struct BatchPairResolutionDraft: Identifiable {
    struct Group: Identifiable {
        let id = UUID()
        let oldColor: CoreColorValue
        let candidates: [CoreBatchPairCandidateProjection]
        var selection: Int?
    }

    let id = UUID()
    var groups: [Group]
}

@MainActor
final class BatchWindowModel: ObservableObject {
    let id: BatchWindowID
    @Published private(set) var context: CommandTargetContext
    @Published var draft = BatchWindowDraft()
    @Published var selectedInput = 0
    @Published var selectedOperation: UUID?
    @Published var preview: CoreBatchPreviewProjection?
    @Published var report: CoreBatchReportProjection?
    @Published var progress: CoreBatchProgressProjection?
    @Published var loadedSetSummary: CoreBatchGraphSummary?
    @Published var loadedSetURL: URL?
    @Published var isPinned = true
    @Published var isRunning = false
    @Published var diagnostic = ""
    @Published fileprivate var operationEditor: BatchOperationEditorDraft?
    @Published fileprivate var pairResolution: BatchPairResolutionDraft?
    @Published var pairOldIndex: UInt32 = 0
    @Published var pairNewIndex: UInt32 = 1

    private unowned let application: ApplicationCoordinator
    private let folderBroker = BatchFolderBroker()
    private let jobs = BatchJobRegistry()
    private var pendingAction: BatchPendingAction?
    private var runOverrides: [UUID: CoreBatchOperation] = [:]

    init(id: BatchWindowID, context: CommandTargetContext, application: ApplicationCoordinator) {
        self.id = id
        self.context = context
        self.application = application
    }

    var selectedOperationIndex: Int? {
        guard let selectedOperation else { return nil }
        return draft.operations.firstIndex { $0.id == selectedOperation }
    }

    func state(_ command: InkpodCommandID) -> CommandState {
        let editable = !isRunning && loadedSetURL == nil
        let hasOperation = selectedOperationIndex != nil
        let canRun = !isRunning && (loadedSetSummary?.operations.isEmpty == false
            || draft.coreGraph.isValid)
        let checked: Bool = switch command {
        case .batchOutputDuplicate: draft.output.policy == .duplicate
        case .batchOutputNew: draft.output.policy == .newSave
        case .batchOutputOverwrite: draft.output.policy == .explicitOverwrite
        case .batchFailureContinue: draft.output.failurePolicy == .continue
        case .batchFailureStop: draft.output.failurePolicy == .stop
        case .batchPin: isPinned
        default: false
        }
        let enabled: Bool
        if BatchCommandCatalog.operationCommands[command] != nil {
            enabled = editable
        } else {
            enabled = switch command {
            case .batchCancel: isRunning
            case .batchOperationRemove, .batchOperationEdit:
                editable && hasOperation
            case .batchReplaceSwap:
                editable && selectedOperationIndex.map {
                    draft.operations[$0].kind == .colorReplace
                } == true
            case .batchOperationUp:
                editable && (selectedOperationIndex ?? 0) > 0
            case .batchOperationDown:
                editable && selectedOperationIndex.map { $0 + 1 < draft.operations.count } == true
            case .batchPreview, .batchDryRun, .batchRunCurrent, .batchRunAll:
                canRun
            case .batchInputFile, .batchInputFolder, .batchInputCurrent,
                 .batchInputRange, .batchOutputDuplicate, .batchOutputNew,
                 .batchOutputOverwrite, .batchFailureContinue, .batchFailureStop,
                 .batchOutputSettings, .batchSaveSet:
                editable && (command != .batchInputRange
                    || draft.inputs.indices.contains(selectedInput))
            case .batchExtractPairs:
                editable && pairOldIndex != pairNewIndex
            default:
                !isRunning
            }
        }
        return CommandState(enabled: enabled, checked: checked)
    }

    @discardableResult
    func execute(_ command: InkpodCommandID) -> CommandRouteResult {
        guard state(command).enabled else { return .noOp }
        if [.batchPreview, .batchDryRun, .batchRunCurrent, .batchRunAll,
            .batchExtractPairs].contains(command),
           !refreshTargetForIssue()
        {
            diagnostic = "The source workspace is no longer available."
            return .stale
        }
        if let operation = BatchCommandCatalog.operationCommands[command] {
            clearLoadedSet()
            _ = draft.add(operation)
            selectedOperation = operation.id
            return .started
        }
        switch command {
        case .batchInputFile:
            presentInputPanel(folder: false)
            return .presentedInput
        case .batchInputFolder:
            presentInputPanel(folder: true)
            return .presentedInput
        case .batchInputCurrent:
            clearLoadedSet()
            draft.inputs.append(.currentSequence())
            selectedInput = draft.inputs.count - 1
            return .started
        case .batchOperationRemove:
            guard let index = selectedOperationIndex else { return .noOp }
            clearLoadedSet()
            _ = draft.removeOperation(at: index)
            selectedOperation = draft.operations.indices.contains(index)
                ? draft.operations[index].id : draft.operations.last?.id
            return .started
        case .batchOperationUp:
            return moveSelected(by: -1)
        case .batchOperationDown:
            return moveSelected(by: 1)
        case .batchOperationEdit:
            guard let index = selectedOperationIndex else { return .noOp }
            operationEditor = BatchOperationEditorDraft(
                operation: draft.operations[index],
                perRun: false
            )
            return .presentedInput
        case .batchReplaceSwap:
            guard let index = selectedOperationIndex,
                  draft.operations[index].kind == .colorReplace
            else { return .invalid }
            clearLoadedSet()
            for pairIndex in draft.operations[index].colorPairs.indices {
                let old = draft.operations[index].colorPairs[pairIndex].oldColor
                draft.operations[index].colorPairs[pairIndex].oldColor =
                    draft.operations[index].colorPairs[pairIndex].newColor
                draft.operations[index].colorPairs[pairIndex].newColor = old
            }
            return .started
        case .batchOutputDuplicate:
            clearLoadedSet(); draft.output.policy = .duplicate; return .started
        case .batchOutputNew:
            clearLoadedSet(); draft.output.policy = .newSave; return .started
        case .batchOutputOverwrite:
            clearLoadedSet(); draft.output.policy = .explicitOverwrite; return .started
        case .batchFailureContinue:
            clearLoadedSet(); draft.output.failurePolicy = .continue; return .started
        case .batchFailureStop:
            clearLoadedSet(); draft.output.failurePolicy = .stop; return .started
        case .batchPreview:
            begin(.preview(.all)); return .started
        case .batchDryRun:
            begin(.run(CoreBatchRunOptions(scope: .all, dryRun: true, previewConfirmed: true)))
            return .started
        case .batchRunCurrent:
            begin(.run(CoreBatchRunOptions(scope: .current, previewConfirmed: true)))
            return .started
        case .batchRunAll:
            begin(.run(CoreBatchRunOptions(scope: .all, previewConfirmed: true)))
            return .started
        case .batchSaveSet:
            presentSaveSetPanel(); return .presentedInput
        case .batchLoadSet:
            presentLoadSetPanel(); return .presentedInput
        case .batchCancel:
            cancel(); return .started
        case .batchPin:
            isPinned.toggle(); return .started
        case .batchExtractPairs:
            extractPairs(); return .started
        case .batchInputRange:
            guard draft.inputs.indices.contains(selectedInput) else { return .invalid }
            guard draft.inputs[selectedInput].isValid else {
                diagnostic = "Enter a valid inclusive cell range."
                return .invalid
            }
            diagnostic = "The cell range is valid."
            return .started
        case .batchOutputSettings:
            guard draft.coreGraph.isValid else {
                diagnostic = "Complete the batch inputs, operations, and output settings."
                return .invalid
            }
            diagnostic = "The batch settings are valid."
            return .started
        default:
            return .presentedInput
        }
    }

    func applyOperationEditor(_ value: CoreBatchOperation) {
        guard let editor = operationEditor else { return }
        operationEditor = nil
        if editor.perRun {
            var resolved = value
            resolved.configureEachRun = false
            runOverrides[value.id] = resolved
            resumePendingAction()
            return
        }
        guard let index = draft.operations.firstIndex(where: { $0.id == value.id }) else { return }
        clearLoadedSet()
        draft.operations[index] = value
    }

    func dismissOperationEditor() {
        operationEditor = nil
        pendingAction = nil
        runOverrides.removeAll(keepingCapacity: false)
    }

    fileprivate func applyPairResolution(_ value: BatchPairResolutionDraft) {
        pairResolution = nil
        let pairs = value.groups.compactMap { group -> CoreBatchColorPair? in
            guard let selection = group.selection,
                  group.candidates.indices.contains(selection)
            else { return nil }
            return CoreBatchColorPair(
                oldColor: group.oldColor,
                newColor: group.candidates[selection].newColor
            )
        }
        guard !pairs.isEmpty else {
            diagnostic = "No color pairs were selected."
            return
        }
        clearLoadedSet()
        let operation = CoreBatchOperation(kind: .colorReplace, colorPairs: pairs)
        _ = draft.add(operation)
        selectedOperation = operation.id
        diagnostic = "Added \(pairs.count) exact color pairs."
    }

    func dismissPairResolution() {
        pairResolution = nil
    }

    func chooseOutputFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.directoryURL = try? application.batchFolderBookmarkStore.resolveMostRecent()?.url
        panel.begin { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            Task { @MainActor [weak self] in self?.installOutputFolder(url) }
        }
    }

    func stop() {
        jobs.close(using: application.coreHost)
    }

    func waitUntilStopped() async {
        await jobs.waitUntilStopped()
    }

    private func begin(_ action: BatchPendingAction) {
        pendingAction = action
        runOverrides.removeAll(keepingCapacity: true)
        resumePendingAction()
    }

    private func refreshTargetForIssue() -> Bool {
        if isPinned {
            return true
        }
        guard let current = application.currentBatchContext(for: context.workspaceID) else {
            return false
        }
        context = current
        return true
    }

    private func resumePendingAction() {
        guard let pendingAction else { return }
        let sourceOperations = loadedSetSummary?.operations ?? draft.operations
        if let unresolved = sourceOperations.first(where: {
               $0.configureEachRun && runOverrides[$0.id] == nil
           })
        {
            operationEditor = BatchOperationEditorDraft(operation: unresolved, perRun: true)
            return
        }
        self.pendingAction = nil
        var graph = draft.coreGraph
        graph.operations = sourceOperations.map { runOverrides[$0.id] ?? $0 }
        runOverrides.removeAll(keepingCapacity: false)
        switch pendingAction {
        case let .preview(scope): launchPreview(graph: graph, scope: scope)
        case let .run(options): launchRun(graph: graph, options: options)
        }
    }

    private func launchPreview(graph: CoreBatchGraphDraft, scope: CoreBatchRunScope) {
        diagnostic = ""
        let task = if let url = loadedSetURL {
            application.coreHost.previewSavedBatch(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                pathUTF8: Array(url.path.utf8),
                operations: graph.operations,
                scope: scope
            )
        } else {
            application.coreHost.previewBatch(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                graph: graph,
                scope: scope
            )
        }
        guard jobs.start(task) else {
            _ = application.coreHost.cancel(request: task.requestID)
            diagnostic = "Another Batch job is already running."
            return
        }
        isRunning = true
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            guard let self else { return }
            jobs.complete(task)
            isRunning = false
            if case let .batchPreview(value) = outcome { preview = value }
            else { diagnostic = describe(outcome) }
        }
    }

    private func launchRun(graph: CoreBatchGraphDraft, options: CoreBatchRunOptions) {
        diagnostic = ""
        report = nil
        let task = if let url = loadedSetURL {
            application.coreHost.executeSavedBatch(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                pathUTF8: Array(url.path.utf8),
                operations: graph.operations,
                options: options
            )
        } else {
            application.coreHost.executeBatch(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                graph: graph,
                options: options
            )
        }
        guard jobs.start(task) else {
            _ = application.coreHost.cancel(request: task.requestID)
            diagnostic = "Another Batch job is already running."
            return
        }
        isRunning = true
        Task { @MainActor [weak self] in
            guard let self else { return }
            while jobs.activeTask === task {
                progress = application.coreHost.batchProgress(request: task.requestID)
                try? await Task.sleep(for: .milliseconds(100))
            }
        }
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            guard let self else { return }
            jobs.complete(task)
            isRunning = false
            progress = nil
            if case let .batchReport(value) = outcome { report = value }
            else { diagnostic = describe(outcome) }
        }
    }

    private func cancel() {
        jobs.cancel(using: application.coreHost)
    }

    private func moveSelected(by offset: Int) -> CommandRouteResult {
        guard let source = selectedOperationIndex else { return .noOp }
        let destination = source + offset
        clearLoadedSet()
        return draft.moveOperation(from: source, to: destination) == .applied
            ? .started : .noOp
    }

    private func presentInputPanel(folder: Bool) {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = folder
        panel.canChooseFiles = !folder
        panel.allowsMultipleSelection = true
        panel.directoryURL = try? application.batchFolderBookmarkStore.resolveMostRecent()?.url
        if !folder { panel.allowedContentTypes = [FileTypeCatalog.native] }
        panel.begin { [weak self] response in
            guard response == .OK else { return }
            Task { @MainActor [weak self] in self?.installInputs(panel.urls, folder: folder) }
        }
    }

    private func installInputs(_ urls: [URL], folder: Bool) {
        guard !urls.isEmpty else { return }
        var accepted: [(URL, SecurityScopedResourceLease)] = []
        for url in urls {
            guard let lease = folderBroker.acquire(url) else {
                accepted.forEach { $0.1.close() }
                diagnostic = "The selected location is no longer accessible."
                return
            }
            accepted.append((url, lease))
        }
        clearLoadedSet()
        jobs.retain(contentsOf: accepted.map(\.1))
        for url in urls {
            try? application.batchFolderBookmarkStore.record(
                url: url,
                identity: FileIdentity.resolve(url)
            )
        }
        draft.inputs.append(contentsOf: accepted.map {
            folder ? .folder($0.0.path) : .file($0.0.path)
        })
        selectedInput = draft.inputs.count - 1
    }

    private func installOutputFolder(_ url: URL) {
        guard let lease = folderBroker.acquire(url) else {
            diagnostic = "The selected output folder is no longer accessible."
            return
        }
        clearLoadedSet()
        jobs.retain(lease)
        try? application.batchFolderBookmarkStore.record(
            url: url,
            identity: FileIdentity.resolve(url)
        )
        draft.output.folder = url.path
    }

    private func presentSaveSetPanel() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "inkbatch") ?? .data]
        panel.nameFieldStringValue = "Batch Set.inkbatch"
        panel.begin { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            Task { @MainActor [weak self] in self?.saveSet(url) }
        }
    }

    private func saveSet(_ url: URL) {
        guard draft.coreGraph.isValid else { diagnostic = "Complete the batch graph first."; return }
        guard let lease = folderBroker.acquire(url) else {
            diagnostic = "The selected save location is no longer accessible."
            return
        }
        let task = application.coreHost.saveBatchGraph(draft.coreGraph, pathUTF8: Array(url.path.utf8))
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            lease.close()
            guard let self else { return }
            if case .acknowledged = outcome { diagnostic = "Batch set saved." }
            else { diagnostic = describe(outcome) }
        }
    }

    private func presentLoadSetPanel() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "inkbatch") ?? .data]
        panel.allowsMultipleSelection = false
        panel.begin { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            Task { @MainActor [weak self] in self?.loadSet(url) }
        }
    }

    private func loadSet(_ url: URL) {
        guard let lease = folderBroker.acquire(url) else {
            diagnostic = "The selected batch set is no longer accessible."
            return
        }
        let task = application.coreHost.inspectBatchGraph(pathUTF8: Array(url.path.utf8))
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            guard let self else { lease.close(); return }
            guard case let .batchGraph(summary) = outcome else {
                lease.close()
                diagnostic = describe(outcome)
                return
            }
            jobs.replaceLeases(with: [lease])
            loadedSetURL = url
            loadedSetSummary = summary
            draft.operations = summary.operations
            selectedOperation = summary.operations.first?.id
            diagnostic = "Loaded \(url.lastPathComponent)."
        }
    }

    private func extractPairs() {
        let task = application.coreHost.extractBatchColorPairs(
            target: context.session,
            expectedDocumentRevision: context.documentRevision,
            oldSequenceIndex: pairOldIndex,
            newSequenceIndex: pairNewIndex
        )
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            guard let self else { return }
            guard case let .batchPairPreview(value) = outcome else {
                diagnostic = describe(outcome)
                return
            }
            guard value.ambiguityCount == 0 else {
                var order: [CoreColorValue] = []
                var grouped: [CoreColorValue: [CoreBatchPairCandidateProjection]] = [:]
                for candidate in value.candidates {
                    if grouped[candidate.oldColor] == nil { order.append(candidate.oldColor) }
                    grouped[candidate.oldColor, default: []].append(candidate)
                }
                pairResolution = BatchPairResolutionDraft(groups: order.map { oldColor in
                    let candidates = grouped[oldColor] ?? []
                    return BatchPairResolutionDraft.Group(
                        oldColor: oldColor,
                        candidates: candidates,
                        selection: candidates.count == 1 ? 0 : nil
                    )
                })
                return
            }
            let operation = CoreBatchOperation(
                kind: .colorReplace,
                colorPairs: value.candidates.map {
                    CoreBatchColorPair(oldColor: $0.oldColor, newColor: $0.newColor)
                }
            )
            _ = draft.add(operation)
            selectedOperation = operation.id
            diagnostic = "Extracted \(value.candidates.count) exact color pairs."
        }
    }

    private func clearLoadedSet() {
        loadedSetURL = nil
        loadedSetSummary = nil
    }

    private func describe(_ outcome: CoreRequestOutcome) -> String {
        switch outcome {
        case .failed(.staleTarget): "The source document changed; reopen the Batch window."
        case .failed(.cancelled): "Batch cancelled."
        case let .failed(failure): "Batch failed: \(String(describing: failure))."
        default: "Unexpected batch result."
        }
    }
}

public struct InkpodBatchWindowScene: View {
    @StateObject private var model: BatchWindowModel
    @ObservedObject private var language: AppLanguageController
    private let application: ApplicationCoordinator

    public init(id: BatchWindowID, context: CommandTargetContext, application: ApplicationCoordinator) {
        _model = StateObject(wrappedValue: BatchWindowModel(
            id: id,
            context: context,
            application: application
        ))
        language = application.languageController
        self.application = application
    }

    public var body: some View {
        VStack(spacing: 0) {
            HSplitView {
                inputColumn
                operationColumn
                settingsColumn
            }
            Divider()
            resultArea
        }
        .frame(minWidth: 920, minHeight: 620)
        .sheet(item: $model.operationEditor) { editor in
            BatchOperationEditor(
                initial: editor.operation,
                perRun: editor.perRun,
                onCancel: model.dismissOperationEditor,
                onApply: model.applyOperationEditor
            )
            .interactiveDismissDisabled()
        }
        .sheet(item: $model.pairResolution) { resolution in
            BatchPairResolutionView(
                initial: resolution,
                onCancel: model.dismissPairResolution,
                onApply: model.applyPairResolution
            )
            .interactiveDismissDisabled()
        }
        .task { application.registerBatchModel(model) }
        .onDisappear { application.batchWindowDidClose(model.id) }
    }

    private var inputColumn: some View {
        VStack(alignment: .leading) {
            Text("Inputs")
                .font(.headline)
                .accessibilityIdentifier("inkpod.batch.window")
            List(selection: $model.selectedInput) {
                ForEach(Array(model.draft.inputs.enumerated()), id: \.offset) { index, input in
                    Text(input.kind == .currentSequence ? "Current sequence" : URL(filePath: input.path).lastPathComponent)
                        .tag(index)
                }
            }
            .disabled(model.loadedSetURL != nil || model.isRunning)
            HStack {
                batchButton(.batchInputFile, icon: "doc.badge.plus")
                batchButton(.batchInputFolder, icon: "folder.badge.plus")
                batchButton(.batchInputCurrent, icon: "rectangle.stack")
            }
            if model.draft.inputs.indices.contains(model.selectedInput) {
                HStack {
                    TextField("First", value: $model.draft.inputs[model.selectedInput].firstCell, format: .number)
                    TextField("Last", value: $model.draft.inputs[model.selectedInput].lastCell, format: .number)
                    batchButton(.batchInputRange)
                }
                .textFieldStyle(.roundedBorder)
                .disabled(model.loadedSetURL != nil || model.isRunning)
            }
            Divider()
            HStack {
                TextField("Old cell index", value: $model.pairOldIndex, format: .number)
                TextField("New cell index", value: $model.pairNewIndex, format: .number)
                batchButton(.batchExtractPairs)
            }
            .textFieldStyle(.roundedBorder)
            .disabled(model.loadedSetURL != nil || model.isRunning)
        }
        .padding()
        .frame(minWidth: 230)
    }

    private var operationColumn: some View {
        VStack(alignment: .leading) {
            Text("Ordered operations").font(.headline)
            List(selection: $model.selectedOperation) {
                ForEach(model.draft.operations) { operation in
                    HStack {
                        Image(systemName: operation.enabled ? "checkmark.circle" : "circle")
                        Text(operation.kind.label)
                        if operation.configureEachRun { Image(systemName: "slider.horizontal.3") }
                    }
                    .tag(operation.id)
                }
            }
            HStack {
                Menu("Add") {
                    ForEach(BatchCommandCatalog.operationCommands.keys.sorted(by: { $0.rawValue < $1.rawValue }), id: \.rawValue) { command in
                        Button(language.commandLabel(command)) { _ = model.execute(command) }
                            .disabled(!model.state(command).enabled)
                            .accessibilityIdentifier("inkpod.batch.command.\(command.rawValue)")
                    }
                }
                .accessibilityIdentifier("inkpod.batch.add-operation")
                batchButton(.batchOperationEdit, icon: "slider.horizontal.3")
                batchButton(.batchOperationRemove, icon: "minus")
                batchButton(.batchOperationUp, icon: "arrow.up")
                batchButton(.batchOperationDown, icon: "arrow.down")
                batchButton(.batchReplaceSwap, icon: "arrow.left.arrow.right")
            }
        }
        .padding()
        .frame(minWidth: 320)
    }

    private var settingsColumn: some View {
        Form {
            TextField("Set name", text: $model.draft.name)
                .disabled(model.loadedSetURL != nil || model.isRunning)
            LabeledContent("Output") {
                HStack {
                    batchButton(.batchOutputDuplicate)
                    batchButton(.batchOutputNew)
                    batchButton(.batchOutputOverwrite)
                }
            }
            LabeledContent("Folder") {
                Button(model.draft.output.folder.isEmpty ? "Choose…" : URL(filePath: model.draft.output.folder).lastPathComponent) {
                    model.chooseOutputFolder()
                }
                .disabled(model.loadedSetURL != nil || model.isRunning)
            }
            TextField("Basename", text: $model.draft.output.basename)
                .disabled(model.loadedSetURL != nil || model.isRunning)
            TextField("Start number", value: $model.draft.output.startNumber, format: .number)
                .disabled(model.loadedSetURL != nil || model.isRunning)
            Toggle("Descending numbers", isOn: $model.draft.output.descending)
                .disabled(model.loadedSetURL != nil || model.isRunning)
            Toggle("Cell subfolders", isOn: $model.draft.output.cellFolder)
                .disabled(model.loadedSetURL != nil || model.isRunning)
            Toggle("Preview before save", isOn: $model.draft.output.previewBeforeSave)
                .disabled(model.loadedSetURL != nil || model.isRunning)
            LabeledContent("On failure") {
                HStack {
                    batchButton(.batchFailureContinue)
                    batchButton(.batchFailureStop)
                }
            }
            TextField("Wait (ms)", value: $model.draft.output.waitMilliseconds, format: .number)
                .disabled(model.loadedSetURL != nil || model.isRunning)
            batchButton(.batchOutputSettings)
            Divider()
            HStack {
                batchButton(.batchLoadSet)
                batchButton(.batchSaveSet)
                batchButton(.batchPin)
            }
            if let url = model.loadedSetURL, let summary = model.loadedSetSummary {
                Text("Loaded: \(url.lastPathComponent) · \(summary.operationCount) operations")
                    .font(.caption)
            }
            Divider()
            HStack {
                batchButton(.batchPreview)
                batchButton(.batchDryRun)
            }
            HStack {
                batchButton(.batchRunCurrent)
                batchButton(.batchRunAll)
                batchButton(.batchCancel)
            }
            if let progress = model.progress {
                if progress.totalWork > 0 {
                    ProgressView(value: Double(progress.completedWork), total: Double(progress.totalWork))
                } else {
                    ProgressView()
                }
            }
        }
        .formStyle(.grouped)
        .frame(minWidth: 330)
    }

    private var resultArea: some View {
        VStack(alignment: .leading) {
            if !model.diagnostic.isEmpty {
                Text(model.diagnostic).foregroundStyle(.secondary)
            }
            if let preview = model.preview {
                Text("Preview — \(preview.items.count) items").font(.headline)
                List(Array(preview.items.enumerated()), id: \.offset) { _, item in
                    HStack {
                        Text(item.inputName).frame(maxWidth: .infinity, alignment: .leading)
                        Text(item.outputPath).frame(maxWidth: .infinity, alignment: .leading)
                        Text(item.warning).frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            } else if let report = model.report {
                Text("Report — \(report.items.count) items, \(report.failureCount) failures")
                    .font(.headline)
                List(Array(report.items.enumerated()), id: \.offset) { _, item in
                    HStack {
                        Text(item.inputName).frame(maxWidth: .infinity, alignment: .leading)
                        Text(item.outputPath).frame(maxWidth: .infinity, alignment: .leading)
                        Text(item.message).frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .accessibilityIdentifier("inkpod.batch.report")
            } else {
                ContentUnavailableView("No preview or report", systemImage: "list.bullet.clipboard")
            }
        }
        .padding()
        .frame(minHeight: 180)
    }

    private func batchButton(_ command: InkpodCommandID, icon: String? = nil) -> some View {
        let state = model.state(command)
        return Button {
            _ = model.execute(command)
        } label: {
            if state.checked { Image(systemName: "checkmark") }
            if let icon { Image(systemName: icon) }
            else { Text(language.commandLabel(command)) }
        }
        .disabled(!state.enabled)
        .accessibilityIdentifier("inkpod.batch.command.\(command.rawValue)")
    }
}

private struct BatchOperationEditor: View {
    @State private var operation: CoreBatchOperation
    let perRun: Bool
    let onCancel: () -> Void
    let onApply: (CoreBatchOperation) -> Void

    init(
        initial: CoreBatchOperation,
        perRun: Bool,
        onCancel: @escaping () -> Void,
        onApply: @escaping (CoreBatchOperation) -> Void
    ) {
        _operation = State(initialValue: initial)
        self.perRun = perRun
        self.onCancel = onCancel
        self.onApply = onApply
    }

    var body: some View {
        Form {
            Text(perRun ? "Configure for this run" : "Operation settings").font(.headline)
            LabeledContent("Type", value: operation.kind.label)
            Toggle("Enabled", isOn: $operation.enabled)
            if !perRun { Toggle("Configure each run", isOn: $operation.configureEachRun) }
            if operation.target != nil {
                Picker("Missing target", selection: targetMissingPolicy) {
                    Text("Error").tag(CoreBatchMissingPolicy.error)
                    Text("Skip").tag(CoreBatchMissingPolicy.skip)
                }
                Picker("Layer kind", selection: targetLayerKind) {
                    ForEach(CoreLayerKind.allCases, id: \.rawValue) { Text(String(describing: $0)).tag($0) }
                }
                if operation.kind != .visibility {
                    Picker("Plane kind", selection: targetPlaneKind) {
                        ForEach(CorePlaneKind.allCases, id: \.rawValue) { Text(String(describing: $0)).tag($0) }
                    }
                }
                TextField("Layer stable ID (0 = kind)", value: targetLayerID, format: .number)
                TextField("Plane stable ID (0 = kind)", value: targetPlaneID, format: .number)
            }
            ForEach(operation.parameters.indices, id: \.self) { index in
                TextField("Parameter \(index + 1)", value: $operation.parameters[index], format: .number)
            }
            if operation.kind == .colorReplace {
                Text("Exact color pairs").font(.headline)
                ForEach(operation.colorPairs.indices, id: \.self) { index in
                    VStack(alignment: .leading) {
                        Toggle("Enabled", isOn: $operation.colorPairs[index].enabled)
                        BatchColorValueEditor(
                            title: "Old",
                            color: $operation.colorPairs[index].oldColor
                        )
                        BatchColorValueEditor(
                            title: "New",
                            color: $operation.colorPairs[index].newColor
                        )
                        Button("Remove pair", role: .destructive) {
                            operation.colorPairs.remove(at: index)
                        }
                    }
                }
                Button("Add pair") {
                    guard operation.colorPairs.count < 4_096 else { return }
                    operation.colorPairs.append(CoreBatchColorPair(
                        oldColor: .rgba8(red: 0, green: 0, blue: 0),
                        newColor: .rgba8(red: 255, green: 255, blue: 255)
                    ))
                }
                Button("Swap old and new") {
                    for index in operation.colorPairs.indices {
                        let old = operation.colorPairs[index].oldColor
                        operation.colorPairs[index].oldColor = operation.colorPairs[index].newColor
                        operation.colorPairs[index].newColor = old
                    }
                }
            }
            if operation.kind == .continuousFill {
                Text("Fill seeds").font(.headline)
                ForEach(operation.seeds.indices, id: \.self) { index in
                    VStack(alignment: .leading) {
                        Toggle("Enabled", isOn: $operation.seeds[index].enabled)
                        TextField("X", value: $operation.seeds[index].x, format: .number)
                        TextField("Y", value: $operation.seeds[index].y, format: .number)
                        TextField(
                            "Tolerance",
                            value: $operation.seeds[index].tolerance,
                            format: .number
                        )
                        TextField(
                            "Gap close",
                            value: $operation.seeds[index].gapClose,
                            format: .number
                        )
                        BatchColorValueEditor(
                            title: "Fill color",
                            color: $operation.seeds[index].fillColor
                        )
                        Toggle("Require expected source color", isOn: seedExpectedEnabled(index))
                        if operation.seeds[index].expectedColor != nil {
                            BatchColorValueEditor(
                                title: "Expected source",
                                color: seedExpectedColor(index)
                            )
                        }
                        Button("Remove seed", role: .destructive) {
                            operation.seeds.remove(at: index)
                        }
                    }
                }
                Button("Add seed") {
                    guard operation.seeds.count < 4_096 else { return }
                    operation.seeds.append(CoreBatchSeed(
                        x: 0,
                        y: 0,
                        fillColor: .rgba8(red: 255, green: 255, blue: 255)
                    ))
                }
            }
            if operation.kind == .separation || operation.kind == .boundaryAirbrush {
                Text("Colors").font(.headline)
                ForEach(operation.colors.indices, id: \.self) { index in
                    BatchColorValueEditor(title: "Color \(index + 1)", color: $operation.colors[index])
                    Button("Remove color", role: .destructive) {
                        operation.colors.remove(at: index)
                    }
                }
                Button("Add color") {
                    guard operation.colors.count < 4_096 else { return }
                    operation.colors.append(.rgba8(red: 0, green: 0, blue: 0))
                }
            }
            if operation.kind == .separation,
               !operation.colorPairs.isEmpty
            {
                Picker("Destination", selection: separationDestination) {
                    ForEach(CoreBatchSeparationDestination.allCases, id: \.rawValue) {
                        Text(String(describing: $0)).tag($0)
                    }
                }
                BatchColorValueEditor(
                    title: "Replacement",
                    color: $operation.colorPairs[0].newColor
                )
            }
            if operation.kind == .filter, operation.filter != nil {
                Picker("Filter", selection: filterKind) {
                    ForEach(CoreFilterKind.allCases, id: \.rawValue) {
                        Text(String(describing: $0)).tag($0)
                    }
                }
                TextField("Filter plane ID", value: filterPlaneID, format: .number)
                Picker("Channel", selection: filterChannel) {
                    ForEach(CoreFilterChannel.allCases, id: \.rawValue) {
                        Text(String(describing: $0)).tag($0)
                    }
                }
                Picker("Curve interpolation", selection: filterInterpolation) {
                    ForEach(CoreCurveInterpolation.allCases, id: \.rawValue) {
                        Text(String(describing: $0)).tag($0)
                    }
                }
                ForEach(operation.filter?.parameters.indices ?? 0 ..< 0, id: \.self) { index in
                    TextField("Filter parameter \(index + 1)", value: filterParameter(index), format: .number)
                }
                ForEach(operation.filter?.curvePoints.indices ?? 0 ..< 0, id: \.self) { index in
                    HStack {
                        TextField("Input", value: filterCurveInput(index), format: .number)
                        TextField("Output", value: filterCurveOutput(index), format: .number)
                        Button("Remove point", role: .destructive) {
                            operation.filter?.curvePoints.remove(at: index)
                        }
                    }
                }
                Button("Add curve point") {
                    guard let count = operation.filter?.curvePoints.count, count < 4_096 else {
                        return
                    }
                    operation.filter?.curvePoints.append(CoreCurvePoint(input: 0, output: 0))
                }
            }
            HStack {
                Button("Cancel", role: .cancel, action: onCancel)
                Spacer()
                Button("Apply") { onApply(operation) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!operation.isValid)
            }
        }
        .padding()
        .frame(width: 460)
    }

    private var targetMissingPolicy: Binding<CoreBatchMissingPolicy> {
        Binding(
            get: { operation.target?.missingPolicy ?? .error },
            set: { operation.target?.missingPolicy = $0 }
        )
    }

    private var targetLayerKind: Binding<CoreLayerKind> {
        Binding(
            get: { operation.target?.layerKind ?? .binaryColoring },
            set: { operation.target?.layerKind = $0 }
        )
    }

    private var targetPlaneKind: Binding<CorePlaneKind> {
        Binding(
            get: { operation.target?.planeKind ?? .color },
            set: { operation.target?.planeKind = $0 }
        )
    }

    private var targetLayerID: Binding<UInt64> {
        Binding(
            get: { operation.target?.layerID ?? 0 },
            set: { operation.target?.layerID = $0 == 0 ? nil : $0 }
        )
    }

    private var targetPlaneID: Binding<UInt64> {
        Binding(
            get: { operation.target?.planeID ?? 0 },
            set: { operation.target?.planeID = $0 == 0 ? nil : $0 }
        )
    }

    private var separationDestination: Binding<CoreBatchSeparationDestination> {
        Binding(
            get: {
                guard operation.parameters.count > 1 else { return .colorPlane }
                return CoreBatchSeparationDestination(rawValue: operation.parameters[1])
                    ?? .colorPlane
            },
            set: { value in
                while operation.parameters.count < 2 { operation.parameters.append(0) }
                operation.parameters[1] = value.rawValue
            }
        )
    }

    private var filterKind: Binding<CoreFilterKind> {
        Binding(get: { operation.filter?.kind ?? .invert }, set: { operation.filter?.kind = $0 })
    }

    private var filterPlaneID: Binding<UInt64> {
        Binding(get: { operation.filter?.planeID ?? 1 }, set: { operation.filter?.planeID = $0 })
    }

    private var filterChannel: Binding<CoreFilterChannel> {
        Binding(get: { operation.filter?.channel ?? .rgb }, set: { operation.filter?.channel = $0 })
    }

    private var filterInterpolation: Binding<CoreCurveInterpolation> {
        Binding(
            get: { operation.filter?.interpolation ?? .bezier },
            set: { operation.filter?.interpolation = $0 }
        )
    }

    private func filterParameter(_ index: Int) -> Binding<Int32> {
        Binding(
            get: { operation.filter?.parameters[index] ?? 0 },
            set: { operation.filter?.parameters[index] = $0 }
        )
    }

    private func filterCurveInput(_ index: Int) -> Binding<UInt32> {
        Binding(
            get: { operation.filter?.curvePoints[index].input ?? 0 },
            set: { operation.filter?.curvePoints[index].input = $0 }
        )
    }

    private func filterCurveOutput(_ index: Int) -> Binding<UInt32> {
        Binding(
            get: { operation.filter?.curvePoints[index].output ?? 0 },
            set: { operation.filter?.curvePoints[index].output = $0 }
        )
    }

    private func seedExpectedEnabled(_ index: Int) -> Binding<Bool> {
        Binding(
            get: { operation.seeds[index].expectedColor != nil },
            set: { enabled in
                operation.seeds[index].expectedColor = enabled
                    ? operation.seeds[index].fillColor : nil
            }
        )
    }

    private func seedExpectedColor(_ index: Int) -> Binding<CoreColorValue> {
        Binding(
            get: { operation.seeds[index].expectedColor ?? operation.seeds[index].fillColor },
            set: { operation.seeds[index].expectedColor = $0 }
        )
    }
}

private struct BatchColorValueEditor: View {
    let title: String
    @Binding var color: CoreColorValue

    var body: some View {
        GroupBox(title) {
            Picker("Depth", selection: depth) {
                ForEach(CoreColorDepth.allCases, id: \.rawValue) {
                    Text(String(describing: $0)).tag($0)
                }
            }
            HStack {
                TextField("R", value: component(\.red), format: .number)
                TextField("G", value: component(\.green), format: .number)
                TextField("B", value: component(\.blue), format: .number)
                TextField("A", value: component(\.alpha), format: .number)
            }
        }
    }

    private var depth: Binding<CoreColorDepth> {
        Binding(
            get: { color.depth },
            set: { newDepth in
                let maximum: UInt16 = switch newDepth {
                case .binary: 1
                case .grayscale8, .rgba8: UInt16(UInt8.max)
                case .grayscale16, .rgba16: UInt16.max
                }
                color = CoreColorValue(
                    depth: newDepth,
                    red: min(color.red, maximum),
                    green: min(color.green, maximum),
                    blue: min(color.blue, maximum),
                    alpha: min(color.alpha, maximum)
                )
            }
        )
    }

    private func component(_ keyPath: KeyPath<CoreColorValue, UInt16>) -> Binding<UInt16> {
        Binding(
            get: { color[keyPath: keyPath] },
            set: { value in
                color = CoreColorValue(
                    depth: color.depth,
                    red: keyPath == \.red ? value : color.red,
                    green: keyPath == \.green ? value : color.green,
                    blue: keyPath == \.blue ? value : color.blue,
                    alpha: keyPath == \.alpha ? value : color.alpha
                )
            }
        )
    }
}

private struct BatchPairResolutionView: View {
    @State private var draft: BatchPairResolutionDraft
    let onCancel: () -> Void
    let onApply: (BatchPairResolutionDraft) -> Void

    init(
        initial: BatchPairResolutionDraft,
        onCancel: @escaping () -> Void,
        onApply: @escaping (BatchPairResolutionDraft) -> Void
    ) {
        _draft = State(initialValue: initial)
        self.onCancel = onCancel
        self.onApply = onApply
    }

    var body: some View {
        VStack(alignment: .leading) {
            Text("Resolve color-pair ambiguity").font(.headline)
            Text("Choose one destination for each old color, or exclude it.")
                .foregroundStyle(.secondary)
            Form {
                ForEach($draft.groups) { $group in
                    Picker(colorLabel(group.oldColor), selection: $group.selection) {
                        Text("Exclude").tag(Int?.none)
                        ForEach(Array(group.candidates.enumerated()), id: \.offset) { index, item in
                            Text("\(colorLabel(item.newColor)) · \(item.pixelCount) px")
                                .tag(Optional(index))
                        }
                    }
                }
            }
            HStack {
                Button("Cancel", role: .cancel, action: onCancel)
                Spacer()
                Button("Add selected pairs") { onApply(draft) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 560, height: 420)
    }
}

private func colorLabel(_ color: CoreColorValue) -> String {
    "\(String(describing: color.depth)) \(color.red),\(color.green),\(color.blue),\(color.alpha)"
}

private extension CoreBatchOperationKind {
    var label: String {
        switch self {
        case .colorReplace: "Color replace"
        case .continuousFill: "Continuous fill"
        case .separation: "Color separation"
        case .visibility: "Visibility"
        case .lineWidth: "Line width"
        case .filter: "Filter"
        case .boundaryAirbrush: "Boundary airbrush"
        case .dustRemoval: "Dust removal"
        case .mirror: "Mirror"
        case .rotate90: "Rotate 90°"
        case .resize: "Image size / resolution"
        case .convertPlane: "Raster conversion"
        }
    }
}
