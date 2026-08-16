import Foundation
import Testing
@testable import InkpodCoreBridge

private final class ShortcutMemoryPersistence: ShortcutPersistence {
    var data: Data?
    var failWrites = false

    func load() throws -> Data? { data }

    func save(_ data: Data) throws {
        if failWrites {
            throw CocoaError(.fileWriteUnknown)
        }
        self.data = data
    }
}

private struct ShortcutTestRecord: Codable {
    let version: UInt32
    let bindings: [InkpodCommandID: ShortcutSequence]
}

@Suite("M10 command and shortcut contracts", .serialized)
@MainActor
struct CommandInfrastructureTests {
    @Test("document-state targets ignore view-only revision changes")
    func documentStateTargetMatching() {
        let workspaceID = WorkspaceID(
            rawValue: UUID(uuidString: "A1110000-0000-0000-0000-000000000099")!
        )
        let session = CoreSessionTarget(
            id: CoreSessionID(rawValue: 11),
            generation: CoreSessionGeneration(rawValue: 2)
        )
        let initial = CommandTargetContext(
            workspaceID: workspaceID,
            lifecycleGeneration: 3,
            session: session,
            view: CoreViewTarget(
                session: session,
                id: CoreViewID(rawValue: 21),
                generation: CoreViewGeneration(rawValue: 4)
            ),
            documentRevision: 5,
            viewRevision: 6
        )
        let resizedView = CommandTargetContext(
            workspaceID: workspaceID,
            lifecycleGeneration: 3,
            session: session,
            view: CoreViewTarget(
                session: session,
                id: CoreViewID(rawValue: 22),
                generation: CoreViewGeneration(rawValue: 7)
            ),
            documentRevision: 5,
            viewRevision: 8
        )
        let editedDocument = CommandTargetContext(
            workspaceID: workspaceID,
            lifecycleGeneration: 3,
            session: session,
            view: resizedView.view,
            documentRevision: 6,
            viewRevision: 8
        )

        #expect(resizedView.hasSameDocumentState(as: initial))
        #expect(!editedDocument.hasSameDocumentState(as: initial))
    }

    @Test("implemented M2 through M10 commands have one owner, state owner, and real surface")
    func commandCatalogCompleteness() {
        #expect(CommandCatalog.parityCommandIDs.count == 372)
        #expect(CommandCatalog.descriptors.count == InkpodCommandID.allCases.count)
        for command in CommandCatalog.parityCommandIDs {
            let descriptor = CommandCatalog.descriptor(for: command)
            #expect(!descriptor.surfaces.isEmpty)
            #expect([
                "MAC-COMMAND-SURFACE-001",
                "MAC-FILE-LIFECYCLE-001",
                "MAC-CLIPBOARD-001",
                "MAC-WORKSPACE-001",
                "MAC-CELL-WORKFLOW-001",
                "MAC-PAINT-FILL-001",
                "MAC-COLOR-WORKFLOW-001",
                "MAC-COLOR-OUTPUT-QA-001",
                "MAC-LOCATOR-001",
                "MAC-PAINT-SURFACE-001",
                "MAC-SELECTION-HISTORY-001",
                "MAC-FILTER-EFFECT-001",
                "MAC-VECTOR-WORKFLOW-001",
                "MAC-ANNOTATION-WORKFLOW-001",
                "MAC-FRAME-GUIDE-001",
                "MAC-RENDER-DIAGNOSTICS-001",
                "MAC-CUT-WORKFLOW-001",
                "MAC-SEQUENCE-STRUCTURE-001",
                "MAC-SEQUENCE-WORKFLOW-001",
                "MAC-SEQUENCE-ENDPOINT-001",
                "MAC-LIGHT-TABLE-001",
                "MAC-MOTION-SUBPALETTE-001",
                "MAC-ANIMATION-SURFACE-001",
                "MAC-BATCH-WORKFLOW-001",
            ].contains(descriptor.parityTestID))
        }
        #expect(CommandCatalog.descriptor(for: .fileSave).routeOwner == .fileLifecycle)
        #expect(CommandCatalog.descriptor(for: .editPaste).routeOwner == .edit)
        #expect(CommandCatalog.descriptor(for: .documentClose).routeOwner == .session)
        #expect(CommandCatalog.descriptor(for: .editorSplitRight).routeOwner == .workspace)
        #expect(CommandCatalog.descriptor(for: .layerNew).routeOwner == .cell)
        #expect(CommandCatalog.descriptor(for: .toolBrush).routeOwner == .tool)
        #expect(CommandCatalog.descriptor(for: .chartGenerate).routeOwner == .color)
        #expect(CommandCatalog.descriptor(for: .colorPin).targetScope == .pane)
        #expect(CommandCatalog.descriptor(for: .locatorPin).targetScope == .documentView)
        #expect(CommandCatalog.descriptor(for: .redo).routeOwner == .edit)
        #expect(CommandCatalog.descriptor(for: .selectionRectangle).stateOwner == .edit)
        #expect(CommandCatalog.descriptor(for: .filterToneCurve).routeOwner == .image)
        #expect(CommandCatalog.descriptor(for: .vectorLine).routeOwner == .tool)
        #expect(CommandCatalog.descriptor(for: .annotationAddText).routeOwner == .cell)
        #expect(CommandCatalog.descriptor(for: .viewVectorEndpoints).targetScope == .documentView)
        #expect(CommandCatalog.descriptor(for: .fileNewCut).routeOwner == .cut)
        #expect(CommandCatalog.descriptor(for: .cutSequenceRenumber).targetScope == .cutSession)
        #expect(CommandCatalog.descriptor(for: .lightTableBulkBoth).routeOwner == .animation)
        #expect(CommandCatalog.descriptor(for: .sequencePin).targetScope == .pane)
        #expect(CommandCatalog.descriptor(for: .batchPreview).routeOwner == .batch)
        #expect(CommandCatalog.descriptor(for: .batchCancel).targetScope == .job)
        #expect(CommandCatalog.descriptor(for: .windowBatch).surfaces == [.workspaceMenu])
        #expect(!CommandCatalog.descriptor(for: .zoomIn).surfaces.contains(.toolbar))
        #expect(CommandCatalog.descriptor(for: .zoomIn).surfaces.contains(.contextMenu))
    }

    @Test("pure ABI resolver distinguishes prefix, exact, none, and invalid")
    func pureResolver() {
        let first = ShortcutStroke.character("K", modifiers: .primary)
        let second = ShortcutStroke.character("G", modifiers: .primary)
        let sequence = ShortcutSequence([first, second])!
        let bindings: [InkpodCommandID: ShortcutSequence] = [.grid: sequence]

        #expect(ShortcutResolver.resolve(bindings: bindings, entered: [first]) == .prefix)
        #expect(ShortcutResolver.resolve(bindings: bindings, entered: [first, second]) == .exact(.grid))
        #expect(ShortcutResolver.resolve(bindings: bindings, entered: [.character("X")]) == .none)
        #expect(ShortcutResolver.resolve(bindings: bindings, entered: []) == .invalid)
    }

    @Test("exact conflicts exchange while strict prefix conflicts are transactional")
    func conflictRules() {
        let persistence = ShortcutMemoryPersistence()
        let controller = ShortcutController(persistence: persistence)
        let zoomIn = controller.sequence(for: .zoomIn)!
        let zoomOut = controller.sequence(for: .zoomOut)!

        #expect(controller.rebind(command: .zoomIn, to: zoomOut) == .applied)
        #expect(controller.sequence(for: .zoomIn) == zoomOut)
        #expect(controller.sequence(for: .zoomOut) == zoomIn)

        let prefix = ShortcutStroke.character("K", modifiers: .primary)
        let long = ShortcutSequence([prefix, .character("G", modifiers: .primary)])!
        #expect(controller.rebind(command: .grid, to: long) == .applied)
        let before = controller.bindings
        #expect(controller.rebind(command: .fit, to: ShortcutSequence([prefix])!) == .prefixConflict)
        #expect(controller.bindings == before)
    }

    @Test("pending sequence expires at 1.5 seconds and Cancel clears it")
    func timeoutAndCancel() {
        let controller = ShortcutController(persistence: ShortcutMemoryPersistence())
        let first = ShortcutStroke.character("K", modifiers: .primary)
        let second = ShortcutStroke.character("G", modifiers: .primary)
        #expect(controller.rebind(command: .grid, to: ShortcutSequence([first, second])!) == .applied)
        #expect(controller.consume(first, now: 10) == .prefix)
        #expect(controller.consume(second, now: 10 + ShortcutController.timeout) == .none)
        #expect(controller.pendingStrokes.isEmpty)
        #expect(controller.consume(first, now: 20) == .prefix)
        controller.cancelPending()
        #expect(controller.pendingStrokes.isEmpty)
    }

    @Test("reset is deterministic and persistence failure leaves bindings unchanged")
    func resetAndPersistenceFailure() {
        let persistence = ShortcutMemoryPersistence()
        let controller = ShortcutController(persistence: persistence)
        let replacement = ShortcutSequence([.character("2", modifiers: .primary)])!
        #expect(controller.rebind(command: .oneToOne, to: replacement) == .applied)
        #expect(controller.reset() == .applied)
        #expect(controller.bindings == ShortcutController.defaults)

        persistence.failWrites = true
        let before = controller.bindings
        #expect(controller.rebind(command: .grid, to: replacement) == .persistenceFailure)
        #expect(controller.bindings == before)
    }

    @Test("standard shortcuts and IME marked text are not intercepted")
    func standardAndIMEGuard() {
        let controller = ShortcutController(persistence: ShortcutMemoryPersistence())
        let newCell = ShortcutSequence([.character("N", modifiers: .primary)])!
        let copy = ShortcutSequence([.character("C", modifiers: .primary)])!
        #expect(controller.sequence(for: .fileNew) == newCell)
        #expect(controller.rebind(command: .grid, to: newCell) == .protectedStandard)
        #expect(controller.rebind(command: .grid, to: copy) == .protectedStandard)

        let plain = ShortcutStroke.character("A")
        let command = ShortcutStroke.character("A", modifiers: .primary)
        #expect(!ShortcutInputGuard.shouldHandle(isTextInput: true, hasMarkedText: true, stroke: command))
        #expect(!ShortcutInputGuard.shouldHandle(isTextInput: true, hasMarkedText: false, stroke: plain))
        #expect(!ShortcutInputGuard.shouldHandle(isTextInput: true, hasMarkedText: false, stroke: command))
        #expect(ShortcutInputGuard.shouldHandle(isTextInput: false, hasMarkedText: false, stroke: command))
    }

    @Test("standard New Cell and M7 history shortcuts migrate into a valid v2 record")
    func standardShortcutMigration() throws {
        let persistence = ShortcutMemoryPersistence()
        let open = ShortcutSequence([.character("O", modifiers: .primary)])!
        persistence.data = try JSONEncoder().encode(
            ShortcutTestRecord(version: 2, bindings: [.fileOpen: open])
        )

        let controller = ShortcutController(persistence: persistence)
        let newCell = ShortcutSequence([.character("N", modifiers: .primary)])!
        let undo = ShortcutSequence([.character("Z", modifiers: .primary)])!
        let redo = ShortcutSequence([.character("Z", modifiers: [.primary, .shift])])!
        #expect(controller.sequence(for: .fileOpen) == open)
        #expect(controller.sequence(for: .fileNew) == newCell)
        #expect(controller.sequence(for: .undo) == undo)
        #expect(controller.sequence(for: .redo) == redo)
        let saved = try #require(persistence.data)
        let migrated = try JSONDecoder().decode(ShortcutTestRecord.self, from: saved)
        #expect(migrated.bindings[.fileNew] == newCell)
        #expect(migrated.bindings[.undo] == undo)
        #expect(migrated.bindings[.redo] == redo)
    }

    @Test("language selection is versioned and takes effect on the next controller launch")
    func languageSelectionNextLaunch() throws {
        let suite = "com.inkpod.tests.language.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let running = AppLanguageController(defaults: defaults)
        let activeBeforeSelection = running.activeLanguageCode

        #expect(running.select(.japanese))
        #expect(running.selection == .japanese)
        #expect(running.activeLanguageCode == activeBeforeSelection)

        let relaunched = AppLanguageController(defaults: defaults)
        #expect(relaunched.selection == .japanese)
        #expect(relaunched.activeLanguageCode == "ja")
    }

    @Test("sequence endpoint policy is bounded, versioned, persistent, and defaults to stop")
    func sequenceEndpointPolicyPersistence() throws {
        let suite = "com.inkpod.tests.sequence-policy.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let controller = SequenceEndpointPolicyController(defaults: defaults)
        #expect(controller.policy == .stop)
        #expect(controller.set(.wrap))
        #expect(!controller.set(.wrap))
        #expect(SequenceEndpointPolicyController(defaults: defaults).policy == .wrap)

        defaults.set(Data(repeating: 0, count: 4_097), forKey: "sequence-endpoint-policy-v1")
        #expect(SequenceEndpointPolicyController(defaults: defaults).policy == .stop)
    }
}
