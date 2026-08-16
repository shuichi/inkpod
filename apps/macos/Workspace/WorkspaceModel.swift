import AppKit
import QuartzCore
import SwiftUI
import UniformTypeIdentifiers

private enum CoordinatedFileAction: Sendable {
    case save(CoreSessionTarget, UInt64, allowClean: Bool)
    case open(CoreSessionTarget, UInt64)
    case autosave(CoreSessionTarget, UInt64)
    case recovery(CoreSessionTarget, UInt64)
    case compact(CoreSessionTarget, UInt64, CoreCompactionToken)
}

enum WorkspaceFileOperationAlert: String, Identifiable, Equatable {
    case saveFailed

    var id: String { rawValue }

    var titleKey: String {
        switch self {
        case .saveFailed: "file.save.error.title"
        }
    }

    var messageKey: String {
        switch self {
        case .saveFailed: "file.save.error.body"
        }
    }
}

private extension PaneTargetRecord {
    var isPinned: Bool {
        if case .pinnedDocument = mode { return true }
        return false
    }
}

@MainActor
extension WorkspaceModel {
    var selectedCutMember: CoreCutMember? {
        guard let index = selectedCutMemberIndex,
              let cut, cut.members.indices.contains(index)
        else { return nil }
        return cut.members[index]
    }

    var selectedLightTableSet: CoreLightTableSetProjection? {
        lightTableAnimation?.lightTableSets.first { $0.id == selectedLightTableSetID }
    }

    var selectedLightTableItem: CoreLightTableItemProjection? {
        selectedLightTableSet?.items.first { $0.id == selectedLightTableItemID }
    }

    var endpointPolicy: CoreSequenceEndpointPolicy {
        application.sequenceEndpointPolicyController.policy
    }

    var sequenceAnimation: CoreAnimationProjection? {
        guard let context = m9PaneContext(for: .sequenceGoto) else { return nil }
        return animationProjections[context.session]
    }

    var lightTableAnimation: CoreAnimationProjection? {
        guard let context = m9PaneContext(for: .lightTableSetNew) else { return nil }
        return animationProjections[context.session]
    }

    var subpaletteAnimation: CoreAnimationProjection? {
        guard let context = m9PaneContext(for: .subpaletteSet) else { return nil }
        return animationProjections[context.session]
    }

    func m9CommandState(_ command: InkpodCommandID) -> CommandState {
        guard let context = m9PaneContext(for: command) else {
            return CommandState(enabled: false)
        }
        return commandState(command, context: context)
    }

    func m9Animation(for context: CommandTargetContext) -> CoreAnimationProjection? {
        animationProjections[context.session]
    }

    func refreshAnimation() {
        let contexts = [
            m9PaneContext(for: .sequenceGoto),
            m9PaneContext(for: .lightTableSetNew),
            m9PaneContext(for: .subpaletteSet),
        ].compactMap { $0 }.reduce(into: [CoreSessionTarget: CommandTargetContext]()) {
            $0[$1.session] = $1
        }
        guard !contexts.isEmpty else {
            animation = nil
            return
        }
        let generation = lifecycleGeneration
        for context in contexts.values {
            observe(
                application.coreHost.inspectAnimation(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision
                ),
                generation: generation
            ) { [weak self] outcome in
                guard let self else { return }
                switch outcome {
                case let .animation(state):
                    self.installAnimation(state)
                case .failed(.staleTarget):
                    self.lastCommandResult = .stale
                case let .failed(failure):
                    self.lastCommandResult = .failed(failure)
                default:
                    self.lastCommandResult = .invalid
                }
            }
        }
    }

    func selectCutMember(_ index: Int?) {
        guard let index else {
            selectedCutMemberIndex = nil
            return
        }
        guard let cut, cut.members.indices.contains(index) else {
            selectedCutMemberIndex = nil
            return
        }
        selectedCutMemberIndex = index
        activateCutMember(cut.members[index])
    }

    func selectLightTableSet(_ id: UInt64) {
        guard animation?.lightTableSets.contains(where: { $0.id == id }) == true else {
            return
        }
        selectedLightTableSetID = id
        selectedLightTableItemID = nil
        routeM9Animation(
            .editLightTable(.activateSet(id: id)),
            context: m9PaneContext(for: .lightTableSetNew)
        )
    }

    func selectLightTableItem(_ id: UInt64?) {
        selectedLightTableItemID = selectedLightTableSet?.items.contains {
            $0.id == id
        } == true ? id : nil
    }

    func activateSequenceCell(_ index: UInt32) {
        if let cut, let memberIndex = cut.members.indices.first(where: {
            UInt32($0) == index
        }), sequenceAnimation?.sequence.first(where: { $0.index == index })?.documentUUID
            == cut.members[memberIndex].documentUUID
        {
            selectCutMember(memberIndex)
            return
        }
        routeM9Animation(
            .activateSequence(index),
            context: m9PaneContext(for: .sequenceGoto)
        )
    }

    func performM9Pane(_ command: InkpodCommandID) {
        guard let context = m9PaneContext(for: command) else {
            lastCommandResult = .stale
            return
        }
        _ = execute(command, context: context)
    }

    func cancelM9Editor() {
        pendingM9Editor = nil
        lastCommandResult = .cancelled
    }

    func applyM9Editor(_ draft: M9EditorDraft) {
        switch draft {
        case let .cut(value):
            if value.createsCut {
                createCut(value)
            } else {
                updateCut(value)
            }
        case let .renumber(value):
            routeCut(
                application.coreHost.editCutSequence(
                    target: value.cutTarget,
                    expectedRevision: value.expectedCutRevision,
                    operations: [.renumber(
                        position: value.position,
                        count: value.count,
                        first: value.first,
                        step: value.step
                    )]
                )
            )
        case let .sequenceIndex(value):
            if m9PaneContext(for: .sequenceGoto) == value.context {
                activateSequenceCell(value.index)
            } else {
                routeM9Animation(.activateSequence(value.index), context: value.context)
            }
        case let .setName(value):
            routeM9Animation(
                .editLightTable(.renameSet(id: value.setID, name: value.name)),
                context: value.context
            )
        case let .opacity(value):
            routeM9Animation(
                .editLightTable(.setGlobalOpacity(value.opacityMilli)),
                context: value.context
            )
        case let .lightTableItem(value):
            routeM9Animation(.editLightTable(value.command), context: value.context)
        case let .bulk(value):
            routeM9Animation(
                .registerLightTableBulk(value.preview.request),
                context: value.context
            )
        }
        pendingM9Editor = nil
    }

    private func executeM9(
        _ command: InkpodCommandID,
        context: CommandTargetContext
    ) -> CommandRouteResult {
        let issuedAnimation = animationProjections[context.session]
        let previousIssueContext = m9IssueContext
        m9IssueContext = context
        defer { m9IssueContext = previousIssueContext }
        switch command {
        case .fileNewCut:
            pendingM9Editor = .cut(M9CutEditorDraft(
                createsCut: true,
                cutTarget: nil,
                expectedCutRevision: 0,
                cellCount: 1,
                metadata: CoreCutMetadata(cutName: "Cut 1", durationFrames: 24),
                defaults: CoreCutDefaults(
                    width: projection?.documentWidth ?? 1_920,
                    height: projection?.documentHeight ?? 1_080,
                    dpiXMilli: projection?.dpiXMilli ?? 72_000,
                    dpiYMilli: projection?.dpiYMilli ?? 72_000
                )
            ))
            return .presentedInput
        case .cutProperties:
            guard let cut else { return .noOp }
            pendingM9Editor = .cut(M9CutEditorDraft(
                createsCut: false,
                cutTarget: cut.target,
                expectedCutRevision: cut.revision,
                cellCount: UInt32(clamping: cut.members.count),
                metadata: cut.metadata,
                defaults: cut.defaults
            ))
            return .presentedInput
        case .cutSave:
            Task { _ = await saveCut(chooseDestination: cutURL == nil) }
            return .presentedInput
        case .cutUndo:
            guard let cut else { return .noOp }
            routeCut(application.coreHost.undoCut(
                target: cut.target,
                expectedRevision: cut.revision
            ))
        case .cutRedo:
            guard let cut else { return .noOp }
            routeCut(application.coreHost.redoCut(
                target: cut.target,
                expectedRevision: cut.revision
            ))
        case .cutSequenceAdd:
            addCurrentCellToCut()
        case .cutSequenceRemove:
            guard let member = selectedCutMember, let cut else { return .noOp }
            routeCut(application.coreHost.editCutSequence(
                target: cut.target,
                expectedRevision: cut.revision,
                operations: [.remove(
                    cellID: member.cellID,
                    documentUUID: member.documentUUID
                )]
            ))
        case .cutSequenceMoveUp:
            moveSelectedCutMember(delta: -1)
        case .cutSequenceMoveDown:
            moveSelectedCutMember(delta: 1)
        case .cutSequenceRenumber:
            guard let cut, !cut.members.isEmpty else { return .noOp }
            pendingM9Editor = .renumber(M9RenumberDraft(
                cutTarget: cut.target,
                expectedCutRevision: cut.revision,
                position: 0,
                count: UInt32(clamping: cut.members.count),
                first: 1,
                step: 1
            ))
            return .presentedInput
        case .sequenceImport:
            presentSequenceImportPanel(context: context)
            return .presentedInput
        case .sequenceExport:
            exportSequence(context: context)
            return .presentedInput
        case .sequencePrevious:
            stepSequence(.previous, context: context)
            return .presentedInput
        case .sequenceNext:
            stepSequence(.next, context: context)
            return .presentedInput
        case .sequenceGoto:
            pendingM9Editor = .sequenceIndex(M9IndexDraft(
                context: context,
                index: issuedAnimation?.activeSequenceIndex ?? 0
            ))
            return .presentedInput
        case .sequenceWrapEndpoints:
            return application.sequenceEndpointPolicyController.toggle() ? .started : .noOp
        case .lightTableSetNew:
            let number = (animation?.lightTableSets.count ?? 0) + 1
            routeM9Animation(.editLightTable(.createSet(name: "Light Table \(number)")))
        case .lightTableSetDuplicate:
            guard let set = selectedLightTableSet else { return .noOp }
            routeM9Animation(.editLightTable(.duplicateSet(
                id: set.id,
                name: "\(set.name) Copy"
            )))
        case .lightTableSetDelete:
            guard let set = selectedLightTableSet else { return .noOp }
            routeM9Animation(.editLightTable(.deleteSet(id: set.id)))
        case .lightTableSetRename:
            guard let set = selectedLightTableSet else { return .noOp }
            pendingM9Editor = .setName(M9NameDraft(
                context: context,
                setID: set.id,
                name: set.name
            ))
            return .presentedInput
        case .lightTableSetUp:
            reorderSelectedLightTableSet(delta: -1)
        case .lightTableSetDown:
            reorderSelectedLightTableSet(delta: 1)
        case .lightTableGlobalOpacity:
            guard let set = selectedLightTableSet else { return .noOp }
            pendingM9Editor = .opacity(M9OpacityDraft(
                context: context,
                opacityMilli: set.opacityMilli
            ))
            return .presentedInput
        case .lightTableItemAdd:
            presentLightTableRasterPanel(reloading: nil, context: context)
            return .presentedInput
        case .lightTableItemReload:
            guard let item = selectedLightTableItem else { return .noOp }
            presentLightTableRasterPanel(reloading: item.id, context: context)
            return .presentedInput
        case .lightTableItemDelete:
            guard let item = selectedLightTableItem else { return .noOp }
            routeM9Animation(.editLightTable(.removeItem(id: item.id)))
        case .lightTableItemUp:
            reorderSelectedLightTableItem(delta: -1)
        case .lightTableItemDown:
            reorderSelectedLightTableItem(delta: 1)
        case .lightTableItemProperties, .lightTableItemMove:
            guard let item = selectedLightTableItem else { return .noOp }
            pendingM9Editor = .lightTableItem(M9LightTableItemDraft(
                item,
                context: context
            ))
            return .presentedInput
        case .lightTableItemSample:
            routeM9Animation(.sampleLightTable(
                x: (projection?.documentWidth ?? 1) / 2,
                y: (projection?.documentHeight ?? 1) / 2
            ))
        case .lightTableItemSwap:
            guard let item = selectedLightTableItem else { return .noOp }
            routeM9Animation(.swapLightTable(itemID: item.id))
        case .lightTableBulkPrevious:
            previewLightTableBulk(.previous)
            return .presentedInput
        case .lightTableBulkNext:
            previewLightTableBulk(.next)
            return .presentedInput
        case .lightTableBulkBoth:
            previewLightTableBulk(.both)
            return .presentedInput
        case .subpaletteSet:
            routeM9Animation(.setSubpalette(issuedAnimation?.activeSequenceIndex ?? 0))
        case .subpaletteSample:
            routeM9Animation(.sampleSubpalette(
                x: (projection?.documentWidth ?? 1) / 2,
                y: (projection?.documentHeight ?? 1) / 2
            ))
        case .motionStart:
            startMotion(context: context)
            return .presentedInput
        case .motionPause:
            routeM9Animation(.motionTogglePause)
        case .motionPrevious:
            routeM9Animation(.motionStep(.previous))
        case .motionNext:
            routeM9Animation(.motionStep(.next))
        case .motionStop:
            stopMotion()
        case .motionFirst:
            activateSequenceCell(0)
        case .motionLast:
            guard let count = issuedAnimation?.sequence.count, count > 0 else { return .noOp }
            activateSequenceCell(UInt32(clamping: count - 1))
        case .motionFPS30, .motionFPS25, .motionFPS24, .motionFPS12,
             .motionFPS10, .motionFPS8:
            motionFPS = motionFPSValue(for: command)
        case .windowSequence:
            sequenceVisible.toggle()
        case .sequencePin:
            toggleM9Pane(.sequence, context: context)
        case .windowLightTable:
            if inspectorSectionIsActive(.animation) {
                lightTableVisible.toggle()
            } else {
                let wasVisible = lightTableVisible
                let chromeResult = reduceChrome(.showInspectorSection(.animation))
                if chromeResult == .stale { return .stale }
                lightTableVisible = true
                if wasVisible, chromeResult == .noOp { return .noOp }
            }
        case .lightTablePin:
            toggleM9Pane(.lightTable, context: context)
        case .windowSubpalette:
            if inspectorSectionIsActive(.animation) {
                subpaletteVisible.toggle()
            } else {
                let wasVisible = subpaletteVisible
                let chromeResult = reduceChrome(.showInspectorSection(.animation))
                if chromeResult == .stale { return .stale }
                subpaletteVisible = true
                if wasVisible, chromeResult == .noOp { return .noOp }
            }
        case .subpalettePin:
            toggleM9Pane(.subpalette, context: context)
        default:
            return .invalid
        }
        return .started
    }
}

private extension CoreFillOptions {
    func replacingOperation(_ operation: CoreFillOperation) -> CoreFillOptions {
        CoreFillOptions(
            operation: operation,
            detachedRegions: detachedRegions,
            overflowAbort: overflowAbort,
            transparentOnly: transparentOnly,
            useDocumentSelection: useDocumentSelection,
            useLightTableBoundary: useLightTableBoundary,
            useLightTableColor: useLightTableColor,
            tolerance: tolerance,
            gapClose: gapClose,
            inclusionMode: inclusionMode,
            extensionDistance: extensionDistance,
            inclusionColors: inclusionColors
        )
    }
}

private extension CoreSelectionOptions {
    func replacingShape(_ shape: CoreSelectionShape) -> CoreSelectionOptions {
        CoreSelectionOptions(
            shape: shape,
            operation: operation,
            tolerance: tolerance,
            gapClose: gapClose,
            diameter: diameter,
            interpretation: interpretation,
            aspectRatio: aspectRatio,
            fromCenter: fromCenter,
            constrainRotationTo45Degrees: constrainRotationTo45Degrees,
            pressureControlsSize: pressureControlsSize,
            screenSizedTrace: screenSizedTrace,
            rotationTurns: rotationTurns,
            traceShape: traceShape
        )
    }

    func replacingOperation(_ operation: CoreSelectionOperation) -> CoreSelectionOptions {
        CoreSelectionOptions(
            shape: shape,
            operation: operation,
            tolerance: tolerance,
            gapClose: gapClose,
            diameter: diameter,
            interpretation: interpretation,
            aspectRatio: aspectRatio,
            fromCenter: fromCenter,
            constrainRotationTo45Degrees: constrainRotationTo45Degrees,
            pressureControlsSize: pressureControlsSize,
            screenSizedTrace: screenSizedTrace,
            rotationTurns: rotationTurns,
            traceShape: traceShape
        )
    }
}

private enum PaintDataKind {
    case palette
    case chart
}

private enum PaletteMutation {
    case registerCurrent
    case deleteCurrent
    case clear
}

enum DirtyCloseDecision: Sendable {
    case save
    case discard
    case cancel
}

enum ColorReplaceRegionTool: String, CaseIterable, Sendable {
    case pen
    case rectangle
    case polyline
    case lasso
    case all
}

struct M8FilterDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    let planeID: UInt64
    let adjustmentLayerID: UInt64?
    let createsAdjustment: Bool
    var kind: CoreFilterKind
    var channel = CoreFilterChannel.rgb
    var interpolation = CoreCurveInterpolation.bezier
    var parameters: [Int32]
    var curvePoints: [CoreCurvePoint]

    var request: CoreFilterRequest {
        CoreFilterRequest(
            kind: kind,
            planeID: planeID,
            channel: channel,
            interpolation: interpolation,
            parameters: parameters,
            curvePoints: curvePoints
        )
    }
}

struct M8EffectDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    let planeID: UInt64
    let command: InkpodCommandID
    var primary: Double = 16
    var secondary: Double = 500
    var maximumPixels: UInt32 = 64
}

struct M8GeometryOptionsDraft: Identifiable, Equatable {
    let id = UUID()
    var options = CoreGeometryOptions()
    var outlineWidth: Double = 2
    var polygonSides: UInt32 = 5
}

struct M8AnnotationDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    let layerID: UInt64
    let objectID: UInt64?
    var text: String
    var instructionOnly: Bool
    var fontFamily: String
    var fontSize: Double
    var x: Int32
    var y: Int32
    var width: Int32
    var height: Int32
}

struct M8ShootingFrameDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    var frame: CoreShootingFrame
}

struct M8VanishingPointDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    var point: CoreVanishingPoint
}

enum M8EditorDraft: Identifiable, Equatable {
    case filter(M8FilterDraft)
    case effect(M8EffectDraft)
    case geometry(M8GeometryOptionsDraft)
    case annotation(M8AnnotationDraft)
    case shootingFrame(M8ShootingFrameDraft)
    case vanishingPoint(M8VanishingPointDraft)

    var id: UUID {
        switch self {
        case let .filter(value): value.id
        case let .effect(value): value.id
        case let .geometry(value): value.id
        case let .annotation(value): value.id
        case let .shootingFrame(value): value.id
        case let .vanishingPoint(value): value.id
        }
    }
}

struct M9CutEditorDraft: Identifiable, Equatable {
    let id = UUID()
    let createsCut: Bool
    let cutTarget: CoreCutTarget?
    let expectedCutRevision: UInt64
    var cellCount: UInt32
    var metadata: CoreCutMetadata
    var defaults: CoreCutDefaults
}

struct M9RenumberDraft: Identifiable, Equatable {
    let id = UUID()
    let cutTarget: CoreCutTarget
    let expectedCutRevision: UInt64
    var position: UInt32
    var count: UInt32
    var first: UInt32
    var step: UInt32
}

struct M9IndexDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    var index: UInt32
}

struct M9NameDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    let setID: UInt64
    var name: String
}

struct M9OpacityDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    var opacityMilli: UInt32
}

struct M9LightTableItemDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    let itemID: UInt64
    var name: String
    var opacityMilli: UInt32
    var displayMode: CoreLightTableDisplayMode
    var displayColor: CoreColorValue
    var isVisible: Bool
    var translateXMilli: Int32
    var translateYMilli: Int32
    var scaleXMilli: UInt32
    var scaleYMilli: UInt32
    var rotationMilliDegrees: Int32

    init(_ item: CoreLightTableItemProjection, context: CommandTargetContext) {
        self.context = context
        itemID = item.id
        name = item.name
        opacityMilli = item.opacityMilli
        displayMode = item.displayMode
        displayColor = item.displayColor
        isVisible = item.isVisible
        translateXMilli = item.translateXMilli
        translateYMilli = item.translateYMilli
        scaleXMilli = item.scaleXMilli
        scaleYMilli = item.scaleYMilli
        rotationMilliDegrees = item.rotationMilliDegrees
    }

    var command: CoreLightTableEditCommand {
        .updateItem(
            id: itemID,
            name: name,
            opacityMilli: opacityMilli,
            displayMode: displayMode,
            displayColor: displayColor,
            isVisible: isVisible,
            translateXMilli: translateXMilli,
            translateYMilli: translateYMilli,
            scaleXMilli: scaleXMilli,
            scaleYMilli: scaleYMilli,
            rotationMilliDegrees: rotationMilliDegrees
        )
    }
}

struct M9LightTableBulkDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    let preview: CoreLightTableBulkPreview
}

enum M9EditorDraft: Identifiable, Equatable {
    case cut(M9CutEditorDraft)
    case renumber(M9RenumberDraft)
    case sequenceIndex(M9IndexDraft)
    case setName(M9NameDraft)
    case opacity(M9OpacityDraft)
    case lightTableItem(M9LightTableItemDraft)
    case bulk(M9LightTableBulkDraft)

    var id: AnyHashable {
        switch self {
        case let .cut(value): value.id
        case let .renumber(value): value.id
        case let .sequenceIndex(value): value.id
        case let .setName(value): value.id
        case let .opacity(value): value.id
        case let .lightTableItem(value): value.id
        case let .bulk(value): value.id
        }
    }
}

enum M8CanvasTool: Equatable {
    case geometry(CoreGeometryPrimitive)
    case vectorEraser(CoreVectorEraseMode)
    case annotationStroke
    case shootingFrameHandles
    case vanishingPointHandles
}

private struct M8CanvasGesture {
    let context: CommandTargetContext
    let viewID: WorkspaceViewID
    let tool: M8CanvasTool
    let start: CorePointerSample
    var samples: [CorePointerSample]
}

private struct M8AnnotationSeed {
    let layerID: UInt64
    let text: String
    let instructionOnly: Bool
    let fontFamily: String
    let fontSize: Double
    let bounds: CoreFrameRect
}

private struct CanvasPaintGesture {
    let viewID: WorkspaceViewID
    let target: CoreViewTarget
    let tool: CoreEditorTool
    let expectation: CorePaintExpectation
    let start: CorePointerSample
    var samples: [CorePointerSample]
}

private struct WorkspaceInspectorTarget: Equatable {
    let workspaceID: WorkspaceID
    let lifecycleGeneration: UInt64
    let session: CoreSessionTarget
    let view: CoreViewTarget

    init?(_ context: CommandTargetContext?) {
        guard let context else { return nil }
        workspaceID = context.workspaceID
        lifecycleGeneration = context.lifecycleGeneration
        session = context.session
        view = context.view
    }

    func matches(_ context: CommandTargetContext?) -> Bool {
        guard let context else { return false }
        return workspaceID == context.workspaceID
            && lifecycleGeneration == context.lifecycleGeneration
            && session == context.session
            && view == context.view
    }
}

enum WorkspaceChromeReductionResult: Equatable {
    case changed
    case noOp
    case stale
}

@MainActor
final class WorkspaceModel: ObservableObject {
    enum Phase: Equatable {
        case idle
        case starting
        case ready
        case failed(CoreHostFailure)
        case stopped
    }

    let id: WorkspaceID
    @Published private(set) var phase: Phase = .idle
    @Published private(set) var projection: CoreSessionProjection? {
        didSet {
            window?.isDocumentEdited = projection?.isDirty == true || cut?.isDirty == true
        }
    }
    @Published private(set) var presentedFrameCount: UInt64 = 0
    @Published private(set) var presentationState = ViewPresentationState()
    @Published private(set) var currentZoom = 1.0
    @Published private(set) var currentPanX = 0.0
    @Published private(set) var currentPanY = 0.0
    @Published private(set) var editorGraph: WorkspaceEditorGraph?
    @Published private(set) var cellTree: CoreTreeProjection?
    @Published private(set) var paint: CorePaintProjection?
    @Published private(set) var locator: CoreLocatorProjection?
    @Published private(set) var colorChartPreview: CoreColorChartPreviewProjection?
    @Published private(set) var colorReplacePreview: CoreColorReplacePreviewProjection?
    @Published var colorInspectorVisible = true
    @Published var locatorVisible = true
    @Published var eyedropperSource = CoreEyedropperSource.composite
    @Published var colorReplaceRegionTool = ColorReplaceRegionTool.rectangle
    @Published var colorReplaceTarget = CoreColorValue.rgba8(red: 0, green: 0, blue: 0)
    @Published var locatorFixed = false
    @Published var locatorAutoscroll = false
    @Published private(set) var colorPaneTarget = PaneTargetRecord.following
    @Published private(set) var locatorPaneTarget = PaneTargetRecord.following
    @Published var chartSearchText = ""
    @Published private(set) var chartCursor: UInt64?
    @Published private(set) var palettePage: UInt32 = 0
    @Published private(set) var chartPage: UInt32 = 0
    @Published private(set) var layerPaneTarget = PaneTargetRecord.following
    @Published private(set) var layerPaneAccessibilityNotice: PaneTargetNotice?
    @Published private(set) var chromePreference = WorkspaceChromePreference.defaultColoring
    @Published private(set) var adaptiveChrome = AdaptiveChromeState.project(
        .defaultColoring,
        availableWidth: 1_200
    )
    @Published private(set) var toolOptionsPresentation = WorkspaceToolOptionsPresentation.closed
    @Published var pendingNewCellDraft: NewCellDraft?
    @Published var pendingNewCellPlan: CoreCellCreationPlanProjection?
    @Published var pendingCellEditor: CellEditorDraft?
    @Published var pendingTreeEditor: TreeEditorDraft?
    @Published private(set) var workspaceLayout = WorkspaceLayoutRecord.defaultColoring
    @Published var pendingCommandInput: WorkspaceCommandInput?
    @Published private(set) var lastCommandResult: CommandRouteResult = .noOp
    @Published private(set) var fileOperationAlert: WorkspaceFileOperationAlert?
    @Published private(set) var documentURL: URL?
    @Published private(set) var isFileOperationActive = false
    @Published var pendingPasteConfirmation: PasteConfirmation?
    @Published var pendingFloatingTransform: FloatingTransformDraft?
    @Published var floatingTransformEditor: FloatingTransformDraft?
    @Published private(set) var history: CoreHistoryProjection?
    @Published private(set) var historyRows: [CoreHistoryVisualizationRow] = []
    @Published private(set) var historyProgress: CoreHistoryVisualizationProgressProjection?
    @Published var pendingRecoveryDecision: RecoveryCandidate?
    @Published private(set) var m8State: CoreM8Projection?
    @Published var pendingM8Editor: M8EditorDraft?
    @Published private(set) var activeM8CanvasTool: M8CanvasTool?
    @Published private(set) var selectedAnnotationID: UInt64?
    @Published private(set) var vectorSelection: CoreVectorSelectionProjection?
    @Published private(set) var cut: CoreCutProjection? {
        didSet {
            window?.isDocumentEdited = projection?.isDirty == true || cut?.isDirty == true
        }
    }
    @Published private(set) var animation: CoreAnimationProjection?
    @Published private var animationProjections: [CoreSessionTarget: CoreAnimationProjection] = [:]
    @Published var pendingM9Editor: M9EditorDraft?
    @Published var selectedCutMemberIndex: Int?
    @Published var selectedLightTableSetID: UInt64?
    @Published var selectedLightTableItemID: UInt64?
    @Published var sequenceVisible = true
    @Published var lightTableVisible = true
    @Published var subpaletteVisible = true
    @Published var referenceVisible = true
    @Published private(set) var sequencePaneTarget = PaneTargetRecord.following
    @Published private(set) var lightTablePaneTarget = PaneTargetRecord.following
    @Published private(set) var subpalettePaneTarget = PaneTargetRecord.following
    @Published private(set) var motionFPS: UInt32 = 24
    @Published private(set) var motionLoops = false

    private unowned let application: ApplicationCoordinator
    private var surfaces: [WorkspaceViewID: CoreSurfaceTarget] = [:]
    private var routes: [WorkspaceViewID: CoreSnapshotRoute] = [:]
    private var drawableSizes: [WorkspaceViewID: CGSize] = [:]
    private var sessionProjections: [CoreSessionTarget: CoreSessionProjection] = [:]
    private var treeProjections: [CoreSessionTarget: CoreTreeProjection] = [:]
    private var paintProjections: [CoreSessionTarget: CorePaintProjection] = [:]
    private var lifecycleGeneration: UInt64 = 1
    private var stopping = false
    private var drawableSize = CGSize.zero
    private var chromeAvailableWidth = 1_200.0
    private var suspendedInspectorTarget: WorkspaceInspectorTarget?
    private var inspectorRestorationBlocked = false
    private var guidePositions: [UInt64: Int32] = [:]
    private var lastAffectedGuideID: UInt64?
    private weak var window: NSWindow?
    private var fileIdentity: FileIdentity?
    private var sessionDocumentURLs: [CoreSessionTarget: URL] = [:]
    private var sessionFileIdentities: [CoreSessionTarget: FileIdentity] = [:]
    private let layoutStore = WorkspaceLayoutStore()
    private var recoveryURL: URL?
    private var pendingCellEditorContext: CommandTargetContext?
    private var pendingTreeEditorContext: CommandTargetContext?
    private var canvasPaintGesture: CanvasPaintGesture?
    private var m8CanvasGesture: M8CanvasGesture?
    private var m8GeometryOptions = M8GeometryOptionsDraft()
    private var vectorEraseMode = CoreVectorEraseMode.partial
    private var vectorSelectionMode = CoreVectorSelectionMode.touching
    private var knownAnnotationIDs: [UInt64] = []
    private var annotationSeeds: [UInt64: M8AnnotationSeed] = [:]
    private var filterPreviewDelay: Task<Void, Never>?
    private var filterPreviewRequestID: CoreRequestID?
    private var pendingFilterPreview: M8FilterDraft?
    private var pendingFilterApply: M8FilterDraft?
    private var filterPreviewStarted = false
    private var historyVisualization: CoreHistoryVisualizationID?
    private var chartClipboard: [CoreColorChartEntry] = []
    private var cutURL: URL?
    private var motionTask: Task<Void, Never>?
    private var m9IssueContext: CommandTargetContext?
    var dirtyCloseDecisionForTesting: (() async -> DirtyCloseDecision)?
    var filePanelResponseForTesting: ((NSSavePanel) async -> NSApplication.ModalResponse)?

    init(id: WorkspaceID, application: ApplicationCoordinator) {
        self.id = id
        self.application = application
    }

    var toolSidebarVisible: Bool {
        adaptiveChrome.toolPresentation != .hidden
    }

    var activePaintEditor: CoreEditorProjection? {
        guard let session = commandContext?.session else { return nil }
        return paintProjections[session]?.editor
    }

    var layerInspectorVisible: Bool {
        adaptiveChrome.inspectorVisible
            && chromePreference.selectedInspectorSection == .layerPlane
    }

    var inspectorOnLeadingEdge: Bool {
        chromePreference.inspectorEdge == .leading
    }

    var inspectorEffectivelyVisible: Bool {
        adaptiveChrome.inspectorVisible
    }

    var visibleCanvasWidth: Double {
        adaptiveChrome.canvasWidth
    }

    func inspectorSectionIsActive(_ section: WorkspaceInspectorSection) -> Bool {
        adaptiveChrome.inspectorVisible && chromePreference.selectedInspectorSection == section
    }

    @discardableResult
    func reduceChrome(_ action: WorkspaceChromeAction) -> WorkspaceChromeReductionResult {
        let wasVisible = adaptiveChrome.inspectorVisible
        let previousSection = chromePreference.selectedInspectorSection
        var replacementPreference = chromePreference
        guard replacementPreference.reduce(action) else { return .noOp }
        let explicitlyDismissesInspector = actionDismissesInspector(
            action,
            replacementPreference: replacementPreference
        )
        if explicitlyDismissesInspector {
            inspectorRestorationBlocked = false
        }
        let projectedAdaptive = AdaptiveChromeState.project(
            replacementPreference,
            availableWidth: chromeAvailableWidth
        )
        var replacementAdaptive = projectedAdaptive
        if !wasVisible, projectedAdaptive.inspectorVisible,
           let suspendedInspectorTarget,
           !suspendedInspectorTarget.matches(commandContext)
        {
            inspectorRestorationBlocked = true
            if actionPresentsInspector(action, replacementPreference: replacementPreference) {
                lastCommandResult = .stale
                return .stale
            }
            replacementAdaptive = suppressInspector(
                in: projectedAdaptive,
                availableWidth: chromeAvailableWidth
            )
        } else if inspectorRestorationBlocked, projectedAdaptive.inspectorVisible {
            if let suspendedInspectorTarget,
               suspendedInspectorTarget.matches(commandContext)
            {
                inspectorRestorationBlocked = false
            } else {
                replacementAdaptive = suppressInspector(
                    in: projectedAdaptive,
                    availableWidth: chromeAvailableWidth
                )
            }
        } else if projectedAdaptive.inspectorVisible {
            inspectorRestorationBlocked = false
        }
        chromePreference = replacementPreference
        workspaceLayout.chrome = chromePreference
        adaptiveChrome = replacementAdaptive
        if adaptiveChrome.toolPresentation == .hidden {
            _ = closeToolOptions()
        }
        handleInspectorProjectionTransition(wasVisible: wasVisible)
        if explicitlyDismissesInspector, !wasVisible, !adaptiveChrome.inspectorVisible {
            suspendedInspectorTarget = WorkspaceInspectorTarget(commandContext)
        }
        if wasVisible, adaptiveChrome.inspectorVisible,
           previousSection == .history,
           chromePreference.selectedInspectorSection != .history
        {
            cancelHistoryVisualization()
        }
        if case .selectInspectorSection = action, wasVisible, adaptiveChrome.inspectorVisible {
            refreshSelectedInspectorOnce()
        } else if case .showInspectorSection = action, wasVisible,
                  adaptiveChrome.inspectorVisible
        {
            refreshSelectedInspectorOnce()
        } else if case .toggleInspectorSection = action, wasVisible,
                  adaptiveChrome.inspectorVisible
        {
            refreshSelectedInspectorOnce()
        }
        return .changed
    }

    private func reduceChromeCommand(_ action: WorkspaceChromeAction) -> CommandRouteResult {
        switch reduceChrome(action) {
        case .changed: .started
        case .noOp: .noOp
        case .stale: .stale
        }
    }

    func updateAdaptiveChrome(availableWidth: Double) {
        guard availableWidth.isFinite, availableWidth > 0 else { return }
        let wasVisible = adaptiveChrome.inspectorVisible
        chromeAvailableWidth = availableWidth
        let projected = AdaptiveChromeState.project(
            chromePreference,
            availableWidth: availableWidth
        )
        var replacement = projected
        if !wasVisible, projected.inspectorVisible,
           let suspendedInspectorTarget,
           !suspendedInspectorTarget.matches(commandContext)
        {
            inspectorRestorationBlocked = true
            lastCommandResult = .stale
            replacement = suppressInspector(in: projected, availableWidth: availableWidth)
        } else if inspectorRestorationBlocked, projected.inspectorVisible {
            if let suspendedInspectorTarget,
               suspendedInspectorTarget.matches(commandContext)
            {
                inspectorRestorationBlocked = false
            } else {
                replacement = suppressInspector(in: projected, availableWidth: availableWidth)
            }
        } else if projected.inspectorVisible {
            inspectorRestorationBlocked = false
        }
        guard replacement != adaptiveChrome else { return }
        adaptiveChrome = replacement
        if adaptiveChrome.toolPresentation == .hidden {
            _ = closeToolOptions()
        }
        handleInspectorProjectionTransition(wasVisible: wasVisible)
    }

    private func actionPresentsInspector(
        _ action: WorkspaceChromeAction,
        replacementPreference: WorkspaceChromePreference
    ) -> Bool {
        switch action {
        case .toggleInspector, .toggleInspectorSection:
            replacementPreference.inspectorRequestedVisible
        case let .setInspectorPresented(isPresented):
            isPresented
        case .showInspectorSection:
            true
        case let .restore(preference):
            preference.inspectorRequestedVisible
        case .toggleToolSurface, .setToolPresentation,
             .selectInspectorSection, .mirrorEdges, .setInspectorWidth:
            false
        }
    }

    private func actionDismissesInspector(
        _ action: WorkspaceChromeAction,
        replacementPreference: WorkspaceChromePreference
    ) -> Bool {
        switch action {
        case .toggleInspector, .toggleInspectorSection:
            !replacementPreference.inspectorRequestedVisible
        case let .setInspectorPresented(isPresented):
            !isPresented
        case let .restore(preference):
            !preference.inspectorRequestedVisible
        case .toggleToolSurface, .setToolPresentation,
             .selectInspectorSection, .showInspectorSection, .mirrorEdges,
             .setInspectorWidth:
            false
        }
    }

    private func suppressInspector(
        in projection: AdaptiveChromeState,
        availableWidth: Double
    ) -> AdaptiveChromeState {
        AdaptiveChromeState(
            toolPresentation: projection.toolPresentation,
            inspectorVisible: false,
            canvasWidth: max(0, availableWidth - projection.toolPresentation.width)
        )
    }

    func setInspectorPresentedFromFramework(_ isPresented: Bool) {
        guard isPresented != adaptiveChrome.inspectorVisible else { return }
        _ = reduceChrome(.setInspectorPresented(isPresented))
    }

    func selectInspectorSection(_ section: WorkspaceInspectorSection) {
        _ = reduceChrome(.selectInspectorSection(section))
    }

    private func handleInspectorProjectionTransition(wasVisible: Bool) {
        let isVisible = adaptiveChrome.inspectorVisible
        guard wasVisible != isVisible else { return }
        if isVisible {
            guard let suspendedInspectorTarget else {
                refreshSelectedInspectorOnce()
                return
            }
            self.suspendedInspectorTarget = nil
            guard suspendedInspectorTarget.matches(commandContext) else {
                lastCommandResult = .stale
                return
            }
            refreshSelectedInspectorOnce()
        } else {
            suspendedInspectorTarget = WorkspaceInspectorTarget(commandContext)
            if chromePreference.selectedInspectorSection == .history {
                cancelHistoryVisualization()
            }
        }
    }

    private func refreshSelectedInspectorOnce() {
        switch chromePreference.selectedInspectorSection {
        case .layerPlane:
            refreshTree()
        case .color:
            refreshPaint()
        case .history:
            refreshHistory(rebuildVisualization: true)
        case .vectorAnnotationGuides:
            refreshM8()
        case .animation:
            refreshAnimation()
        }
    }

    func start(opening startupItem: StartupWorkspaceItem? = nil) {
        guard phase == .idle else { return }
        phase = .starting
        if let transfer = application.takePendingViewTransfer(for: id) {
            installTransferredInitial(transfer)
            phase = .ready
            refreshTree()
            refreshPaint()
            refreshHistory(rebuildVisualization: true)
            refreshM8()
            refreshAnimation()
            return
        }
        let generation = lifecycleGeneration
        observe(
            application.coreHost.createSession(documentUUID: id.coreDocumentUUID),
            generation: generation
        ) { [weak self] outcome in
            guard let self else { return }
            switch outcome {
            case let .created(projection), let .noOp(.some(projection)):
                self.installInitialSession(projection)
                self.application.claimSession(projection.target, for: self.id)
                self.phase = .ready
                self.refreshTree()
                self.refreshPaint()
                self.refreshHistory(rebuildVisualization: true)
                self.refreshM8()
                self.refreshAnimation()
                switch startupItem {
                case let .document(url):
                    Task { await self.openURL(url, recovery: false) }
                case let .recovery(candidate):
                    self.pendingRecoveryDecision = candidate
                case nil:
                    break
                }
            case let .failed(failure):
                self.phase = .failed(failure)
            default:
                self.phase = .failed(.invalidRequest)
            }
        }
    }

    func registerCanvas(
        viewID: WorkspaceViewID,
        layer: CAMetalLayer,
        drawableSize: CGSize
    ) -> CoreSurfaceTarget? {
        guard phase == .ready,
              surfaces[viewID] == nil,
              let view = editorGraph?.allViews.first(where: { $0.id == viewID }),
              let surface = application.allocateSurfaceTarget()
        else {
            return nil
        }
        let route = CoreSnapshotRoute(
            session: view.session,
            view: view.coreTarget,
            surface: surface
        )
        guard application.rendererHost.registerSurface(
            route: route,
            layer: layer,
            drawableSize: drawableSize
        ) else {
            return nil
        }
        surfaces[viewID] = surface
        routes[viewID] = route
        drawableSizes[viewID] = drawableSize
        self.drawableSize = drawableSize
        viewportChanged(drawableSize, viewID: viewID)
        return surface
    }

    func registerCanvas(layer: CAMetalLayer, drawableSize: CGSize) -> CoreSurfaceTarget? {
        guard let viewID = editorGraph?.activeView?.id else { return nil }
        return registerCanvas(viewID: viewID, layer: layer, drawableSize: drawableSize)
    }

    func registerWindow(_ window: NSWindow) {
        self.window = window
        window.isDocumentEdited = projection?.isDirty == true
        application.registerWindow(
            window,
            workspaceID: id,
            lifecycleGeneration: lifecycleGeneration
        )
        if let restored = layoutStore.load(
            visibleWorkAreas: NSScreen.screens.map(\.visibleFrame)
        ) {
            applyWorkspaceLayout(restored, to: window)
        }
    }

    func unregisterWindow(_ window: NSWindow) {
        application.unregisterWindow(window, workspaceID: id)
        if self.window === window { self.window = nil }
    }

    func unregisterCanvas(_ target: CoreSurfaceTarget) {
        guard let pair = surfaces.first(where: { $0.value == target }) else { return }
        cancelStroke(viewID: pair.key)
        _ = application.rendererHost.unregisterSurface(target)
        surfaces.removeValue(forKey: pair.key)
        routes.removeValue(forKey: pair.key)
        drawableSizes.removeValue(forKey: pair.key)
    }

    func viewportChanged(_ drawableSize: CGSize, viewID: WorkspaceViewID? = nil) {
        guard let view = viewRecord(viewID) else { return }
        self.drawableSize = drawableSize
        drawableSizes[view.id] = drawableSize
        observeCoreMutation(
            application.coreHost.applyView(
                target: view.coreTarget,
                command: .viewportResized(
                    width: drawableSize.width,
                    height: drawableSize.height
                )
            ),
            requestsSnapshot: true,
            viewID: view.id
        )
    }

    func setCanvasVisible(_ visible: Bool, viewID: WorkspaceViewID? = nil) {
        guard let view = viewRecord(viewID), let surface = surfaces[view.id] else { return }
        application.rendererHost.setSurfaceVisible(surface, visible: visible)
        if visible {
            requestSnapshot(viewID: view.id)
        }
    }

    func displayOrBackingChanged(_ drawableSize: CGSize, viewID: WorkspaceViewID? = nil) {
        guard let view = viewRecord(viewID), let surface = surfaces[view.id] else { return }
        application.rendererHost.resizeSurface(surface, drawableSize: drawableSize)
        application.rendererHost.handleDisplayOrDeviceChange(surface)
        self.drawableSize = drawableSize
        viewportChanged(drawableSize, viewID: view.id)
    }

    func beginStroke(_ sample: CorePointerSample, viewID: WorkspaceViewID? = nil) {
        guard canvasPaintGesture == nil,
              let view = viewRecord(viewID),
              let paint = paintProjections[view.session],
              let expectation = paintExpectation(for: view, paint: paint)
        else {
            refreshPaint()
            return
        }
        canvasPaintGesture = CanvasPaintGesture(
            viewID: view.id,
            target: view.coreTarget,
            tool: paint.editor.activeTool,
            expectation: expectation,
            start: sample,
            samples: [sample]
        )
        switch paint.editor.activeTool {
        case .pencil, .brush, .eraser:
            observePaintMutation(
                application.coreHost.beginRasterStroke(
                    target: view.coreTarget,
                    expectation: expectation,
                    samples: [sample]
                ),
                requestsSnapshot: true,
                viewID: view.id
            )
        case .fill, .eyedropper, .selection, .floatingTransform, .colorReplace:
            break
        }
    }

    func appendStroke(_ sample: CorePointerSample, viewID: WorkspaceViewID? = nil) {
        guard var gesture = canvasPaintGesture,
              viewID == nil || gesture.viewID == viewID
        else {
            return
        }
        switch gesture.tool {
        case .pencil, .brush, .eraser:
            observePaintMutation(
                application.coreHost.appendRasterStroke(
                    target: gesture.target,
                    samples: [sample]
                ),
                requestsSnapshot: true,
                viewID: gesture.viewID
            )
        case .fill, .eyedropper:
            gesture.samples = [gesture.start, sample]
            canvasPaintGesture = gesture
        case .selection:
            guard gesture.samples.count < 1_048_576 else {
                cancelStroke(viewID: gesture.viewID)
                lastCommandResult = .failed(.coreOperation(.invalidArgument))
                return
            }
            gesture.samples.append(sample)
            canvasPaintGesture = gesture
        case .floatingTransform:
            canvasPaintGesture = nil
        case .colorReplace:
            guard gesture.samples.count < 1_048_576 else {
                cancelStroke(viewID: gesture.viewID)
                lastCommandResult = .failed(.coreOperation(.invalidArgument))
                return
            }
            gesture.samples.append(sample)
            canvasPaintGesture = gesture
            if gesture.samples.count == 2 || gesture.samples.count.isMultiple(of: 8) {
                requestColorReplacePreview(for: gesture)
            }
        }
    }

    func endStroke(finalSample: CorePointerSample?, viewID: WorkspaceViewID? = nil) {
        guard var gesture = canvasPaintGesture,
              viewID == nil || gesture.viewID == viewID
        else {
            return
        }
        canvasPaintGesture = nil
        if let finalSample, gesture.samples.last != finalSample {
            gesture.samples.append(finalSample)
        }
        switch gesture.tool {
        case .pencil, .brush, .eraser:
            if let finalSample {
                _ = application.coreHost.appendRasterStroke(
                    target: gesture.target,
                    samples: [finalSample]
                )
            }
            observePaintMutation(
                application.coreHost.endStroke(target: gesture.target),
                requestsSnapshot: true,
                viewID: gesture.viewID
            )
        case .fill:
            observePaintMutation(
                application.coreHost.applyFill(
                    target: gesture.target,
                    expectation: gesture.expectation,
                    gesture: CoreFillGesture(
                        start: gesture.start,
                        end: gesture.samples.last ?? gesture.start
                    )
                ),
                requestsSnapshot: true,
                viewID: gesture.viewID
            )
        case .eyedropper:
            observePaintMutation(
                application.coreHost.eyedropper(
                    target: gesture.target,
                    expectation: gesture.expectation,
                    source: eyedropperSource,
                    devicePoint: gesture.samples.last ?? gesture.start
                ),
                requestsSnapshot: false,
                viewID: gesture.viewID
            )
        case .selection:
            guard let options = paintProjections[gesture.target.session]?.editor.selectionOptions
            else { return }
            let samples: [CorePointerSample] = switch options.shape {
            case .rectangle, .ellipse:
                [gesture.start, gesture.samples.last ?? gesture.start]
            case .wand:
                [gesture.samples.last ?? gesture.start]
            case .lasso, .polyline, .trace:
                gesture.samples
            }
            observePaintMutation(
                application.coreHost.applySelection(
                    target: gesture.target,
                    expectation: gesture.expectation,
                    samples: samples
                ),
                requestsSnapshot: true,
                viewID: gesture.viewID
            )
        case .floatingTransform:
            lastCommandResult = .noOp
        case .colorReplace:
            colorReplacePreview = nil
            guard let request = colorReplaceRequest(for: gesture) else { return }
            observePaintMutation(
                application.coreHost.applyColorReplace(
                    target: gesture.target,
                    expectation: gesture.expectation,
                    request: request
                ),
                requestsSnapshot: true,
                viewID: gesture.viewID
            )
        }
    }

    func cancelStroke(viewID: WorkspaceViewID? = nil) {
        guard let gesture = canvasPaintGesture,
              viewID == nil || gesture.viewID == viewID
        else {
            return
        }
        canvasPaintGesture = nil
        colorReplacePreview = nil
        if [.pencil, .brush, .eraser].contains(gesture.tool) {
            observePaintMutation(
                application.coreHost.cancelStroke(target: gesture.target),
                requestsSnapshot: true,
                viewID: gesture.viewID
            )
        } else {
            lastCommandResult = .cancelled
        }
    }

    private func colorReplaceRequest(
        for gesture: CanvasPaintGesture
    ) -> CoreColorReplaceRequest? {
        let region: CoreColorReplaceRegion = switch colorReplaceRegionTool {
        case .all:
            .entireSelectionOrDocument
        case .rectangle:
            .rectangle(CoreFillGesture(
                start: gesture.start,
                end: gesture.samples.last ?? gesture.start
            ))
        case .pen:
            .pen(
                samples: gesture.samples,
                diameter: Float(paintProjections[gesture.target.session]?.editor.diameter ?? 1)
            )
        case .polyline:
            .polyline(gesture.samples)
        case .lasso:
            .lasso(gesture.samples)
        }
        guard let replacement = paintProjections[gesture.target.session]?.editor.currentColor
        else { return nil }
        return CoreColorReplaceRequest(
            mode: .rasterColor,
            targetColor: colorReplaceTarget,
            replacementColor: replacement,
            region: region
        )
    }

    private func requestColorReplacePreview(for gesture: CanvasPaintGesture) {
        guard let request = colorReplaceRequest(for: gesture) else { return }
        let generation = lifecycleGeneration
        observe(
            application.coreHost.previewColorReplace(
                target: gesture.target,
                expectation: gesture.expectation,
                request: request
            ),
            generation: generation
        ) { [weak self] outcome in
            guard let self, self.canvasPaintGesture?.target == gesture.target else { return }
            if case let .colorReplacePreview(preview) = outcome {
                self.colorReplacePreview = preview
            }
        }
    }

    func updateLocator(_ sample: CorePointerSample, viewID: WorkspaceViewID) {
        guard inspectorSectionIsActive(.color), locatorVisible, !locatorFixed,
              let context = locatorPaneContext(preferredViewID: viewID)
        else {
            return
        }
        let generation = lifecycleGeneration
        observe(
            application.coreHost.inspectLocator(
                target: context.view,
                expectedViewRevision: context.viewRevision,
                devicePoint: sample,
                radius: 4
            ),
            generation: generation
        ) { [weak self] outcome in
            guard let self, case let .locator(projection) = outcome,
                  self.locatorPaneContext(preferredViewID: viewID)?.view == context.view
            else {
                return
            }
            self.locator = projection
        }
    }

    func selectLocatorPixel(documentX: Int32, documentY: Int32) {
        guard locatorFixed,
              let context = locatorPaneContext(),
              let view = editorGraph?.allViews.first(where: { $0.coreTarget == context.view }),
              let paint = paintProjections[context.session],
              let expectation = paintExpectation(for: view, paint: paint)
        else {
            lastCommandResult = .stale
            return
        }
        observePaintMutation(
            application.coreHost.paintLocatorPixel(
                target: context.view,
                expectation: expectation,
                documentX: documentX,
                documentY: documentY
            ),
            requestsSnapshot: true,
            viewID: view.id
        )
        guard locatorAutoscroll, let locator else { return }
        let right = locator.neighborhoodOriginX
            + Int32(locator.neighborhoodWidth) - 1
        let bottom = locator.neighborhoodOriginY
            + Int32(locator.neighborhoodHeight) - 1
        let dx = documentX == locator.neighborhoodOriginX ? 32.0
            : (documentX == right ? -32.0 : 0.0)
        let dy = documentY == locator.neighborhoodOriginY ? 32.0
            : (documentY == bottom ? -32.0 : 0.0)
        if dx != 0 || dy != 0 {
            pan(deviceDX: dx, deviceDY: dy, viewID: view.id)
        }
    }

    func updateEditor(_ update: CoreEditorUpdate, viewID: WorkspaceViewID? = nil) {
        guard let view = viewRecord(viewID), let paint = paintProjections[view.session],
              let expectation = paintExpectation(for: view, paint: paint)
        else {
            lastCommandResult = .stale
            refreshPaint()
            return
        }
        observePaintMutation(
            application.coreHost.updateEditor(
                target: view.coreTarget,
                expectation: expectation,
                update: update
            ),
            requestsSnapshot: false,
            viewID: view.id
        )
    }

    func chooseColor(_ color: CoreColorValue) {
        updateEditor(.toolColor(color))
    }

    func choosePaletteColor(_ color: CoreColorValue) {
        chooseColor(color)
    }

    func selectChartEntry(_ entry: CoreColorChartEntry) {
        chartCursor = entry.index
        chooseColor(entry.color)
    }

    var colorPaneIsPinned: Bool {
        if case .pinnedDocument = colorPaneTarget.mode { return true }
        return false
    }

    var locatorPaneIsPinned: Bool {
        if case .pinnedDocument = locatorPaneTarget.mode { return true }
        return false
    }

    func toggleColorPanePin() {
        if colorPaneIsPinned {
            colorPaneTarget.follow()
        } else if let active = editorGraph?.activeView {
            colorPaneTarget.pin(to: active)
        }
        refreshPaint()
    }

    func toggleLocatorPanePin() {
        if locatorPaneIsPinned {
            locatorPaneTarget.follow()
        } else if let active = editorGraph?.activeView {
            locatorPaneTarget.pin(to: active)
        }
    }

    func applyColorChartPreview() {
        guard let preview = colorChartPreview,
              let session = sessionProjections[preview.session]
        else { return }
        colorChartPreview = nil
        observePaintMutation(
            application.coreHost.applyColorChartPreview(
                target: preview.session,
                expectedDocumentRevision: session.documentRevision,
                preview: preview.id
            ),
            requestsSnapshot: false
        )
    }

    func cancelColorChartPreview() {
        guard let preview = colorChartPreview else { return }
        colorChartPreview = nil
        observePaintMutation(
            application.coreHost.cancelColorChartPreview(preview.id),
            requestsSnapshot: false
        )
    }

    func pan(deviceDX: Double, deviceDY: Double, viewID: WorkspaceViewID? = nil) {
        guard let view = viewRecord(viewID) else { return }
        observeCoreMutation(
            application.coreHost.applyView(
                target: view.coreTarget,
                command: .panBy(deviceDX: deviceDX, deviceDY: deviceDY)
            ),
            requestsSnapshot: true,
            viewID: view.id
        )
    }

    func zoom(
        factor: Double,
        deviceX: Double,
        deviceY: Double,
        viewID: WorkspaceViewID? = nil
    ) {
        guard let view = viewRecord(viewID) else { return }
        observeCoreMutation(
            application.coreHost.applyView(
                target: view.coreTarget,
                command: .zoomAt(factor: factor, deviceX: deviceX, deviceY: deviceY)
            ),
            requestsSnapshot: true,
            viewID: view.id
        )
    }

    func undo() {
        guard let projection else { return }
        observeCoreMutation(
            application.coreHost.undo(target: projection.target),
            requestsSnapshot: true
        )
    }

    func openDroppedURL(_ url: URL) {
        guard FileTypeCatalog.classify(url) != nil else {
            lastCommandResult = .invalid
            return
        }
        Task { await openURL(url, recovery: false) }
    }

    func openRecentURL(_ url: URL, context: CommandTargetContext) {
        guard matches(context), application.recentURLs.contains(url) else {
            lastCommandResult = .stale
            return
        }
        Task { await openURL(url, recovery: false) }
    }

    func applyPendingPaste() {
        guard let draft = pendingFloatingTransform else { return }
        commitPendingTransform(draft)
    }

    func cancelPendingPaste() {
        guard let pendingPasteConfirmation else { return }
        self.pendingPasteConfirmation = nil
        pendingFloatingTransform = nil
        floatingTransformEditor = nil
        observeFileMutation(
            application.coreHost.cancelPaste(
                target: pendingPasteConfirmation.context.session,
                expectedDocumentRevision: pendingPasteConfirmation.context.documentRevision
            ),
            generation: pendingPasteConfirmation.context.lifecycleGeneration
        )
    }

    func previewPendingPaste(_ draft: FloatingTransformDraft) {
        guard pendingPasteConfirmation?.context == draft.context,
              matches(draft.context)
        else {
            lastCommandResult = .stale
            return
        }
        pendingFloatingTransform = draft
        let generation = lifecycleGeneration
        let transformTask = application.coreHost.transformFloatingPaste(
            target: draft.context.session,
            expectedDocumentRevision: draft.context.documentRevision,
            transform: draft.transform
        )
        Task { @MainActor [weak self] in
            guard let self else { return }
            let outcome = await transformTask.value()
            guard lifecycleGeneration == generation, phase == .ready,
                  pendingPasteConfirmation?.context == draft.context
            else { return }
            switch outcome {
            case let .floatingTransformed(session):
                updateSessionProjection(session)
                lastCommandResult = .started
                requestSnapshot()
            case .noOp:
                lastCommandResult = .noOp
            case .failed(.staleTarget):
                lastCommandResult = .stale
            case let .failed(failure):
                lastCommandResult = .failed(failure)
            default:
                lastCommandResult = .invalid
            }
        }
    }

    func nudgePendingPaste(documentDX: Double, documentDY: Double) {
        guard var draft = pendingFloatingTransform else { return }
        draft.targetX += documentDX
        draft.targetY += documentDY
        previewPendingPaste(draft)
    }

    func localizedText(_ key: String) -> String {
        application.languageController.text(key)
    }

    func floatingHandleDevicePoints(viewID: WorkspaceViewID) -> [CGPoint] {
        guard let draft = pendingFloatingTransform,
              let view = viewRecord(viewID),
              view.coreTarget == draft.context.view,
              currentZoom.isFinite, currentZoom > 0
        else { return [] }
        let width = draft.sourceWidth * draft.scaleX
        let height = draft.sourceHeight * draft.scaleY
        let anchorOffset: CGPoint = switch draft.anchor {
        case .topLeft: CGPoint(x: 0, y: 0)
        case .topRight: CGPoint(x: width, y: 0)
        case .center: CGPoint(x: width / 2, y: height / 2)
        case .bottomLeft: CGPoint(x: 0, y: height)
        case .bottomRight: CGPoint(x: width, y: height)
        }
        let radians = draft.rotationDegrees * .pi / 180
        let cosine = cos(radians)
        let sine = sin(radians)
        let documentPoints = [
            CGPoint(x: 0, y: 0),
            CGPoint(x: width, y: 0),
            CGPoint(x: width / 2, y: height / 2),
            CGPoint(x: 0, y: height),
            CGPoint(x: width, y: height),
        ].map { point -> CGPoint in
            let x = point.x - anchorOffset.x
            let y = point.y - anchorOffset.y
            return CGPoint(
                x: draft.targetX + x * cosine - y * sine,
                y: draft.targetY + x * sine + y * cosine
            )
        }
        return documentPoints.map {
            CGPoint(
                x: $0.x * currentZoom + currentPanX,
                y: $0.y * currentZoom + currentPanY
            )
        }
    }

    func commitPendingTransform(_ draft: FloatingTransformDraft) {
        guard pendingPasteConfirmation?.context == draft.context,
              matches(draft.context)
        else {
            lastCommandResult = .stale
            return
        }
        pendingFloatingTransform = draft
        let generation = lifecycleGeneration
        let transformTask = application.coreHost.transformFloatingPaste(
            target: draft.context.session,
            expectedDocumentRevision: draft.context.documentRevision,
            transform: draft.transform
        )
        Task { @MainActor [weak self] in
            guard let self else { return }
            let outcome = await transformTask.value()
            guard lifecycleGeneration == generation, phase == .ready else { return }
            switch outcome {
            case .floatingTransformed, .noOp:
                guard let confirmation = pendingPasteConfirmation,
                      confirmation.context == draft.context
                else { return }
                pendingPasteConfirmation = nil
                pendingFloatingTransform = nil
                floatingTransformEditor = nil
                let commit = await application.coreHost.commitPaste(
                    target: confirmation.context.session,
                    expectedDocumentRevision: confirmation.context.documentRevision
                ).value()
                guard lifecycleGeneration == generation, phase == .ready else { return }
                handleFileOutcome(
                    commit,
                    sourceURL: nil,
                    identity: nil,
                    reservation: nil
                )
            case .failed(.staleTarget):
                lastCommandResult = .stale
            case let .failed(failure):
                lastCommandResult = .failed(failure)
            default:
                lastCommandResult = .invalid
            }
        }
    }

    func recoverPendingCandidate() {
        guard let candidate = pendingRecoveryDecision else { return }
        pendingRecoveryDecision = nil
        let originalURL = candidate.originalPath.map { URL(filePath: $0) }
        Task {
            await openURL(
                candidate.artifactURL,
                recovery: true,
                recoveryOriginalURL: originalURL
            )
        }
    }

    func discardPendingRecoveryCandidate() {
        guard let candidate = pendingRecoveryDecision else { return }
        do {
            try application.recoveryStore.discard(candidate)
            pendingRecoveryDecision = nil
            lastCommandResult = .started
        } catch {
            lastCommandResult = .failed(.coreOperation(.ioError))
        }
    }

    func deferPendingRecoveryCandidate() {
        guard pendingRecoveryDecision != nil else { return }
        pendingRecoveryDecision = nil
        lastCommandResult = .cancelled
    }

    private func presentOpenPanel(recovery: Bool = false, rasterOnly: Bool = false) {
        Task {
            let panel = NSOpenPanel()
            panel.allowsMultipleSelection = false
            panel.canChooseDirectories = false
            panel.allowedContentTypes = recovery ? [FileTypeCatalog.native]
                : (rasterOnly ? FileTypeCatalog.rasterContentTypes
                    : FileTypeCatalog.readableContentTypes)
            guard await panelResponse(panel) == .OK, let url = panel.url else {
                lastCommandResult = .cancelled
                return
            }
            await openURL(url, recovery: recovery)
        }
    }

    private func openURL(
        _ url: URL,
        recovery: Bool,
        recoveryOriginalURL: URL? = nil
    ) async {
        guard !isFileOperationActive, let projection else { return }
        guard let classification = FileTypeCatalog.classify(url),
              recovery ? classification == .native : true
        else {
            lastCommandResult = .invalid
            return
        }
        if classification == .native, await tryOpenCut(
            url,
            recovery: recovery,
            memberBaseURL: recoveryOriginalURL?.deletingLastPathComponent()
        ) {
            return
        }
        guard await confirmReplacingDirtyDocument() else {
            lastCommandResult = .cancelled
            return
        }
        let generation = lifecycleGeneration
        let action: CoordinatedFileAction
        let reservation: FileIdentityReservation?
        let identity: FileIdentity?
        switch classification {
        case .native:
            if recovery {
                identity = nil
                reservation = nil
                action = .recovery(projection.target, projection.documentRevision)
                break
            }
            let resolvedIdentity = FileIdentity.resolve(url)
            if let owner = application.fileIdentityRegistry.owner(of: resolvedIdentity),
               owner != projection.target
            {
                application.focusSession(owner)
                lastCommandResult = .noOp
                return
            }
            guard let reserved = application.fileIdentityRegistry.reserve(
                resolvedIdentity,
                for: projection.target
            ) else {
                lastCommandResult = .stale
                return
            }
            identity = resolvedIdentity
            reservation = reserved
            action = .open(projection.target, projection.documentRevision)
        case let .raster(format):
            identity = nil
            reservation = nil
            isFileOperationActive = true
            let result = await importRaster(
                url: url,
                format: format,
                target: projection.target,
                revision: projection.documentRevision,
                documentUUID: WorkspaceID().coreDocumentUUID
            )
            isFileOperationActive = false
            guard lifecycleGeneration == generation else { return }
            handleFileOutcome(result, sourceURL: nil, identity: nil, reservation: nil)
            return
        }
        isFileOperationActive = true
        let result = await coordinated(action, at: url)
        isFileOperationActive = false
        guard lifecycleGeneration == generation else {
            if let reservation { application.fileIdentityRegistry.cancel(reservation) }
            return
        }
        handleFileOutcome(
            result,
            sourceURL: url,
            identity: recovery ? nil : identity,
            reservation: reservation
        )
    }

    private func save(chooseDestination: Bool, allowClean: Bool = false) async -> Bool {
        guard !isFileOperationActive, let projection else { return false }
        let wasUntitled = documentURL == nil
        var destination = documentURL
        if chooseDestination || destination == nil {
            let panel = NSSavePanel()
            panel.allowedContentTypes = [FileTypeCatalog.native]
            panel.nameFieldStringValue = documentURL?.lastPathComponent ?? "Untitled.inkpod"
            guard await panelResponse(panel) == .OK, let selected = panel.url else {
                lastCommandResult = .cancelled
                return false
            }
            destination = selected
        }
        guard let destination else { return false }
        let identity = FileIdentity.resolve(destination)
        if let owner = application.fileIdentityRegistry.owner(of: identity),
           owner != projection.target
        {
            application.focusSession(owner)
            lastCommandResult = .stale
            return false
        }
        guard let reservation = application.fileIdentityRegistry.reserve(
            identity,
            for: projection.target
        ) else {
            lastCommandResult = .stale
            return false
        }
        isFileOperationActive = true
        let result = await coordinated(
            .save(
                projection.target,
                projection.documentRevision,
                allowClean: allowClean || chooseDestination || wasUntitled
            ),
            at: destination
        )
        isFileOperationActive = false
        guard lifecycleGeneration == commandContext?.lifecycleGeneration else {
            application.fileIdentityRegistry.cancel(reservation)
            return false
        }
        handleFileOutcome(
            result,
            sourceURL: destination,
            identity: identity,
            reservation: reservation
        )
        if case .failed = result {
            fileOperationAlert = .saveFailed
        }
        if case .fileCompleted = result { return true }
        if case .noOp = result {
            application.fileIdentityRegistry.cancel(reservation)
            return true
        }
        return false
    }

    func dismissFileOperationAlert() {
        fileOperationAlert = nil
    }

    private func autosaveNow() async {
        guard !isFileOperationActive, let projection else { return }
        do {
            let directory = try recoveryDirectory()
            let destination = directory.appending(path: "\(id.rawValue.uuidString).inkpod")
            isFileOperationActive = true
            let result = await coordinated(
                .autosave(projection.target, projection.documentRevision),
                at: destination
            )
            isFileOperationActive = false
            if case let .fileCompleted(file) = result,
               file.operation == .autosave
            {
                do {
                    _ = try application.recoveryStore.publish(
                        artifactURL: destination,
                        session: file.session.target,
                        documentUUID: file.session.documentUUID,
                        originalPath: documentURL?.path,
                        writtenAtMilliseconds: UInt64(
                            max(1, Date().timeIntervalSince1970 * 1_000)
                        )
                    )
                } catch {
                    try? FileManager.default.removeItem(at: destination)
                    try? FileManager.default.removeItem(
                        at: application.recoveryStore.metadataURL(for: destination)
                    )
                    lastCommandResult = .failed(.coreOperation(.ioError))
                    return
                }
            }
            handleFileOutcome(
                result,
                sourceURL: destination,
                identity: nil,
                reservation: nil
            )
        } catch {
            isFileOperationActive = false
            lastCommandResult = .failed(.coreOperation(.ioError))
        }
    }

    private func revert(partial: Bool) {
        guard let projection else { return }
        let task = partial
            ? application.coreHost.revertPartial(
                target: projection.target,
                expectedDocumentRevision: projection.documentRevision
            )
            : application.coreHost.revert(
                target: projection.target,
                expectedDocumentRevision: projection.documentRevision
            )
        observeFileMutation(task, generation: lifecycleGeneration)
    }

    private func exportRaster() {
        Task {
            guard !isFileOperationActive, let projection else { return }
            let panel = NSSavePanel()
            panel.allowedContentTypes = FileTypeCatalog.rasterContentTypes
            panel.nameFieldStringValue = documentURL?.deletingPathExtension()
                .lastPathComponent.appending(".png") ?? "Untitled.png"
            let white = NSButton(
                checkboxWithTitle: "Composite transparent pixels over white",
                target: nil,
                action: nil
            )
            panel.accessoryView = white
            guard await panelResponse(panel) == .OK, let destination = panel.url,
                  case let .raster(format) = FileTypeCatalog.classify(destination)
            else {
                lastCommandResult = .cancelled
                return
            }
            isFileOperationActive = true
            let exported = await application.coreHost.exportCommonRaster(
                target: projection.target,
                expectedDocumentRevision: projection.documentRevision,
                format: format,
                compositeWhite: white.state == .on
            ).value()
            guard case let .rasterExported(output) = exported else {
                isFileOperationActive = false
                handleFileOutcome(exported, sourceURL: nil, identity: nil, reservation: nil)
                return
            }
            let broker = application.fileAccessBroker
            let bytes = Data(output.bytes)
            let writeSucceeded = await Task.detached {
                Result {
                    try broker.coordinateReplacing(destination) { coordinatedURL in
                        try AtomicFileWriter().write(bytes, to: coordinatedURL)
                    }
                }
            }.value
            isFileOperationActive = false
            switch writeSucceeded {
            case .success:
                lastCommandResult = .started
            case .failure:
                lastCommandResult = .failed(.coreOperation(.ioError))
            }
        }
    }

    private func exportInstructionRaster() {
        Task {
            guard !isFileOperationActive, let projection else { return }
            let panel = NSSavePanel()
            panel.allowedContentTypes = FileTypeCatalog.rasterContentTypes
            panel.nameFieldStringValue = documentURL?.deletingPathExtension()
                .lastPathComponent.appending("-instructions.png") ?? "Instructions.png"
            let white = NSButton(
                checkboxWithTitle: "Composite transparent pixels over white",
                target: nil,
                action: nil
            )
            panel.accessoryView = white
            guard await panelResponse(panel) == .OK, let destination = panel.url,
                  case let .raster(format) = FileTypeCatalog.classify(destination)
            else {
                lastCommandResult = .cancelled
                return
            }
            isFileOperationActive = true
            let exported = await application.coreHost.exportInstructionRaster(
                target: projection.target,
                expectedDocumentRevision: projection.documentRevision,
                format: format,
                compositeWhite: white.state == .on
            ).value()
            guard case let .rasterExported(output) = exported else {
                isFileOperationActive = false
                handleFileOutcome(exported, sourceURL: nil, identity: nil, reservation: nil)
                return
            }
            let broker = application.fileAccessBroker
            let bytes = Data(output.bytes)
            let writeSucceeded = await Task.detached {
                Result {
                    try broker.coordinateReplacing(destination) { coordinatedURL in
                        try AtomicFileWriter().write(bytes, to: coordinatedURL)
                    }
                }
            }.value
            isFileOperationActive = false
            switch writeSucceeded {
            case .success:
                lastCommandResult = .started
            case .failure:
                lastCommandResult = .failed(.coreOperation(.ioError))
            }
        }
    }

    private func compactedCopy() {
        Task {
            guard !isFileOperationActive, let projection else { return }
            let plan = await application.coreHost.compactionPlan(
                target: projection.target,
                expectedDocumentRevision: projection.documentRevision
            ).value()
            guard case let .compactionPlanned(token) = plan else {
                handleFileOutcome(plan, sourceURL: nil, identity: nil, reservation: nil)
                return
            }
            let alert = NSAlert()
            alert.messageText = "Create a compacted copy?"
            alert.informativeText = "This separate copy omits \(token.historyEventCount) history events and \(token.historyProcedureCount) procedures. The current document is unchanged."
            alert.addButton(withTitle: "Continue")
            alert.addButton(withTitle: "Cancel")
            guard await alertResponse(alert) == .alertFirstButtonReturn else {
                lastCommandResult = .cancelled
                return
            }
            let panel = NSSavePanel()
            panel.allowedContentTypes = [FileTypeCatalog.native]
            panel.nameFieldStringValue = "Compacted Copy.inkpod"
            guard await panelResponse(panel) == .OK, let destination = panel.url else {
                lastCommandResult = .cancelled
                return
            }
            isFileOperationActive = true
            let result = await coordinated(
                .compact(projection.target, projection.documentRevision, token),
                at: destination
            )
            isFileOperationActive = false
            handleFileOutcome(result, sourceURL: nil, identity: nil, reservation: nil)
        }
    }

    private func copy(cut: Bool) {
        guard let context = commandContext else { return }
        Task {
            let result = await (cut
                ? application.coreHost.cutClipboard(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision
                )
                : application.coreHost.copyClipboard(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision
                )).value()
            guard lifecycleGeneration == context.lifecycleGeneration else {
                if case let .clipboardCopied(copied) = result {
                    _ = await application.coreHost.releaseClipboard(copied.id).value()
                }
                return
            }
            guard case let .clipboardCopied(copied) = result else {
                handleFileOutcome(result, sourceURL: nil, identity: nil, reservation: nil)
                return
            }
            guard await application.clipboardBroker.publish(copied) else {
                lastCommandResult = .failed(.coreOperation(.invalidState))
                return
            }
            if let session = copied.session { updateSessionProjection(session) }
            lastCommandResult = .started
            if cut { requestSnapshot() }
        }
    }

    private func paste(mode: CorePasteMode) {
        guard let context = commandContext else { return }
        Task {
            var effectiveMode = mode
            if case .newRasterPlane = mode {
                guard let name = await requestPastePlaneName() else {
                    lastCommandResult = .cancelled
                    return
                }
                effectiveMode = .newRasterPlane(CoreNewPlanePaste(name: name))
            }
            guard let clipboard = await application.clipboardBroker.projectionForPaste() else {
                lastCommandResult = .invalid
                return
            }
            let result = await application.coreHost.beginPaste(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                clipboard: clipboard.id,
                mode: effectiveMode
            ).value()
            guard lifecycleGeneration == context.lifecycleGeneration else { return }
            switch result {
            case let .pasteStarted(updated):
                updateSessionProjection(updated)
                pendingPasteConfirmation = PasteConfirmation(
                    context: context,
                    mode: effectiveMode
                )
                pendingFloatingTransform = FloatingTransformDraft(
                    context: context,
                    mode: effectiveMode,
                    raster: clipboard.raster
                )
                floatingTransformEditor = nil
                lastCommandResult = .presentedInput
                requestSnapshot()
            case .noOp:
                lastCommandResult = .noOp
            case .failed(.staleTarget):
                lastCommandResult = .stale
            case let .failed(failure):
                lastCommandResult = .failed(failure)
            default:
                lastCommandResult = .invalid
            }
        }
    }

    func activate(groupID: EditorGroupID, viewID: WorkspaceViewID) {
        guard var graph = editorGraph, graph.activate(groupID: groupID, viewID: viewID),
              let active = graph.activeView,
              let session = sessionProjections[active.session]
        else {
            return
        }
        editorGraph = graph
        projection = session
        documentURL = sessionDocumentURLs[session.target]
        fileIdentity = sessionFileIdentities[session.target]
        window?.representedURL = documentURL
        window?.title = documentURL?.lastPathComponent ?? active.title
        drawableSize = drawableSizes[active.id] ?? drawableSize
        if let cut {
            selectedCutMemberIndex = cut.members.firstIndex {
                $0.documentUUID == session.documentUUID && $0.cellID == session.cellID
            }
        }
        refreshTree()
        refreshPaint()
        refreshHistory(rebuildVisualization: true)
        refreshM8()
        refreshAnimation()
        requestSnapshot(viewID: active.id)
    }

    func perform(_ command: InkpodCommandID) {
        guard let context = commandContext else { return }
        _ = execute(command, context: context)
    }

    func selectTool(_ command: InkpodCommandID) {
        perform(command)
    }

    func toggleToolOptions(for tool: CoreEditorTool) {
        if adaptiveChrome.toolPresentation == .hidden {
            _ = reduceChrome(.setToolPresentation(.compact))
        }
        var replacement = toolOptionsPresentation
        _ = replacement.toggle(tool)
        toolOptionsPresentation = replacement
    }

    @discardableResult
    func presentToolOptions(for tool: CoreEditorTool) -> Bool {
        if adaptiveChrome.toolPresentation == .hidden {
            _ = reduceChrome(.setToolPresentation(.compact))
        }
        var replacement = toolOptionsPresentation
        let changed = replacement.present(tool)
        guard changed else { return false }
        toolOptionsPresentation = replacement
        return true
    }

    func setToolOptionsPinned(_ pinned: Bool) {
        var replacement = toolOptionsPresentation
        guard replacement.setPinned(pinned) else { return }
        toolOptionsPresentation = replacement
    }

    func dismissToolOptions(for tool: CoreEditorTool) {
        guard toolOptionsPresentation.tool == tool else { return }
        var replacement = toolOptionsPresentation
        guard replacement.dismissTransient() else { return }
        toolOptionsPresentation = replacement
    }

    @discardableResult
    func closeToolOptions() -> Bool {
        var replacement = toolOptionsPresentation
        guard replacement.close() else { return false }
        toolOptionsPresentation = replacement
        return true
    }

    func performLayerPane(_ command: InkpodCommandID) {
        guard let context = layerPaneContext() else {
            lastCommandResult = .stale
            return
        }
        _ = execute(command, context: context)
    }

    var layerPaneIsPinned: Bool {
        if case .pinnedDocument = layerPaneTarget.mode { return true }
        return false
    }

    func toggleLayerPanePin() {
        if layerPaneIsPinned {
            layerPaneTarget.follow()
            layerPaneAccessibilityNotice = nil
            refreshTree()
            return
        }
        guard let active = editorGraph?.activeView else { return }
        layerPaneTarget.pin(to: active)
        layerPaneAccessibilityNotice = nil
        refreshTree()
    }

    func moveView(_ viewID: WorkspaceViewID, to groupID: EditorGroupID) -> Bool {
        guard var graph = editorGraph, graph.move(viewID: viewID, to: groupID) else {
            return false
        }
        editorGraph = graph
        if let active = graph.activeView, let session = sessionProjections[active.session] {
            projection = session
            refreshTree()
            refreshPaint()
        }
        return true
    }

    func beginNewCell() {
        pendingNewCellPlan = nil
        pendingNewCellDraft = NewCellDraft()
    }

    func prepareNewCell(_ draft: NewCellDraft) {
        guard pendingNewCellDraft?.id == draft.id else {
            lastCommandResult = .stale
            return
        }
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            guard let self else { return }
            let outcome = await application.coreHost.prepareCellCreation(draft.options).value()
            guard lifecycleGeneration == generation else {
                if case let .cellPlan(plan) = outcome {
                    _ = await application.coreHost.cancelCellCreation(plan.id).value()
                }
                return
            }
            switch outcome {
            case let .cellPlan(plan):
                pendingNewCellDraft = draft
                pendingNewCellPlan = plan
                lastCommandResult = .presentedInput
            case let .failed(failure):
                lastCommandResult = .failed(failure)
            default:
                lastCommandResult = .invalid
            }
        }
    }

    func commitNewCellPlan() {
        guard let plan = pendingNewCellPlan else { return }
        let uuids = plan.items.map { _ in WorkspaceID().coreDocumentUUID }
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            guard let self else { return }
            let outcome = await application.coreHost.commitCellCreation(
                plan: plan.id,
                documentUUIDs: uuids
            ).value()
            guard lifecycleGeneration == generation else {
                if case let .cellsCreated(sessions) = outcome {
                    for session in sessions {
                        _ = await application.coreHost.closeSession(session.target).value()
                    }
                }
                return
            }
            guard case let .cellsCreated(created) = outcome,
                  var graph = editorGraph
            else {
                if case let .failed(failure) = outcome { lastCommandResult = .failed(failure) }
                return
            }
            let records = created.map { session in
                WorkspaceViewRecord(
                    id: WorkspaceViewID(rawValue: session.primaryView.id.rawValue),
                    coreTarget: session.primaryView,
                    session: session.target,
                    viewRevision: session.viewRevision,
                    title: documentTitle(for: session)
                )
            }
            guard graph.appendAtomically(records, to: graph.activeGroupID) else {
                for session in created {
                    _ = await application.coreHost.closeSession(session.target).value()
                }
                lastCommandResult = .failed(.sessionLimit)
                return
            }
            for session in created { sessionProjections[session.target] = session }
            for session in created {
                application.claimSession(session.target, for: id)
            }
            editorGraph = graph
            if let last = created.last { projection = last }
            pendingNewCellPlan = nil
            pendingNewCellDraft = nil
            lastCommandResult = .started
            refreshTree()
        }
    }

    func cancelNewCell() {
        let plan = pendingNewCellPlan
        pendingNewCellPlan = nil
        pendingNewCellDraft = nil
        lastCommandResult = .cancelled
        if let plan { _ = application.coreHost.cancelCellCreation(plan.id) }
    }

    private func createLogicalView(
        split: WorkspaceSplitOrientation? = nil,
        inOtherGroup: Bool = false
    ) {
        guard let projection, let existingGraph = editorGraph else { return }
        let sourceTitle = existingGraph.activeView?.title ?? documentTitle(for: projection)
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            guard let self else { return }
            let outcome = await application.coreHost.createView(
                target: projection.target,
                expectedDocumentRevision: projection.documentRevision
            ).value()
            guard lifecycleGeneration == generation else {
                if case let .viewCreated(view) = outcome {
                    _ = await application.coreHost.closeView(view.target).value()
                }
                return
            }
            guard case let .viewCreated(created) = outcome else {
                if case let .failed(failure) = outcome { lastCommandResult = .failed(failure) }
                return
            }
            var graph = existingGraph
            let record = WorkspaceViewRecord(
                id: WorkspaceViewID(rawValue: created.target.id.rawValue),
                coreTarget: created.target,
                session: created.session.target,
                viewRevision: created.viewRevision,
                title: "\(sourceTitle) — View"
            )
            let inserted: Bool
            if let split {
                inserted = graph.split(split, with: record) != nil
            } else if inOtherGroup, graph.groups.count == 2 {
                let destination = graph.groups.first { $0.id != graph.activeGroupID }!.id
                inserted = graph.appendAtomically([record], to: destination)
            } else {
                inserted = graph.insertDuplicate(record, after: graph.activeView!.id)
            }
            guard inserted else {
                _ = await application.coreHost.closeView(created.target).value()
                lastCommandResult = .noOp
                return
            }
            editorGraph = graph
            updateSessionProjection(created.session)
            lastCommandResult = .started
        }
    }

    private func closeActiveView() {
        guard var graph = editorGraph, let active = graph.activeView else { return }
        if graph.allViews.count == 1 {
            window?.performClose(nil)
            return
        }
        let removed = graph.remove(viewID: active.id)
        guard removed != nil else { return }
        editorGraph = graph
        if active.coreTarget != sessionProjections[active.session]?.primaryView {
            _ = application.coreHost.closeView(active.coreTarget)
        }
        if !graph.allViews.contains(where: { $0.session == active.session }) {
            sessionProjections.removeValue(forKey: active.session)
            treeProjections.removeValue(forKey: active.session)
            animationProjections.removeValue(forKey: active.session)
            sessionDocumentURLs.removeValue(forKey: active.session)
            sessionFileIdentities.removeValue(forKey: active.session)
            application.fileIdentityRegistry.release(session: active.session)
            application.releaseSession(active.session, for: id)
        }
        if let next = graph.activeView, let session = sessionProjections[next.session] {
            projection = session
            refreshTree()
            refreshPaint()
        }
    }

    private func cycleTab(_ delta: Int) {
        guard var graph = editorGraph,
              graph.cycleTab(in: graph.activeGroupID, delta: delta),
              let active = graph.activeView,
              let session = sessionProjections[active.session]
        else { return }
        editorGraph = graph
        projection = session
        refreshTree()
        refreshPaint()
    }

    func prepareViewTransfer(copy: Bool) async -> WorkspaceViewTransfer? {
        guard phase == .ready,
              let graph = editorGraph,
              let active = graph.activeView,
              let session = sessionProjections[active.session]
        else {
            return nil
        }
        let generation = lifecycleGeneration
        guard copy else {
            return WorkspaceViewTransfer(view: active, session: session, removesSource: true)
        }
        let outcome = await application.coreHost.createView(
            target: active.session,
            expectedDocumentRevision: session.documentRevision
        ).value()
        guard generation == lifecycleGeneration,
              phase == .ready,
              editorGraph?.activeView?.id == active.id
        else {
            if case let .viewCreated(created) = outcome {
                _ = await application.coreHost.closeView(created.target).value()
            }
            lastCommandResult = .stale
            return nil
        }
        guard case let .viewCreated(created) = outcome else {
            if case let .failed(failure) = outcome {
                lastCommandResult = failure == .staleTarget ? .stale : .failed(failure)
            }
            return nil
        }
        let copy = WorkspaceViewRecord(
            id: WorkspaceViewID(rawValue: created.target.id.rawValue),
            coreTarget: created.target,
            session: created.session.target,
            viewRevision: created.viewRevision,
            title: "\(active.title) — View"
        )
        return WorkspaceViewTransfer(view: copy, session: created.session, removesSource: false)
    }

    @discardableResult
    func adoptViewTransfer(_ transfer: WorkspaceViewTransfer) -> Bool {
        guard phase == .ready, var graph = editorGraph,
              graph.appendAtomically([transfer.view], to: graph.activeGroupID)
        else {
            return false
        }
        sessionProjections[transfer.session.target] = transfer.session
        application.claimSession(transfer.session.target, for: id)
        editorGraph = graph
        projection = transfer.session
        lastCommandResult = .started
        refreshTree()
        return true
    }

    func completeMovedViewTransfer(_ viewID: WorkspaceViewID) {
        guard var graph = editorGraph,
              let removed = graph.extractForWindowTransfer(viewID: viewID)
        else {
            return
        }
        if let surface = surfaces.removeValue(forKey: removed.id) {
            _ = application.rendererHost.unregisterSurface(surface)
        }
        routes.removeValue(forKey: removed.id)
        drawableSizes.removeValue(forKey: removed.id)
        editorGraph = graph.groups.isEmpty ? nil : graph
        if !graph.allViews.contains(where: { $0.session == removed.session }) {
            sessionProjections.removeValue(forKey: removed.session)
            treeProjections.removeValue(forKey: removed.session)
            animationProjections.removeValue(forKey: removed.session)
            sessionDocumentURLs.removeValue(forKey: removed.session)
            sessionFileIdentities.removeValue(forKey: removed.session)
            application.fileIdentityRegistry.release(session: removed.session)
            application.releaseSession(removed.session, for: id)
        }
        guard let active = graph.activeView,
              let activeSession = sessionProjections[active.session]
        else {
            projection = nil
            window?.performClose(nil)
            return
        }
        projection = activeSession
        refreshTree()
    }

    func selectNode(layerID: UInt64, planeID: UInt64) {
        guard let context = layerPaneContext(),
              let target = sessionProjections[context.session]
        else { return }
        routeCommand(
            application.coreHost.setActiveNode(
                target: target.target,
                layerID: layerID,
                planeID: planeID,
                expectedDocumentRevision: context.documentRevision
            ),
            context: context
        )
    }

    func submitCellEditor(_ draft: CellEditorDraft) {
        guard pendingCellEditor?.id == draft.id,
              let context = pendingCellEditorContext,
              matches(context)
        else {
            lastCommandResult = .stale
            return
        }
        pendingCellEditor = nil
        pendingCellEditorContext = nil
        routeCommand(
            application.coreHost.editCell(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                command: draft.command
            ),
            context: context
        )
    }

    func submitTreeEditor(_ draft: TreeEditorDraft) {
        guard pendingTreeEditor?.id == draft.id,
              let context = pendingTreeEditorContext,
              matches(context)
        else {
            lastCommandResult = .stale
            return
        }
        pendingTreeEditor = nil
        pendingTreeEditorContext = nil
        routeTreeEdit(draft.command, context: context)
    }

    func cancelM5Editor() {
        pendingCellEditor = nil
        pendingTreeEditor = nil
        pendingCellEditorContext = nil
        pendingTreeEditorContext = nil
        lastCommandResult = .cancelled
    }

    private func routeTreeEdit(
        _ command: CoreTreeEditCommand,
        context: CommandTargetContext
    ) {
        routeCommand(
            application.coreHost.editTree(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                command: command
            ),
            context: context
        )
    }

    private func selectedLayer(for context: CommandTargetContext) -> CoreLayerProjection? {
        guard let tree = treeProjections[context.session] else { return nil }
        return tree.layers.first { $0.id == tree.activeLayerID }
    }

    private func selectedPlane(for context: CommandTargetContext) -> CoreNodeProjection? {
        guard let tree = treeProjections[context.session] else { return nil }
        return selectedLayer(for: context)?.planes.first { $0.id == tree.activePlaneID }
    }

    private func layerProperties(
        _ layer: CoreLayerProjection,
        visible: Bool? = nil,
        editable: Bool? = nil
    ) -> CoreTreeEditCommand {
        .setLayerProperties(
            id: layer.id,
            visible: visible ?? layer.isVisible,
            editable: editable ?? layer.isEditable,
            opacityMilli: layer.opacityMilli,
            name: layer.name
        )
    }

    private func planeProperties(
        _ plane: CoreNodeProjection,
        visible: Bool? = nil,
        editable: Bool? = nil
    ) -> CoreTreeEditCommand {
        .setPlaneProperties(
            id: plane.id,
            parentLayerID: plane.parentID,
            visible: visible ?? plane.isVisible,
            editable: editable ?? plane.isEditable,
            opacityMilli: plane.opacityMilli,
            name: plane.name
        )
    }

    private func routeCellEdit(
        _ command: CoreCellEditCommand,
        context: CommandTargetContext
    ) {
        routeCommand(
            application.coreHost.editCell(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                command: command
            ),
            context: context
        )
    }

    private func applyPreset(_ preset: WorkspacePreset) {
        var record = WorkspaceLayoutRecord.defaultColoring
        record.preset = preset
        switch preset {
        case .coloring:
            record.chrome = .defaultColoring
        case .lineCleanup:
            record.chrome = .defaultColoring
            record.chrome.inspectorEdge = .leading
            record.chrome.selectedInspectorSection = .vectorAnnotationGuides
        case .referenceCheck:
            record.chrome = .defaultColoring
            record.chrome.selectedInspectorSection = .animation
            record.split = .horizontal
        case .batch:
            record.chrome = .defaultColoring
            record.chrome.selectedInspectorSection = .animation
            record.split = .vertical
        case .focus:
            record.chrome.toolPresentation = .hidden
            record.chrome.inspectorRequestedVisible = false
        }
        restoreWorkspace(record)
    }

    private func saveWorkspace(named name: String?) {
        workspaceLayout = WorkspaceLayoutRecord(
            preset: workspaceLayout.preset,
            split: editorGraph?.splitOrientation,
            splitRatio: workspaceLayout.splitRatio,
            chrome: chromePreference,
            layerPlaneRatio: workspaceLayout.layerPlaneRatio,
            windowFrame: window?.frame ?? workspaceLayout.windowFrame,
            customName: name
        )
        lastCommandResult = layoutStore.save(workspaceLayout)
            ? .started : .failed(.invalidRequest)
    }

    private func requestWorkspaceName() {
        Task { @MainActor [weak self] in
            guard let self else { return }
            let alert = NSAlert()
            alert.messageText = application.languageController.text("m5.workspace.save.title")
            alert.addButton(withTitle: application.languageController.text("m5.action.save"))
            alert.addButton(withTitle: application.languageController.text("action.cancel"))
            let field = NSTextField(
                string: workspaceLayout.customName
                    ?? application.languageController.text("m5.workspace.defaultName")
            )
            field.frame = NSRect(x: 0, y: 0, width: 280, height: 24)
            alert.accessoryView = field
            guard await alertResponse(alert) == .alertFirstButtonReturn else {
                lastCommandResult = .cancelled
                return
            }
            let name = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !name.isEmpty, name.utf8.count <= 256 else {
                lastCommandResult = .invalid
                return
            }
            saveWorkspace(named: name)
        }
    }

    private func restoreSavedWorkspace() {
        guard let record = layoutStore.load(
            visibleWorkAreas: NSScreen.screens.map(\.visibleFrame)
        ) else {
            lastCommandResult = .noOp
            return
        }
        restoreWorkspace(record)
    }

    func updateSplitRatio(_ ratio: Double) {
        guard ratio.isFinite else { return }
        workspaceLayout.splitRatio = min(max(ratio, 0.2), 0.8)
    }

    private func restoreWorkspace(_ record: WorkspaceLayoutRecord) {
        workspaceLayout = record
        _ = reduceChrome(.restore(record.chrome))
        if let window { applyWorkspaceLayout(record, to: window) }
        if let split = record.split, editorGraph?.groups.count == 1 {
            createLogicalView(split: split)
        } else if record.split == nil, var graph = editorGraph, graph.groups.count == 2 {
            _ = graph.closeGroup(graph.groups.last!.id)
            editorGraph = graph
        }
        lastCommandResult = .started
    }

    private func applyWorkspaceLayout(_ record: WorkspaceLayoutRecord, to window: NSWindow) {
        workspaceLayout = record
        chromeAvailableWidth = Double(record.windowFrame.width)
        _ = reduceChrome(.restore(record.chrome))
        window.setFrame(record.windowFrame, display: true)
    }

    private func viewID(for context: CommandTargetContext) -> WorkspaceViewID? {
        editorGraph?.allViews.first { $0.coreTarget == context.view }?.id
    }

    private func selectedChartEntry(in session: CoreSessionTarget) -> CoreColorChartEntry? {
        guard let chart = paintProjections[session]?.chart else { return nil }
        let index = chartCursor ?? chart.selectedIndex ?? chart.entries.first?.index
        return chart.entries.first { $0.index == index }
    }

    private func mutatePalette(context: CommandTargetContext, operation: PaletteMutation) {
        guard let paint = paintProjections[context.session] else { return }
        var colors = paint.palette.colors
        switch operation {
        case .registerCurrent:
            if !colors.contains(paint.editor.currentColor) {
                colors.append(paint.editor.currentColor)
            }
        case .deleteCurrent:
            if let index = colors.firstIndex(of: paint.editor.currentColor) {
                colors.remove(at: index)
            } else if !colors.isEmpty {
                colors.removeLast()
            }
        case .clear:
            colors.removeAll(keepingCapacity: false)
        }
        observePaintMutation(
            application.coreHost.replacePalette(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                colors: colors
            ),
            requestsSnapshot: false
        )
    }

    private func replaceChart(
        context: CommandTargetContext,
        entries: [CoreColorChartEntry],
        locked: Bool
    ) {
        observePaintMutation(
            application.coreHost.replaceColorChart(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                entries: entries,
                locked: locked
            ),
            requestsSnapshot: false
        )
    }

    private func advanceChartCursor(
        session: CoreSessionTarget,
        matchingSearch: Bool
    ) {
        guard let entries = paintProjections[session]?.chart.entries, !entries.isEmpty else {
            chartCursor = nil
            return
        }
        let filtered = matchingSearch && !chartSearchText.isEmpty
            ? entries.filter { $0.name.localizedCaseInsensitiveContains(chartSearchText) }
            : entries
        guard !filtered.isEmpty else {
            chartCursor = nil
            return
        }
        let current = filtered.firstIndex { $0.index == chartCursor } ?? -1
        chartCursor = filtered[(current + 1) % filtered.count].index
    }

    private func pasteChart(context: CommandTargetContext) {
        guard !chartClipboard.isEmpty,
              let chart = paintProjections[context.session]?.chart
        else { return }
        var entries = chart.entries
        var next = (entries.map(\.index).max() ?? 0) + 1
        for copied in chartClipboard {
            entries.append(CoreColorChartEntry(
                index: next,
                color: copied.color,
                name: copied.name,
                frequency: copied.frequency
            ))
            next += 1
        }
        replaceChart(context: context, entries: entries, locked: chart.isLocked)
    }

    private func cutChart(context: CommandTargetContext) {
        guard let selected = selectedChartEntry(in: context.session),
              let chart = paintProjections[context.session]?.chart
        else { return }
        chartClipboard = [selected]
        replaceChart(
            context: context,
            entries: chart.entries.filter { $0.index != selected.index },
            locked: chart.isLocked
        )
    }

    private func presentChartRename(context: CommandTargetContext) {
        guard let selected = selectedChartEntry(in: context.session),
              let chart = paintProjections[context.session]?.chart
        else { return }
        Task {
            let alert = NSAlert()
            alert.messageText = application.languageController.text("m6.chart.rename.title")
            alert.addButton(withTitle: application.languageController.text("action.apply"))
            alert.addButton(withTitle: application.languageController.text("action.cancel"))
            let field = NSTextField(string: selected.name)
            field.frame = NSRect(x: 0, y: 0, width: 280, height: 24)
            alert.accessoryView = field
            guard await alertResponse(alert) == .alertFirstButtonReturn else {
                lastCommandResult = .cancelled
                return
            }
            let name = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !name.isEmpty, name.utf8.count <= 4_096, matches(context) else {
                lastCommandResult = matches(context) ? .invalid : .stale
                return
            }
            let entries = chart.entries.map { entry in
                entry.index == selected.index
                    ? CoreColorChartEntry(
                        index: entry.index,
                        color: entry.color,
                        name: name,
                        frequency: entry.frequency
                    ) : entry
            }
            replaceChart(context: context, entries: entries, locked: chart.isLocked)
        }
    }

    private func presentPaintDataPanel(
        kind: PaintDataKind,
        saving: Bool,
        context: CommandTargetContext
    ) {
        Task {
            let contentType = UTType(
                exportedAs: kind == .palette
                    ? "com.openai.inkpod.palette" : "com.openai.inkpod.color-chart",
                conformingTo: .data
            )
            let panel: NSSavePanel = saving ? NSSavePanel() : NSOpenPanel()
            panel.allowedContentTypes = [contentType]
            if saving {
                panel.nameFieldStringValue = kind == .palette
                    ? "Colors.inkpalette" : "Colors.inkchart"
            }
            guard await panelResponse(panel) == .OK, let url = panel.url else {
                lastCommandResult = .cancelled
                return
            }
            guard matches(context) else {
                lastCommandResult = .stale
                return
            }
            let lease = SecurityScopedResourceLease(url: url)
            defer { lease.close() }
            let path = Array(url.path.utf8)
            let task: CoreTask = switch (kind, saving) {
            case (.palette, true):
                application.coreHost.savePaletteFile(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision,
                    pathUTF8: path
                )
            case (.palette, false):
                application.coreHost.loadPaletteFile(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision,
                    pathUTF8: path
                )
            case (.chart, true):
                application.coreHost.saveColorChartFile(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision,
                    pathUTF8: path
                )
            case (.chart, false):
                application.coreHost.loadColorChartFile(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision,
                    pathUTF8: path
                )
            }
            let outcome = await task.value()
            guard lifecycleGeneration == context.lifecycleGeneration else { return }
            switch outcome {
            case .acknowledged:
                lastCommandResult = .started
                refreshPaint()
            case .noOp:
                lastCommandResult = .noOp
            case .failed(.staleTarget):
                lastCommandResult = .stale
            case let .failed(failure):
                lastCommandResult = .failed(failure)
            default:
                lastCommandResult = .invalid
            }
        }
    }

    private func updateFillOperationAndSelectTool(
        _ operation: CoreFillOperation,
        context: CommandTargetContext
    ) {
        guard let viewID = viewID(for: context),
              let paint = paintProjections[context.session],
              let expectation = paintExpectation(
                for: viewRecord(viewID)!,
                paint: paint
              )
        else { return }
        let options = paint.editor.fillOptions.replacingOperation(operation)
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            guard let self else { return }
            let first = await application.coreHost.updateEditor(
                target: context.view,
                expectation: expectation,
                update: .fillOptions(options)
            ).value()
            guard generation == lifecycleGeneration else { return }
            let updated: CorePaintProjection
            switch first {
            case let .paintUpdated(value): updated = value
            case .noOp: updated = paint
            case .failed(.staleTarget):
                lastCommandResult = .stale
                refreshPaint()
                return
            case let .failed(failure):
                lastCommandResult = .failed(failure)
                return
            default:
                lastCommandResult = .invalid
                return
            }
            paintProjections[context.session] = updated
            let followupExpectation = paintExpectation(for: viewRecord(viewID)!, paint: updated)!
            observePaintMutation(
                application.coreHost.updateEditor(
                    target: context.view,
                    expectation: followupExpectation,
                    update: .activeTool(.fill)
                ),
                requestsSnapshot: false,
                viewID: viewID
            )
        }
    }

    private func updateSelectionOptionsAndSelectTool(
        shape: CoreSelectionShape? = nil,
        operation: CoreSelectionOperation? = nil,
        context: CommandTargetContext
    ) {
        guard let viewID = viewID(for: context),
              let view = viewRecord(viewID),
              let paint = paintProjections[context.session],
              let expectation = paintExpectation(for: view, paint: paint)
        else { return }
        var options = paint.editor.selectionOptions
        if let shape { options = options.replacingShape(shape) }
        if let operation { options = options.replacingOperation(operation) }
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            guard let self else { return }
            let first = await application.coreHost.updateEditor(
                target: context.view,
                expectation: expectation,
                update: .selectionOptions(options)
            ).value()
            guard generation == lifecycleGeneration else { return }
            let updated: CorePaintProjection
            switch first {
            case let .paintUpdated(value): updated = value
            case .noOp: updated = paint
            case .failed(.staleTarget):
                lastCommandResult = .stale
                refreshPaint()
                return
            case let .failed(failure):
                lastCommandResult = .failed(failure)
                return
            default:
                lastCommandResult = .invalid
                return
            }
            paintProjections[context.session] = updated
            guard let currentView = viewRecord(viewID),
                  let followupExpectation = paintExpectation(
                      for: currentView,
                      paint: updated
                  )
            else { return }
            observePaintMutation(
                application.coreHost.updateEditor(
                    target: context.view,
                    expectation: followupExpectation,
                    update: .activeTool(.selection)
                ),
                requestsSnapshot: false,
                viewID: viewID
            )
        }
    }

    private func routeSelectionAdjust(
        _ operation: CoreSelectionAdjustOperation,
        pixels: UInt32,
        context: CommandTargetContext
    ) {
        observePaintMutation(
            application.coreHost.selectionAdjust(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                operation: operation,
                pixels: pixels
            ),
            requestsSnapshot: true,
            viewID: viewID(for: context)
        )
    }

    var commandContext: CommandTargetContext? {
        guard phase == .ready, let projection, let view = editorGraph?.activeView else { return nil }
        return CommandTargetContext(
            workspaceID: id,
            lifecycleGeneration: lifecycleGeneration,
            session: projection.target,
            view: view.coreTarget,
            documentRevision: projection.documentRevision,
            viewRevision: view.viewRevision
        )
    }

    func commandState(_ command: InkpodCommandID, context: CommandTargetContext) -> CommandState {
        guard matches(context), let projection = sessionProjections[context.session] else {
            return CommandState(enabled: false)
        }
        let animation = animationProjections[context.session]
        let selectedLayer = selectedLayer(for: context)
        let selectedPlane = selectedPlane(for: context)
        switch command {
        case .fileNewCut:
            return CommandState(enabled: !isFileOperationActive && cut == nil)
        case .cutProperties:
            return CommandState(enabled: cut != nil && !isFileOperationActive)
        case .cutSave:
            return CommandState(
                enabled: cut?.isDirty == true && !isFileOperationActive
            )
        case .cutUndo:
            return CommandState(enabled: cut?.canUndo == true)
        case .cutRedo:
            return CommandState(enabled: cut?.canRedo == true)
        case .cutSequenceAdd:
            return CommandState(
                enabled: cut != nil && cutURL != nil && documentURL != nil
                    && projection.cellID > 0
                    && !projection.isDirty
            )
        case .cutSequenceRemove:
            return CommandState(enabled: selectedCutMember != nil)
        case .cutSequenceMoveUp:
            return CommandState(enabled: (selectedCutMemberIndex ?? 0) > 0)
        case .cutSequenceMoveDown:
            return CommandState(
                enabled: selectedCutMemberIndex.map {
                    $0 + 1 < (cut?.members.count ?? 0)
                } ?? false
            )
        case .cutSequenceRenumber:
            return CommandState(enabled: !(cut?.members.isEmpty ?? true))
        case .sequenceImport:
            return CommandState(enabled: !isFileOperationActive)
        case .sequenceExport:
            return CommandState(
                enabled: !(animation?.sequence.isEmpty ?? true) && !isFileOperationActive
            )
        case .sequencePrevious, .sequenceNext, .sequenceGoto:
            return CommandState(enabled: !(animation?.sequence.isEmpty ?? true))
        case .sequenceWrapEndpoints:
            return CommandState(
                enabled: true,
                checked: application.sequenceEndpointPolicyController.policy == .wrap
            )
        case .lightTableSetNew:
            return CommandState(enabled: !projection.hasActiveTransient)
        case .lightTableSetDuplicate, .lightTableSetDelete, .lightTableSetRename,
             .lightTableSetUp, .lightTableSetDown, .lightTableGlobalOpacity:
            return CommandState(
                enabled: selectedLightTableSet != nil && !projection.hasActiveTransient
            )
        case .lightTableItemAdd:
            return CommandState(
                enabled: selectedLightTableSet != nil && !isFileOperationActive
                    && !projection.hasActiveTransient
            )
        case .lightTableItemReload, .lightTableItemDelete, .lightTableItemUp,
             .lightTableItemDown, .lightTableItemProperties,
             .lightTableItemSample, .lightTableItemSwap, .lightTableItemMove:
            return CommandState(
                enabled: selectedLightTableItem != nil && !projection.hasActiveTransient
            )
        case .lightTableBulkPrevious, .lightTableBulkNext, .lightTableBulkBoth:
            return CommandState(
                enabled: selectedLightTableSet != nil
                    && !(animation?.sequence.isEmpty ?? true)
                    && !projection.hasActiveTransient
            )
        case .subpaletteSet, .subpaletteSample:
            return CommandState(enabled: !(animation?.sequence.isEmpty ?? true))
        case .motionStart:
            return CommandState(
                enabled: !(animation?.sequence.isEmpty ?? true) && animation?.motion == nil
            )
        case .motionPause, .motionPrevious, .motionNext, .motionStop:
            return CommandState(
                enabled: animation?.motion != nil,
                checked: command == .motionPause && animation?.motion?.isPaused == true
            )
        case .motionFirst, .motionLast:
            return CommandState(enabled: !(animation?.sequence.isEmpty ?? true))
        case .motionFPS30, .motionFPS25, .motionFPS24, .motionFPS12,
             .motionFPS10, .motionFPS8:
            return CommandState(
                enabled: animation?.motion == nil,
                checked: motionFPS == motionFPSValue(for: command)
            )
        case .windowSequence:
            return CommandState(enabled: true, checked: sequenceVisible)
        case .sequencePin:
            return CommandState(enabled: true, checked: sequencePaneTarget.isPinned)
        case .windowLightTable:
            return CommandState(
                enabled: true,
                checked: inspectorSectionIsActive(.animation) && lightTableVisible
            )
        case .lightTablePin:
            return CommandState(enabled: true, checked: lightTablePaneTarget.isPinned)
        case .windowSubpalette:
            return CommandState(
                enabled: true,
                checked: inspectorSectionIsActive(.animation) && subpaletteVisible
            )
        case .subpalettePin:
            return CommandState(enabled: true, checked: subpalettePaneTarget.isPinned)
        case .fileNew, .fileOpen, .fileOpenRecovery, .fileImportRaster,
             .fileSave, .fileSaveAs, .fileExportRaster, .fileCompactCopy,
             .documentClose:
            return CommandState(enabled: !isFileOperationActive)
        case .fileOpenRecent:
            return CommandState(
                enabled: !isFileOperationActive && !application.recentURLs.isEmpty
            )
        case .fileRestorePrevious:
            return CommandState(enabled: !isFileOperationActive)
        case .fileRevert:
            return CommandState(
                enabled: !isFileOperationActive && documentURL != nil && projection.isDirty
            )
        case .fileRevertPartial:
            return CommandState(enabled: !isFileOperationActive && projection.isDirty)
        case .fileAutosaveNow:
            return CommandState(enabled: !isFileOperationActive && projection.isDirty)
        case .fileSequenceAutosave:
            return CommandState(
                enabled: !isFileOperationActive
                    && (projection.isDirty || cut?.isDirty == true)
            )
        case .editCopy, .editCut:
            return CommandState(
                enabled: !isFileOperationActive && !projection.hasActiveTransient
            )
        case .editPaste, .editPasteSelected, .editPasteConverted:
            return CommandState(
                enabled: !isFileOperationActive && !projection.hasActiveTransient
                    && application.clipboardBroker.hasPasteableRepresentation()
            )
        case .undo:
            return CommandState(enabled: projection.canUndo)
        case .redo:
            return CommandState(enabled: projection.canRedo)
        case .historyBack:
            return CommandState(enabled: projection.canUndo)
        case .historyForward:
            return CommandState(enabled: projection.canRedo)
        case .editMirrorHorizontal:
            return CommandState(enabled: !projection.hasActiveTransient)
        case .floatingTransform, .floatingCommit, .floatingCancel:
            return CommandState(
                enabled: projection.hasActiveTransient && pendingPasteConfirmation != nil,
                checked: command == .floatingTransform && floatingTransformEditor != nil
            )
        case .ruler:
            return CommandState(enabled: true, checked: presentationState.rulerVisible)
        case .guides:
            return CommandState(enabled: true, checked: presentationState.guidesVisible)
        case .grid:
            return CommandState(enabled: true, checked: presentationState.gridVisible)
        case .snapGuides:
            return CommandState(enabled: true, checked: presentationState.guideSnapEnabled)
        case .snapGrid:
            return CommandState(enabled: true, checked: presentationState.gridSnapEnabled)
        case .transparent:
            return CommandState(enabled: true, checked: presentationState.transparentVisible)
        case .flipHorizontal:
            return CommandState(enabled: true, checked: presentationState.flipHorizontal)
        case .flipVertical:
            return CommandState(enabled: true, checked: presentationState.flipVertical)
        case .guideMove:
            return CommandState(enabled: !guidePositions.isEmpty)
        case .zoomIn, .zoomOut, .fit, .oneToOne, .zoomPercent, .boxZoom,
             .guideVertical, .guideHorizontal, .guideDeleteAll, .gridSettings:
            return CommandState(enabled: true)
        case .viewNew, .workspaceNewWindow, .workspaceReset, .workspaceSave,
             .workspaceRestore, .workspaceMirror, .workspacePresetColoring,
             .workspacePresetLineCleanup, .workspacePresetReference,
             .workspacePresetBatch, .workspacePresetFocus, .workspaceSaveAs,
             .windowLayerPalette:
            return CommandState(enabled: true, checked: command == .windowLayerPalette
                && inspectorSectionIsActive(.layerPlane))
        case .viewClose:
            return CommandState(enabled: (editorGraph?.allViews.count ?? 0) > 1)
        case .tabNext, .tabPrevious, .tabMoveLeft, .tabMoveRight:
            let tabCount = editorGraph.flatMap { graph in
                graph.group(id: graph.activeGroupID)?.views.count
            } ?? 0
            return CommandState(enabled: tabCount > 1)
        case .editorSplitRight, .editorSplitDown:
            return CommandState(enabled: editorGraph?.groups.count == 1)
        case .editorMoveOtherGroup, .editorNewViewOtherGroup, .editorGroupClose,
             .editorGroupNext:
            return CommandState(enabled: editorGraph?.groups.count == 2)
        case .viewMoveNextWindow, .viewDuplicateNextWindow:
            return CommandState(enabled: application.hasOtherReadyWorkspace(than: id))
        case .viewMoveNewWindow, .viewDuplicateNewWindow:
            return CommandState(enabled: true)
        case .planeMainLine:
            return CommandState(enabled: selectedLayer?.planes.contains { $0.kind == .mainLine } == true)
        case .planeColor:
            return CommandState(enabled: selectedLayer?.planes.contains { $0.kind == .color } == true)
        case .layerDuplicate, .layerDelete, .layerMoveTop, .layerMoveUp,
             .layerMoveDown, .layerToggleVisible, .layerToggleEditable,
             .layerOpacity, .layerConvert, .layerMerge, .layerProperties:
            return CommandState(enabled: selectedLayer != nil && !projection.hasActiveTransient)
        case .planeDuplicate, .planeDelete, .planeMoveUp, .planeMoveDown,
             .planeToggleVisible, .planeToggleEditable, .planeOpacity,
             .planeConvert, .planeMerge, .planeProperties:
            return CommandState(enabled: selectedPlane != nil && !projection.hasActiveTransient)
        case .layerNew, .layerDeleteHidden, .planeNew, .cellPaperSettings,
             .cellFrameHundred, .cellFrameReference, .cellFrameDrawing,
             .cellFrameSafe, .cellMargins, .cellMirrorVertical, .cellRotateLeft,
             .cellRotateRight, .cellImageSize, .cellResolution,
             .cellFitCaptureFrame:
            return CommandState(enabled: !projection.hasActiveTransient)
        case .toolPencil, .toolBrush, .toolEraser, .toolFill, .toolEyedropper:
            let active = paintProjections[context.session]?.editor.activeTool
            let expected: CoreEditorTool = switch command {
            case .toolPencil: .pencil
            case .toolBrush: .brush
            case .toolEraser: .eraser
            case .toolFill: .fill
            default: .eyedropper
            }
            return CommandState(
                enabled: paintProjections[context.session] != nil
                    && !projection.hasActiveTransient,
                checked: active == expected
            )
        case .toolFillOptions:
            return CommandState(
                enabled: paintProjections[context.session] != nil,
                checked: toolOptionsPresentation.tool == .fill
            )
        case .toolClosedFill, .toolFillExtension:
            let operation = paintProjections[context.session]?.editor.fillOptions.operation
            return CommandState(
                enabled: paintProjections[context.session] != nil
                    && !projection.hasActiveTransient,
                checked: operation == (command == .toolClosedFill
                    ? .closedRegion : .extensionRegion)
            )
        case .toolColorReplaceTarget:
            return CommandState(
                enabled: paintProjections[context.session] != nil,
                checked: paintProjections[context.session]?.editor.activeTool == .colorReplace
            )
        case .toolColorReplacePen, .toolColorReplaceRectangle,
             .toolColorReplacePolyline, .toolColorReplaceLasso, .toolColorReplaceAll:
            let region: ColorReplaceRegionTool = switch command {
            case .toolColorReplacePen: .pen
            case .toolColorReplaceRectangle: .rectangle
            case .toolColorReplacePolyline: .polyline
            case .toolColorReplaceLasso: .lasso
            default: .all
            }
            return CommandState(
                enabled: paintProjections[context.session] != nil
                    && !projection.hasActiveTransient,
                checked: colorReplaceRegionTool == region
            )
        case .colorChoose, .colorEditor:
            return CommandState(enabled: paintProjections[context.session] != nil)
        case .colorCheckOff, .colorCheckLegacy, .colorCheckNative:
            let mode = paintProjections[context.session]?.colorCheckMode
            let expected: CoreColorCheckMode = switch command {
            case .colorCheckLegacy: .legacyWhite
            case .colorCheckNative: .nativeAlpha
            default: .off
            }
            return CommandState(enabled: true, checked: mode == expected)
        case .colorSourceTopmost, .colorSourceSelected, .colorSourceComposite,
             .colorSourceLightTable:
            let source: CoreEyedropperSource = switch command {
            case .colorSourceTopmost: .topmostNontransparent
            case .colorSourceSelected: .selectedPlane
            case .colorSourceLightTable: .lightTableTopmost
            default: .composite
            }
            return CommandState(enabled: true, checked: eyedropperSource == source)
        case .paletteRegister, .paletteClear, .paletteLoad, .chartGenerate,
             .chartPaste, .chartLoad:
            return CommandState(enabled: !projection.hasActiveTransient)
        case .paletteDelete:
            return CommandState(
                enabled: !(paintProjections[context.session]?.palette.colors.isEmpty ?? true)
                    && !projection.hasActiveTransient
            )
        case .paletteSave:
            return CommandState(
                enabled: !(paintProjections[context.session]?.palette.colors.isEmpty ?? true)
            )
        case .paletteNextGroup:
            return CommandState(
                enabled: (paintProjections[context.session]?.palette.colors.count ?? 0) > 16
            )
        case .chartSearch:
            return CommandState(
                enabled: !(paintProjections[context.session]?.chart.entries.isEmpty ?? true)
            )
        case .chartNext, .chartCopy, .chartCut, .chartRename:
            return CommandState(
                enabled: selectedChartEntry(in: context.session) != nil
                    && (command == .chartCopy || !projection.hasActiveTransient)
            )
        case .chartLock:
            return CommandState(
                enabled: paintProjections[context.session] != nil,
                checked: paintProjections[context.session]?.chart.isLocked == true
            )
        case .chartSave:
            return CommandState(
                enabled: !(paintProjections[context.session]?.chart.entries.isEmpty ?? true)
            )
        case .chartNextPage:
            return CommandState(
                enabled: (paintProjections[context.session]?.chart.entries.count ?? 0) > 32
            )
        case .selectionOutputColorGuard:
            return CommandState(enabled: !projection.hasActiveTransient)
        case .selectionAll, .selectionInvert, .selectionExpand, .selectionShrink,
             .selectionClear, .selectionColor, .selectionColorDifferent,
             .selectionColorAdd, .selectionToLayer:
            return CommandState(
                enabled: paintProjections[context.session] != nil
                    && !projection.hasActiveTransient
            )
        case .selectionFromLayer, .selectionLayerAdd, .selectionLayerSubtract:
            return CommandState(
                enabled: selectedLayer?.kind == .selection
                    && !projection.hasActiveTransient
            )
        case .selectionRectangle, .selectionEllipse, .selectionLasso,
             .selectionPolyline, .selectionTrace, .selectionWand:
            let expected: CoreSelectionShape = switch command {
            case .selectionRectangle: .rectangle
            case .selectionEllipse: .ellipse
            case .selectionLasso: .lasso
            case .selectionPolyline: .polyline
            case .selectionTrace: .trace
            default: .wand
            }
            return CommandState(
                enabled: paintProjections[context.session] != nil
                    && !projection.hasActiveTransient,
                checked: paintProjections[context.session]?.editor.activeTool == .selection
                    && paintProjections[context.session]?.editor.selectionOptions.shape == expected
            )
        case .selectionModeNew, .selectionModeAdd, .selectionModeSubtract,
             .selectionModeIntersect:
            let expected: CoreSelectionOperation = switch command {
            case .selectionModeAdd: .add
            case .selectionModeSubtract: .subtract
            case .selectionModeIntersect: .intersect
            default: .replace
            }
            return CommandState(
                enabled: paintProjections[context.session] != nil
                    && !projection.hasActiveTransient,
                checked: paintProjections[context.session]?.editor.selectionOptions.operation
                    == expected
            )
        case .selectionOptions:
            return CommandState(
                enabled: paintProjections[context.session] != nil,
                checked: toolOptionsPresentation.tool == .selection
            )
        case .windowToolPalette:
            return CommandState(
                enabled: true,
                checked: adaptiveChrome.toolPresentation != .hidden
            )
        case .windowToolOptions:
            return CommandState(
                enabled: paintProjections[context.session] != nil,
                checked: toolOptionsPresentation.isPresented
            )
        case .windowColorPane:
            return CommandState(enabled: true, checked: inspectorSectionIsActive(.color))
        case .windowLocator:
            return CommandState(
                enabled: true,
                checked: inspectorSectionIsActive(.color) && locatorVisible
            )
        case .locatorPin:
            return CommandState(enabled: true, checked: locatorPaneIsPinned)
        case .locatorFixed:
            return CommandState(
                enabled: inspectorSectionIsActive(.color) && locatorVisible,
                checked: locatorFixed
            )
        case .locatorAutoscroll:
            return CommandState(
                enabled: inspectorSectionIsActive(.color) && locatorVisible,
                checked: locatorAutoscroll
            )
        case .colorPin:
            return CommandState(enabled: true, checked: colorPaneIsPinned)
        case .fileExportInstructionRaster:
            return CommandState(enabled: !isFileOperationActive && !projection.hasActiveTransient)
        case .viewVectorAntialias:
            return CommandState(enabled: true, checked: presentationState.vectorAntialias)
        case .viewVectorCenterline:
            return CommandState(
                enabled: true,
                checked: presentationState.vectorCenterlineMode == 1
            )
        case .viewVectorCenterlineOnly:
            return CommandState(
                enabled: true,
                checked: presentationState.vectorCenterlineMode == 2
            )
        case .viewVectorEndpoints:
            return CommandState(enabled: true, checked: presentationState.vectorEndpointsVisible)
        case .filterLast, .filterInvert, .filterBlurWeak, .filterSharpenWeak,
             .filterSharpenStrong, .filterBlurStrong, .filterGaussian,
             .filterAutoContrast, .filterBrightness, .filterToneCurve,
             .filterLevels, .filterHSV, .filterColorBalance, .filterUnsharp,
             .effectGradient, .effectAirbrush, .effectBoundaryAirbrush,
             .effectBlur, .effectStamp, .effectDust, .effectAlphaGradient:
            return CommandState(
                enabled: selectedPlane?.pixelFormat != CorePixelStorageFormat.none
                    && selectedPlane?.isEditable == true
                    && !projection.hasActiveTransient
            )
        case .effectAlphaView:
            return CommandState(
                enabled: true,
                checked: presentationState.alphaVisible
            )
        case .adjustmentCreate:
            return CommandState(
                enabled: selectedPlane?.pixelFormat != CorePixelStorageFormat.none
                    && !projection.hasActiveTransient
            )
        case .adjustmentEdit, .adjustmentToggle, .adjustmentMoveTop:
            return CommandState(
                enabled: selectedLayer?.kind == .adjustment && !projection.hasActiveTransient
            )
        case .adjustmentPrevious, .adjustmentNext:
            return CommandState(
                enabled: cellTree?.layers.contains(where: { $0.kind == .adjustment }) == true
            )
        case .vectorLine, .vectorCurve, .vectorRectangle, .vectorEllipse,
             .vectorPolyline, .vectorPolygon, .vectorEraser, .vectorErasePartial,
             .vectorEraseIntersection, .vectorEraseWhole, .vectorConnect,
             .vectorWidth, .vectorSelectCut, .vectorSelectTouch,
             .vectorSelectContained, .vectorSelectLine, .vectorSelectWholeLine,
             .vectorSelectIntersection, .vectorSelectFillBoundary,
             .vectorSelectFill, .vectorRasterize:
            return CommandState(
                enabled: selectedLayer?.kind == .vectorColoring
                    && selectedPlane?.isEditable == true && !projection.hasActiveTransient,
                checked: m8CommandIsSelected(command)
            )
        case .vectorVectorize:
            return CommandState(
                enabled: selectedPlane?.pixelFormat != CorePixelStorageFormat.none
                    && cellTree?.layers.contains(where: { $0.kind == .vectorColoring }) == true
                    && !projection.hasActiveTransient
            )
        case .geometryOptions:
            return CommandState(enabled: true, checked: pendingM8Editor?.id == m8GeometryOptions.id)
        case .annotationAddText, .annotationDrawInstruction:
            return CommandState(
                enabled: selectedLayer?.kind == .annotation && !projection.hasActiveTransient,
                checked: command == .annotationDrawInstruction
                    && activeM8CanvasTool == .annotationStroke
            )
        case .annotationEditText, .annotationMoveLeft, .annotationMoveRight,
             .annotationDelete:
            return CommandState(enabled: selectedAnnotationID != nil && !projection.hasActiveTransient)
        case .annotationSelectPrevious, .annotationSelectNext:
            return CommandState(enabled: !knownAnnotationIDs.isEmpty)
        case .cellShootingFrameProperties, .cellShootingFrameEditHandles,
             .cellShootingFrameDelete, .cellVanishingPointProperties,
             .cellVanishingPointEditHandles, .cellVanishingPointDeleteAll:
            let hasFrame = m8State?.shootingFrame != nil
            let hasPoint = !(m8State?.vanishingPoints.isEmpty ?? true)
            let hasVanishingPointLayer = cellTree?.layers.contains {
                $0.kind == .vanishingPoint && $0.isEditable
            } == true
            let enabled = switch command {
            case .cellShootingFrameEditHandles, .cellShootingFrameDelete: hasFrame
            case .cellVanishingPointProperties: hasVanishingPointLayer
            case .cellVanishingPointEditHandles, .cellVanishingPointDeleteAll: hasPoint
            default: true
            }
            return CommandState(
                enabled: enabled && !projection.hasActiveTransient,
                checked: (command == .cellShootingFrameEditHandles
                    && activeM8CanvasTool == .shootingFrameHandles)
                    || (command == .cellVanishingPointEditHandles
                        && activeM8CanvasTool == .vanishingPointHandles)
            )
        default:
            return CommandState(enabled: false)
        }
    }

    @discardableResult
    func execute(_ command: InkpodCommandID, context: CommandTargetContext) -> CommandRouteResult {
        guard matches(context), let projection = sessionProjections[context.session] else {
            return .stale
        }
        guard commandState(command, context: context).enabled else { return .noOp }
        let selectedLayer = selectedLayer(for: context)
        let selectedPlane = selectedPlane(for: context)
        let expectation = CoreCommandExpectation(
            documentRevision: context.documentRevision,
            viewRevision: context.viewRevision
        )
        switch command {
        case .fileNewCut, .cutProperties, .cutSave, .cutUndo, .cutRedo,
             .cutSequenceAdd, .cutSequenceRemove, .cutSequenceMoveUp,
             .cutSequenceMoveDown, .cutSequenceRenumber,
             .lightTableSetNew, .lightTableSetDuplicate, .lightTableSetDelete,
             .lightTableSetRename, .lightTableSetUp, .lightTableSetDown,
             .lightTableGlobalOpacity, .lightTableItemAdd,
             .lightTableItemReload, .lightTableItemDelete, .lightTableItemUp,
             .lightTableItemDown, .lightTableItemProperties,
             .lightTableItemSample, .lightTableItemSwap, .lightTableItemMove,
             .lightTableBulkPrevious, .lightTableBulkNext, .lightTableBulkBoth,
             .sequenceImport, .sequenceExport, .sequencePrevious,
             .sequenceNext, .sequenceGoto, .subpaletteSet,
             .subpaletteSample, .motionStart, .motionPause, .motionPrevious,
             .motionNext, .motionStop, .motionFirst, .motionLast,
             .motionFPS30, .motionFPS25, .motionFPS24, .motionFPS12,
             .motionFPS10, .motionFPS8, .sequenceWrapEndpoints,
             .windowSequence, .sequencePin, .windowLightTable,
             .lightTablePin, .windowSubpalette, .subpalettePin:
            return executeM9(command, context: context)
        case .fileNew:
            beginNewCell()
            return .presentedInput
        case .fileOpen:
            presentOpenPanel()
            return .presentedInput
        case .fileImportRaster:
            presentOpenPanel(rasterOnly: true)
            return .presentedInput
        case .fileOpenRecovery:
            presentOpenPanel(recovery: true)
            return .presentedInput
        case .fileSave:
            Task { _ = await save(chooseDestination: false) }
        case .fileSaveAs:
            Task { _ = await save(chooseDestination: true, allowClean: true) }
        case .fileRevert:
            Task {
                guard await confirmRevert() else {
                    lastCommandResult = .cancelled
                    return
                }
                revert(partial: false)
            }
            return .presentedInput
        case .fileRevertPartial:
            revert(partial: true)
        case .fileAutosaveNow:
            Task { await autosaveNow() }
        case .fileSequenceAutosave:
            if cut?.isDirty == true {
                Task { await autosaveCutNow() }
            } else {
                Task { await autosaveNow() }
            }
        case .fileExportRaster:
            exportRaster()
            return .presentedInput
        case .fileOpenRecent:
            guard let recent = application.recentURLs.first else { return .noOp }
            Task { await openURL(recent, recovery: false) }
        case .fileRestorePrevious:
            return application.toggleRestorePreviousDocumentsAtStartup()
                ? .started : .failed(.invalidRequest)
        case .fileCompactCopy:
            compactedCopy()
            return .presentedInput
        case .editCopy:
            copy(cut: false)
        case .editCut:
            copy(cut: true)
        case .editPaste:
            paste(mode: .compatible)
        case .editPasteSelected:
            paste(mode: .activePlaneConverted)
        case .editPasteConverted:
            paste(mode: .newRasterPlane(CoreNewPlanePaste(name: "Pasted Raster")))
            return .presentedInput
        case .documentClose:
            window?.performClose(nil)
            return .presentedInput
        case .undo:
            routeCommand(
                application.coreHost.undo(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision
                ),
                context: context
            )
        case .redo:
            routeCommand(
                application.coreHost.redo(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision
                ),
                context: context
            )
        case .historyBack:
            routeCommand(
                application.coreHost.undo(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision
                ),
                context: context
            )
        case .historyForward:
            routeCommand(
                application.coreHost.redo(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision
                ),
                context: context
            )
        case .editMirrorHorizontal:
            routeCellEdit(.mirror(.horizontal), context: context)
        case .floatingTransform:
            guard let draft = pendingFloatingTransform else { return .noOp }
            floatingTransformEditor = draft
            return .presentedInput
        case .floatingCommit:
            applyPendingPaste()
        case .floatingCancel:
            cancelPendingPaste()
        case .selectionAll:
            observePaintMutation(
                application.coreHost.selectAll(
                    context.session,
                    expectedDocumentRevision: context.documentRevision
                ),
                requestsSnapshot: true,
                viewID: viewID(for: context)
            )
        case .selectionInvert:
            routeSelectionAdjust(.invert, pixels: 0, context: context)
        case .selectionExpand:
            pendingCommandInput = .selectionAdjust(.expand, 1)
            return .presentedInput
        case .selectionShrink:
            pendingCommandInput = .selectionAdjust(.shrink, 1)
            return .presentedInput
        case .selectionClear:
            observePaintMutation(
                application.coreHost.clearSelection(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision
                ),
                requestsSnapshot: true,
                viewID: viewID(for: context)
            )
        case .selectionRectangle, .selectionEllipse, .selectionLasso,
             .selectionPolyline, .selectionTrace, .selectionWand:
            let shape: CoreSelectionShape = switch command {
            case .selectionRectangle: .rectangle
            case .selectionEllipse: .ellipse
            case .selectionLasso: .lasso
            case .selectionPolyline: .polyline
            case .selectionTrace: .trace
            default: .wand
            }
            updateSelectionOptionsAndSelectTool(shape: shape, context: context)
        case .selectionModeNew, .selectionModeAdd, .selectionModeSubtract,
             .selectionModeIntersect:
            let operation: CoreSelectionOperation = switch command {
            case .selectionModeAdd: .add
            case .selectionModeSubtract: .subtract
            case .selectionModeIntersect: .intersect
            default: .replace
            }
            updateSelectionOptionsAndSelectTool(operation: operation, context: context)
        case .selectionColor, .selectionColorDifferent, .selectionColorAdd:
            guard let viewID = viewID(for: context),
                  let view = viewRecord(viewID),
                  let paint = paintProjections[context.session],
                  let paintExpectation = paintExpectation(for: view, paint: paint)
            else { return .noOp }
            observePaintMutation(
                application.coreHost.selectColor(
                    target: context.view,
                    expectation: paintExpectation,
                    different: command == .selectionColorDifferent,
                    operation: command == .selectionColorAdd ? .add : .replace
                ),
                requestsSnapshot: true,
                viewID: viewID
            )
        case .selectionToLayer:
            observePaintMutation(
                application.coreHost.selectionToLayer(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision,
                    nameUTF8: Array("Selection".utf8)
                ),
                requestsSnapshot: true,
                viewID: viewID(for: context)
            )
        case .selectionFromLayer, .selectionLayerAdd, .selectionLayerSubtract:
            guard let layer = selectedLayer else { return .noOp }
            let operation: CoreSelectionLayerOperation = switch command {
            case .selectionLayerAdd: .add
            case .selectionLayerSubtract: .subtract
            default: .replace
            }
            observePaintMutation(
                application.coreHost.selectionFromLayer(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision,
                    layerID: layer.id,
                    operation: operation
                ),
                requestsSnapshot: true,
                viewID: viewID(for: context)
            )
        case .selectionOptions:
            _ = presentToolOptions(for: .selection)
        case .zoomIn:
            routeView(
                .zoomAt(
                    factor: 1.25,
                    deviceX: drawableSize.width / 2,
                    deviceY: drawableSize.height / 2
                ),
                expectation: expectation,
                context: context
            )
        case .zoomOut:
            routeView(
                .zoomAt(
                    factor: 0.8,
                    deviceX: drawableSize.width / 2,
                    deviceY: drawableSize.height / 2
                ),
                expectation: expectation,
                context: context
            )
        case .fit:
            routeView(
                .fit(viewportWidth: drawableSize.width, viewportHeight: drawableSize.height),
                expectation: expectation,
                context: context
            )
        case .oneToOne:
            routeView(
                .oneToOne(viewportWidth: drawableSize.width, viewportHeight: drawableSize.height),
                expectation: expectation,
                context: context
            )
        case .flipHorizontal:
            routeView(.flipHorizontal, expectation: expectation, context: context) {
                self.presentationState.flipHorizontal.toggle()
            }
        case .flipVertical:
            routeView(.flipVertical, expectation: expectation, context: context) {
                self.presentationState.flipVertical.toggle()
            }
        case .ruler:
            let value = !presentationState.rulerVisible
            routeView(.setRulerVisible(value), expectation: expectation, context: context) {
                self.presentationState.rulerVisible = value
            }
        case .guides:
            let value = !presentationState.guidesVisible
            routeView(.setGuidesVisible(value), expectation: expectation, context: context) {
                self.presentationState.guidesVisible = value
            }
        case .grid:
            let value = !presentationState.gridVisible
            routeView(.setGridVisible(value), expectation: expectation, context: context) {
                self.presentationState.gridVisible = value
            }
        case .snapGuides:
            let value = !presentationState.guideSnapEnabled
            routeView(.setGuideSnapEnabled(value), expectation: expectation, context: context) {
                self.presentationState.guideSnapEnabled = value
            }
        case .snapGrid:
            let value = !presentationState.gridSnapEnabled
            routeView(.setGridSnapEnabled(value), expectation: expectation, context: context) {
                self.presentationState.gridSnapEnabled = value
            }
        case .transparent:
            let value = !presentationState.transparentVisible
            routeView(.setTransparentVisible(value), expectation: expectation, context: context) {
                self.presentationState.transparentVisible = value
            }
        case .zoomPercent:
            pendingCommandInput = .zoomPercent(currentZoom * 100)
            return .presentedInput
        case .boxZoom:
            pendingCommandInput = .boxZoom(
                x: 0,
                y: 0,
                width: Int32(clamping: projection.documentWidth),
                height: Int32(clamping: projection.documentHeight)
            )
            return .presentedInput
        case .guideVertical, .guideHorizontal:
            let axis: CoreGuideAxis = command == .guideVertical ? .vertical : .horizontal
            let dimension = axis == .vertical ? projection.documentWidth : projection.documentHeight
            pendingCommandInput = .addGuide(
                axis: axis,
                position: Int32(clamping: dimension / 2)
            )
            return .presentedInput
        case .guideMove:
            guard let pair = guidePositions.sorted(by: { $0.key < $1.key }).first else {
                return .noOp
            }
            pendingCommandInput = .moveGuide(id: pair.key, position: pair.value)
            return .presentedInput
        case .gridSettings:
            pendingCommandInput = .grid(CoreGridDefinition(
                originX: 0,
                originY: 0,
                spacingX: 16,
                spacingY: 16,
                subdivisions: 1
            ))
            return .presentedInput
        case .guideDeleteAll:
            routeDocument(.deleteAllGuides, context: context) {
                self.guidePositions.removeAll(keepingCapacity: false)
            }
        case .viewNew:
            createLogicalView()
        case .viewClose:
            closeActiveView()
        case .tabNext:
            cycleTab(1)
        case .tabPrevious:
            cycleTab(-1)
        case .editorSplitRight:
            createLogicalView(split: .horizontal)
        case .editorSplitDown:
            createLogicalView(split: .vertical)
        case .editorNewViewOtherGroup:
            createLogicalView(inOtherGroup: true)
        case .editorMoveOtherGroup:
            guard var graph = editorGraph, let active = graph.activeView,
                  let destination = graph.groups.first(where: {
                      $0.id != graph.activeGroupID
                  })?.id,
                  graph.move(viewID: active.id, to: destination)
            else { return .noOp }
            editorGraph = graph
        case .editorGroupClose:
            guard var graph = editorGraph,
                  graph.closeGroup(graph.activeGroupID) != nil
            else { return .noOp }
            editorGraph = graph
        case .editorGroupNext:
            guard var graph = editorGraph,
                  let next = graph.groups.first(where: { $0.id != graph.activeGroupID }),
                  graph.activate(groupID: next.id)
            else { return .noOp }
            editorGraph = graph
            if let active = graph.activeView,
               let session = sessionProjections[active.session]
            {
                self.projection = session
                refreshTree()
            }
        case .tabMoveLeft:
            guard var graph = editorGraph, let active = graph.activeView,
                  graph.reorderTab(viewID: active.id, delta: -1)
            else { return .noOp }
            editorGraph = graph
        case .tabMoveRight:
            guard var graph = editorGraph, let active = graph.activeView,
                  graph.reorderTab(viewID: active.id, delta: 1)
            else { return .noOp }
            editorGraph = graph
        case .viewMoveNextWindow:
            application.transferActiveView(from: id, copy: false, newWindow: false)
        case .viewDuplicateNextWindow:
            application.transferActiveView(from: id, copy: true, newWindow: false)
        case .viewMoveNewWindow:
            application.transferActiveView(from: id, copy: false, newWindow: true)
        case .viewDuplicateNewWindow:
            application.transferActiveView(from: id, copy: true, newWindow: true)
        case .workspaceNewWindow:
            application.openNewWorkspaceWindow()
        case .windowLayerPalette:
            return reduceChromeCommand(.toggleInspectorSection(.layerPlane))
        case .windowToolPalette:
            let result = reduceChromeCommand(.toggleToolSurface)
            if adaptiveChrome.toolPresentation == .hidden {
                _ = closeToolOptions()
            }
            return result
        case .windowToolOptions:
            guard let tool = paintProjections[context.session]?.editor.activeTool else {
                return .noOp
            }
            toggleToolOptions(for: tool)
            return .started
        case .windowColorPane:
            return reduceChromeCommand(.toggleInspectorSection(.color))
        case .windowLocator:
            if inspectorSectionIsActive(.color) {
                locatorVisible.toggle()
            } else {
                let wasVisible = locatorVisible
                let chromeResult = reduceChrome(.showInspectorSection(.color))
                if chromeResult == .stale { return .stale }
                locatorVisible = true
                if wasVisible, chromeResult == .noOp { return .noOp }
            }
        case .locatorPin:
            toggleLocatorPanePin()
        case .locatorFixed:
            locatorFixed.toggle()
        case .locatorAutoscroll:
            locatorAutoscroll.toggle()
        case .colorPin:
            toggleColorPanePin()
        case .workspaceMirror:
            return reduceChromeCommand(.mirrorEdges)
        case .workspaceReset:
            restoreWorkspace(.defaultColoring)
        case .workspaceSave:
            saveWorkspace(named: workspaceLayout.customName)
        case .workspaceSaveAs:
            requestWorkspaceName()
            return .presentedInput
        case .workspaceRestore:
            restoreSavedWorkspace()
        case .workspacePresetColoring:
            applyPreset(.coloring)
        case .workspacePresetLineCleanup:
            applyPreset(.lineCleanup)
        case .workspacePresetReference:
            applyPreset(.referenceCheck)
        case .workspacePresetBatch:
            applyPreset(.batch)
        case .workspacePresetFocus:
            applyPreset(.focus)
        case .cellPaperSettings, .cellImageSize:
            pendingCellEditor = CellEditorDraft(
                kind: .resize(allowsResample: false),
                session: projection
            )
            pendingCellEditorContext = context
            return .presentedInput
        case .cellResolution:
            var draft = CellEditorDraft(
                kind: .resize(allowsResample: true),
                session: projection
            )
            draft.resample = true
            pendingCellEditor = draft
            pendingCellEditorContext = context
            return .presentedInput
        case .cellFrameHundred, .cellFrameReference, .cellFrameDrawing,
             .cellFrameSafe, .cellMargins:
            pendingCellEditor = CellEditorDraft(kind: .paperFrames, session: projection)
            pendingCellEditorContext = context
            return .presentedInput
        case .cellMirrorVertical:
            routeCellEdit(.mirror(.vertical), context: context)
        case .cellRotateLeft:
            routeCellEdit(.rotate(.left), context: context)
        case .cellRotateRight:
            routeCellEdit(.rotate(.right), context: context)
        case .cellFitCaptureFrame:
            routeCellEdit(.fitPaperToFrames, context: context)
        case .planeMainLine:
            guard let layer = selectedLayer,
                  let plane = layer.planes.first(where: { $0.kind == .mainLine })
            else { return .noOp }
            selectNode(layerID: layer.id, planeID: plane.id)
        case .planeColor:
            guard let layer = selectedLayer,
                  let plane = layer.planes.first(where: { $0.kind == .color })
            else { return .noOp }
            selectNode(layerID: layer.id, planeID: plane.id)
        case .toolPencil:
            updateEditor(.activeTool(.pencil), viewID: viewID(for: context))
        case .toolBrush:
            updateEditor(.activeTool(.brush), viewID: viewID(for: context))
        case .toolEraser:
            updateEditor(.activeTool(.eraser), viewID: viewID(for: context))
        case .toolFill:
            updateEditor(.activeTool(.fill), viewID: viewID(for: context))
        case .toolEyedropper:
            updateEditor(.activeTool(.eyedropper), viewID: viewID(for: context))
        case .toolFillOptions:
            _ = presentToolOptions(for: .fill)
        case .toolClosedFill, .toolFillExtension:
            updateFillOperationAndSelectTool(
                command == .toolClosedFill ? .closedRegion : .extensionRegion,
                context: context
            )
        case .toolColorReplaceTarget:
            colorInspectorVisible = true
            _ = reduceChrome(.showInspectorSection(.color))
            updateEditor(.activeTool(.colorReplace), viewID: viewID(for: context))
        case .toolColorReplacePen, .toolColorReplaceRectangle,
             .toolColorReplacePolyline, .toolColorReplaceLasso, .toolColorReplaceAll:
            colorReplaceRegionTool = switch command {
            case .toolColorReplacePen: .pen
            case .toolColorReplaceRectangle: .rectangle
            case .toolColorReplacePolyline: .polyline
            case .toolColorReplaceLasso: .lasso
            default: .all
            }
            updateEditor(.activeTool(.colorReplace), viewID: viewID(for: context))
        case .colorChoose, .colorEditor:
            colorInspectorVisible = true
            _ = reduceChrome(.showInspectorSection(.color))
        case .colorCheckOff, .colorCheckLegacy, .colorCheckNative:
            let mode: CoreColorCheckMode = switch command {
            case .colorCheckLegacy: .legacyWhite
            case .colorCheckNative: .nativeAlpha
            default: .off
            }
            observePaintMutation(
                application.coreHost.setColorCheck(
                    target: context.view,
                    expectedViewRevision: context.viewRevision,
                    mode: mode
                ),
                requestsSnapshot: true,
                viewID: viewID(for: context)
            )
        case .colorSourceTopmost:
            eyedropperSource = .topmostNontransparent
        case .colorSourceSelected:
            eyedropperSource = .selectedPlane
        case .colorSourceComposite:
            eyedropperSource = .composite
        case .colorSourceLightTable:
            eyedropperSource = .lightTableTopmost
        case .paletteRegister:
            mutatePalette(context: context, operation: .registerCurrent)
        case .paletteDelete:
            mutatePalette(context: context, operation: .deleteCurrent)
        case .paletteClear:
            mutatePalette(context: context, operation: .clear)
        case .paletteSave:
            presentPaintDataPanel(kind: .palette, saving: true, context: context)
            return .presentedInput
        case .paletteLoad:
            presentPaintDataPanel(kind: .palette, saving: false, context: context)
            return .presentedInput
        case .paletteNextGroup:
            let count = paintProjections[context.session]?.palette.colors.count ?? 0
            let pages = max(1, (count + 15) / 16)
            chartPage = UInt32((Int(chartPage) + 1) % pages)
        case .chartGenerate:
            observePaintMutation(
                application.coreHost.createColorChartPreview(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision,
                    maximumColors: 256,
                    quantizationBits: 5
                ),
                requestsSnapshot: false
            )
            return .presentedInput
        case .chartSearch:
            colorInspectorVisible = true
            _ = reduceChrome(.showInspectorSection(.color))
        case .chartNext:
            advanceChartCursor(session: context.session, matchingSearch: true)
        case .chartLock:
            replaceChart(
                context: context,
                entries: paintProjections[context.session]?.chart.entries ?? [],
                locked: !(paintProjections[context.session]?.chart.isLocked ?? false)
            )
        case .chartCopy:
            if let selected = selectedChartEntry(in: context.session) {
                chartClipboard = [selected]
            }
        case .chartPaste:
            pasteChart(context: context)
        case .chartCut:
            cutChart(context: context)
        case .chartRename:
            presentChartRename(context: context)
            return .presentedInput
        case .chartSave:
            presentPaintDataPanel(kind: .chart, saving: true, context: context)
            return .presentedInput
        case .chartLoad:
            presentPaintDataPanel(kind: .chart, saving: false, context: context)
            return .presentedInput
        case .chartNextPage:
            let count = paintProjections[context.session]?.chart.entries.count ?? 0
            let pages = max(1, (count + 31) / 32)
            palettePage = UInt32((Int(palettePage) + 1) % pages)
        case .selectionOutputColorGuard:
            observePaintMutation(
                application.coreHost.selectOutputColorGuard(
                    target: context.session,
                    expectedDocumentRevision: context.documentRevision,
                    operation: .replace
                ),
                requestsSnapshot: true,
                viewID: viewID(for: context)
            )
        case .layerNew:
            pendingTreeEditor = TreeEditorDraft(
                kind: .createLayer,
                defaultName: application.languageController.text("m5.default.layerName")
            )
            pendingTreeEditorContext = context
            return .presentedInput
        case .planeNew:
            guard let layer = selectedLayer else { return .noOp }
            pendingTreeEditor = TreeEditorDraft(
                kind: .createPlane(parentLayerID: layer.id),
                defaultName: application.languageController.text("m5.default.planeName")
            )
            pendingTreeEditorContext = context
            return .presentedInput
        case .layerProperties, .layerOpacity:
            guard let layer = selectedLayer else { return .noOp }
            pendingTreeEditor = TreeEditorDraft(kind: .layerProperties(layer))
            pendingTreeEditorContext = context
            return .presentedInput
        case .planeProperties, .planeOpacity:
            guard let plane = selectedPlane else { return .noOp }
            pendingTreeEditor = TreeEditorDraft(kind: .planeProperties(plane))
            pendingTreeEditorContext = context
            return .presentedInput
        case .layerConvert:
            guard let layer = selectedLayer else { return .noOp }
            pendingTreeEditor = TreeEditorDraft(kind: .convertLayer(id: layer.id))
            pendingTreeEditorContext = context
            return .presentedInput
        case .planeConvert:
            guard let plane = selectedPlane else { return .noOp }
            pendingTreeEditor = TreeEditorDraft(
                kind: .convertPlane(id: plane.id, parentLayerID: plane.parentID)
            )
            pendingTreeEditorContext = context
            return .presentedInput
        case .layerDuplicate:
            guard let layer = selectedLayer else { return .noOp }
            routeTreeEdit(.duplicateLayer(id: layer.id), context: context)
        case .layerDelete:
            guard let layer = selectedLayer else { return .noOp }
            routeTreeEdit(.deleteLayer(id: layer.id), context: context)
        case .layerMoveTop:
            guard let layer = selectedLayer else { return .noOp }
            routeTreeEdit(.reorderLayer(id: layer.id, destinationIndex: 0), context: context)
        case .layerMoveUp:
            guard let layer = selectedLayer, layer.index > 0 else { return .noOp }
            routeTreeEdit(
                .reorderLayer(id: layer.id, destinationIndex: layer.index - 1),
                context: context
            )
        case .layerMoveDown:
            guard let layer = selectedLayer else { return .noOp }
            routeTreeEdit(
                .reorderLayer(id: layer.id, destinationIndex: layer.index + 1),
                context: context
            )
        case .layerToggleVisible:
            guard let layer = selectedLayer else { return .noOp }
            routeTreeEdit(layerProperties(layer, visible: !layer.isVisible), context: context)
        case .layerToggleEditable:
            guard let layer = selectedLayer else { return .noOp }
            routeTreeEdit(layerProperties(layer, editable: !layer.isEditable), context: context)
        case .layerMerge:
            guard let layer = selectedLayer else { return .noOp }
            routeTreeEdit(.mergeLayer(id: layer.id), context: context)
        case .layerDeleteHidden:
            routeTreeEdit(.deleteHiddenLayers, context: context)
        case .planeDuplicate:
            guard let plane = selectedPlane else { return .noOp }
            routeTreeEdit(
                .duplicatePlane(id: plane.id, parentLayerID: plane.parentID),
                context: context
            )
        case .planeDelete:
            guard let plane = selectedPlane else { return .noOp }
            routeTreeEdit(.deletePlane(id: plane.id, parentLayerID: plane.parentID), context: context)
        case .planeMoveUp:
            guard let plane = selectedPlane, plane.index > 0 else { return .noOp }
            routeTreeEdit(
                .reorderPlane(
                    id: plane.id,
                    parentLayerID: plane.parentID,
                    destinationIndex: plane.index - 1
                ),
                context: context
            )
        case .planeMoveDown:
            guard let plane = selectedPlane else { return .noOp }
            routeTreeEdit(
                .reorderPlane(
                    id: plane.id,
                    parentLayerID: plane.parentID,
                    destinationIndex: plane.index + 1
                ),
                context: context
            )
        case .planeToggleVisible:
            guard let plane = selectedPlane else { return .noOp }
            routeTreeEdit(planeProperties(plane, visible: !plane.isVisible), context: context)
        case .planeToggleEditable:
            guard let plane = selectedPlane else { return .noOp }
            routeTreeEdit(planeProperties(plane, editable: !plane.isEditable), context: context)
        case .planeMerge:
            guard let plane = selectedPlane else { return .noOp }
            routeTreeEdit(.mergePlane(id: plane.id, parentLayerID: plane.parentID), context: context)
        case .fileExportInstructionRaster:
            exportInstructionRaster()
            return .presentedInput
        case .viewVectorAntialias:
            let value = !presentationState.vectorAntialias
            routeView(.setVectorAntialias(value), expectation: expectation, context: context) {
                self.presentationState.vectorAntialias = value
            }
        case .viewVectorCenterline, .viewVectorCenterlineOnly:
            let requested: UInt32 = command == .viewVectorCenterline ? 1 : 2
            let value = presentationState.vectorCenterlineMode == requested ? 0 : requested
            routeView(.setVectorCenterlineMode(value), expectation: expectation, context: context) {
                self.presentationState.vectorCenterlineMode = value
            }
        case .viewVectorEndpoints:
            let value = !presentationState.vectorEndpointsVisible
            routeView(.setVectorEndpointsVisible(value), expectation: expectation, context: context) {
                self.presentationState.vectorEndpointsVisible = value
            }
        case .effectAlphaView:
            let value = !presentationState.alphaVisible
            routeView(.setAlphaVisible(value), expectation: expectation, context: context) {
                self.presentationState.alphaVisible = value
            }
        case .filterLast:
            guard let plane = selectedPlane else { return .noOp }
            routeM8(.applyLastFilter(planeID: plane.id), context: context)
        case .filterInvert, .filterBlurWeak, .filterSharpenWeak,
             .filterSharpenStrong, .filterBlurStrong, .filterGaussian,
             .filterAutoContrast, .filterBrightness, .filterToneCurve,
             .filterLevels, .filterHSV, .filterColorBalance, .filterUnsharp:
            guard let plane = selectedPlane,
                  let kind = filterKind(for: command)
            else { return .noOp }
            presentFilter(kind: kind, planeID: plane.id, context: context)
            return .presentedInput
        case .effectGradient, .effectAirbrush, .effectBoundaryAirbrush,
             .effectBlur, .effectStamp, .effectDust, .effectAlphaGradient:
            guard let plane = selectedPlane else { return .noOp }
            pendingM8Editor = .effect(M8EffectDraft(
                context: context,
                planeID: plane.id,
                command: command
            ))
            return .presentedInput
        case .adjustmentCreate:
            guard let plane = selectedPlane else { return .noOp }
            presentFilter(
                kind: .brightnessContrast,
                planeID: plane.id,
                context: context,
                createsAdjustment: true
            )
            return .presentedInput
        case .adjustmentEdit:
            guard let layer = selectedLayer, let plane = firstEditableRasterPlane() else {
                return .noOp
            }
            presentFilter(
                kind: .brightnessContrast,
                planeID: plane.id,
                context: context,
                adjustmentLayerID: layer.id
            )
            return .presentedInput
        case .adjustmentToggle:
            guard let layer = selectedLayer else { return .noOp }
            routeTreeEdit(layerProperties(layer, visible: !layer.isVisible), context: context)
        case .adjustmentMoveTop:
            guard let layer = selectedLayer else { return .noOp }
            routeTreeEdit(.reorderLayer(id: layer.id, destinationIndex: 0), context: context)
        case .adjustmentPrevious, .adjustmentNext:
            selectAdjacentAdjustment(previous: command == .adjustmentPrevious, context: context)
        case .geometryOptions:
            pendingM8Editor = .geometry(m8GeometryOptions)
            return .presentedInput
        case .vectorLine, .vectorCurve, .vectorRectangle, .vectorEllipse,
             .vectorPolyline, .vectorPolygon:
            activeM8CanvasTool = .geometry(geometryPrimitive(for: command))
        case .vectorEraser:
            activeM8CanvasTool = .vectorEraser(vectorEraseMode)
        case .vectorErasePartial, .vectorEraseIntersection, .vectorEraseWhole:
            vectorEraseMode = switch command {
            case .vectorEraseIntersection: .toIntersection
            case .vectorEraseWhole: .wholePath
            default: .partial
            }
            activeM8CanvasTool = .vectorEraser(vectorEraseMode)
        case .vectorConnect:
            guard let plane = selectedPlane else { return .noOp }
            routeM8(.vector(.connect(planeID: plane.id, maximumGap: 8)), context: context)
        case .vectorWidth:
            let ids = vectorSelection?.ranges.map(\.pathID) ?? []
            guard !ids.isEmpty else { return .noOp }
            routeM8(.vector(.correctWidth(pathIDs: ids, mode: .scale, parameter: 1.1)), context: context)
        case .vectorSelectCut, .vectorSelectTouch, .vectorSelectContained,
             .vectorSelectLine, .vectorSelectWholeLine, .vectorSelectIntersection,
             .vectorSelectFillBoundary, .vectorSelectFill:
            vectorSelectionMode = vectorSelectionMode(for: command)
            routeM8(
                .vector(.select(
                    mode: vectorSelectionMode,
                    bounds: CoreFrameRect(
                        x: 0,
                        y: 0,
                        width: Int32(clamping: projection.documentWidth),
                        height: Int32(clamping: projection.documentHeight)
                    )
                )),
                context: context
            )
        case .vectorRasterize:
            guard let layer = selectedLayer else { return .noOp }
            routeM8(
                .vector(.rasterizeToLayer(
                    layerID: layer.id,
                    scale: 1,
                    antialias: presentationState.vectorAntialias,
                    name: "Rasterized Vector"
                )),
                context: context
            )
        case .vectorVectorize:
            guard let plane = selectedPlane,
                  let target = cellTree?.layers.first(where: { $0.kind == .vectorColoring })
            else { return .noOp }
            routeM8(
                .vector(.vectorize(
                    sourcePlaneID: plane.id,
                    targetLayerID: target.id,
                    alphaThreshold: 32_768
                )),
                context: context
            )
        case .annotationAddText, .annotationEditText:
            guard let layer = selectedLayer else { return .noOp }
            if command == .annotationEditText,
               selectedAnnotationID.flatMap({ annotationSeeds[$0] }) == nil
            {
                return .noOp
            }
            presentAnnotation(
                layerID: layer.id,
                objectID: command == .annotationEditText ? selectedAnnotationID : nil,
                context: context
            )
            return .presentedInput
        case .annotationDrawInstruction:
            activeM8CanvasTool = .annotationStroke
        case .annotationSelectPrevious, .annotationSelectNext:
            selectAdjacentAnnotation(previous: command == .annotationSelectPrevious)
        case .annotationMoveLeft, .annotationMoveRight:
            guard let objectID = selectedAnnotationID else { return .noOp }
            routeM8(
                .annotation([.move(
                    id: objectID,
                    deltaX: command == .annotationMoveLeft ? -1 : 1,
                    deltaY: 0
                )]),
                context: context
            )
        case .annotationDelete:
            guard let objectID = selectedAnnotationID else { return .noOp }
            routeM8(.annotation([.delete(id: objectID)]), context: context)
        case .cellShootingFrameProperties:
            presentShootingFrame(context: context)
            return .presentedInput
        case .cellShootingFrameEditHandles:
            activeM8CanvasTool = .shootingFrameHandles
        case .cellShootingFrameDelete:
            guard let frameID = m8State?.shootingFrame?.id else { return .noOp }
            routeM8(.shootingFrameDelete(id: frameID), context: context)
        case .cellVanishingPointProperties:
            guard let layer = selectedLayer?.kind == .vanishingPoint
                ? selectedLayer
                : cellTree?.layers.first(where: {
                    $0.kind == .vanishingPoint && $0.isEditable
                })
            else { return .noOp }
            presentVanishingPoint(layerID: layer.id, context: context)
            return .presentedInput
        case .cellVanishingPointEditHandles:
            activeM8CanvasTool = .vanishingPointHandles
        case .cellVanishingPointDeleteAll:
            routeM8(.vanishingPointDeleteAll, context: context)
        default:
            return .invalid
        }
        return .started
    }

    func refreshM8() {
        guard let context = commandContext else {
            m8State = nil
            return
        }
        let generation = lifecycleGeneration
        observe(
            application.coreHost.inspectM8(
                target: context.session,
                expectedDocumentRevision: context.documentRevision
            ),
            generation: generation
        ) { [weak self] outcome in
            guard let self else { return }
            switch outcome {
            case let .m8State(state):
                guard state.session.target == context.session else { return }
                self.m8State = state
                self.updateSessionProjection(state.session)
            case .failed(.staleTarget):
                self.lastCommandResult = .stale
            default:
                break
            }
        }
    }

    func updateFilterPreview(_ draft: M8FilterDraft) {
        guard case let .filter(current) = pendingM8Editor,
              current.id == draft.id,
              current.context == draft.context,
              matches(draft.context),
              draft.request.isValid,
              !draft.createsAdjustment,
              draft.adjustmentLayerID == nil
        else {
            lastCommandResult = .stale
            return
        }
        pendingM8Editor = .filter(draft)
        pendingFilterPreview = draft
        filterPreviewDelay?.cancel()
        filterPreviewDelay = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(120))
            guard !Task.isCancelled, let self,
                  self.pendingFilterPreview?.id == draft.id
            else { return }
            self.pendingFilterPreview = nil
            self.launchFilterPreview(draft, begin: false)
        }
    }

    func applyFilterEditor(_ draft: M8FilterDraft) {
        guard case let .filter(current) = pendingM8Editor,
              current.id == draft.id,
              current.context == draft.context,
              matches(draft.context), draft.request.isValid
        else {
            lastCommandResult = .stale
            return
        }
        filterPreviewDelay?.cancel()
        filterPreviewDelay = nil
        pendingFilterPreview = nil
        pendingM8Editor = nil
        if draft.createsAdjustment {
            routeM8(.createAdjustment(draft.request, name: "Adjustment"), context: draft.context)
        } else if let layerID = draft.adjustmentLayerID {
            routeM8(.updateAdjustment(layerID: layerID, filter: draft.request), context: draft.context)
        } else {
            pendingFilterApply = draft
            if filterPreviewRequestID == nil {
                launchFilterPreview(draft, begin: !filterPreviewStarted)
            }
        }
    }

    func cancelM8Editor() {
        guard let draft = pendingM8Editor else { return }
        pendingM8Editor = nil
        filterPreviewDelay?.cancel()
        filterPreviewDelay = nil
        pendingFilterPreview = nil
        pendingFilterApply = nil
        switch draft {
        case let .filter(filter)
            where !filter.createsAdjustment && filter.adjustmentLayerID == nil:
            if let requestID = filterPreviewRequestID {
                _ = application.coreHost.cancel(request: requestID)
            }
            routeM8(.cancelFilterPreview, context: filter.context)
            filterPreviewStarted = false
            filterPreviewRequestID = nil
        case let .shootingFrame(frame):
            routeM8(.shootingFramePreviewCancel, context: frame.context)
        case let .vanishingPoint(point):
            routeM8(.vanishingPointPreviewCancel, context: point.context)
        default:
            lastCommandResult = .cancelled
        }
    }

    func applyEffectEditor(_ draft: M8EffectDraft) {
        guard case let .effect(current) = pendingM8Editor,
              current.id == draft.id, current.context == draft.context,
              matches(draft.context), draft.primary.isFinite, draft.secondary.isFinite
        else {
            lastCommandResult = .stale
            return
        }
        guard let effect = effectCommand(from: draft) else {
            lastCommandResult = .invalid
            return
        }
        pendingM8Editor = nil
        routeM8(.effect(effect), context: draft.context)
    }

    func applyGeometryOptions(_ draft: M8GeometryOptionsDraft) {
        guard case let .geometry(current) = pendingM8Editor, current.id == draft.id,
              draft.outlineWidth.isFinite, draft.outlineWidth > 0,
              (3 ... 256).contains(draft.polygonSides),
              draft.options.outline || draft.options.fill
        else {
            lastCommandResult = .invalid
            return
        }
        m8GeometryOptions = draft
        pendingM8Editor = nil
        lastCommandResult = .started
    }

    func applyAnnotationEditor(_ draft: M8AnnotationDraft) {
        guard case let .annotation(current) = pendingM8Editor,
              current.id == draft.id, current.context == draft.context,
              matches(draft.context), !draft.text.utf8.isEmpty,
              draft.width >= 0, draft.height >= 0,
              draft.fontSize.isFinite, draft.fontSize > 0
        else {
            lastCommandResult = .invalid
            return
        }
        let object = CoreAnnotationObject(
            kind: .text,
            layerID: draft.layerID,
            output: draft.instructionOnly ? .instruction : .normal,
            bounds: CoreFrameRect(
                x: draft.x,
                y: draft.y,
                width: draft.width,
                height: draft.height
            ),
            fontFamily: draft.fontFamily,
            fontSizeMilli: UInt32(clamping: Int64((draft.fontSize * 1_000).rounded())),
            color: .rgba8(red: 0, green: 0, blue: 0),
            text: draft.text
        )
        pendingM8Editor = nil
        let edit: CoreAnnotationEdit = draft.objectID.map {
            .update(id: $0, object: object)
        } ?? .create(object)
        routeM8(.annotation([edit]), context: draft.context)
    }

    func previewShootingFrame(_ draft: M8ShootingFrameDraft) {
        guard case let .shootingFrame(current) = pendingM8Editor,
              current.id == draft.id, matches(draft.context), draft.frame.isValid
        else { return }
        pendingM8Editor = .shootingFrame(draft)
        routeM8(.shootingFramePreviewUpdate(draft.frame), context: draft.context)
    }

    func applyShootingFrame(_ draft: M8ShootingFrameDraft) {
        guard case let .shootingFrame(current) = pendingM8Editor,
              current.id == draft.id, matches(draft.context), draft.frame.isValid
        else {
            lastCommandResult = .stale
            return
        }
        pendingM8Editor = nil
        routeM8(.shootingFramePreviewApply, context: draft.context)
    }

    func previewVanishingPoint(_ draft: M8VanishingPointDraft) {
        guard case let .vanishingPoint(current) = pendingM8Editor,
              current.id == draft.id, matches(draft.context), draft.point.isValid
        else { return }
        pendingM8Editor = .vanishingPoint(draft)
        routeM8(.vanishingPointPreviewUpdate(draft.point), context: draft.context)
    }

    func applyVanishingPoint(_ draft: M8VanishingPointDraft) {
        guard case let .vanishingPoint(current) = pendingM8Editor,
              current.id == draft.id, matches(draft.context), draft.point.isValid
        else {
            lastCommandResult = .stale
            return
        }
        pendingM8Editor = nil
        routeM8(.vanishingPointPreviewApply, context: draft.context)
    }

    func beginM8CanvasGesture(_ sample: CorePointerSample, viewID: WorkspaceViewID) -> Bool {
        guard let tool = activeM8CanvasTool,
              let context = commandContext,
              context.view == viewRecord(viewID)?.coreTarget,
              sample.isValid
        else { return false }
        m8CanvasGesture = M8CanvasGesture(
            context: context,
            viewID: viewID,
            tool: tool,
            start: sample,
            samples: [sample]
        )
        return true
    }

    func appendM8CanvasGesture(_ sample: CorePointerSample, viewID: WorkspaceViewID) {
        guard var gesture = m8CanvasGesture, gesture.viewID == viewID,
              gesture.samples.count < 65_536, sample.isValid
        else { return }
        gesture.samples.append(sample)
        m8CanvasGesture = gesture
    }

    func endM8CanvasGesture(_ sample: CorePointerSample?, viewID: WorkspaceViewID) {
        guard var gesture = m8CanvasGesture, gesture.viewID == viewID else { return }
        m8CanvasGesture = nil
        if let sample, sample.isValid { gesture.samples.append(sample) }
        guard matches(gesture.context), !gesture.samples.isEmpty else {
            lastCommandResult = .stale
            return
        }
        let generation = lifecycleGeneration
        let task = application.coreHost.resolveDocumentPoints(
            target: gesture.context.view,
            expectedDocumentRevision: gesture.context.documentRevision,
            expectedViewRevision: gesture.context.viewRevision,
            samples: gesture.samples
        )
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            guard let self, self.lifecycleGeneration == generation, self.phase == .ready else {
                return
            }
            switch outcome {
            case let .documentPoints(points)
                where points.count == gesture.samples.count && self.matches(gesture.context):
                self.applyM8CanvasGesture(gesture, points: points)
            case .failed(.staleTarget), .failed(.invalidTarget):
                self.lastCommandResult = .stale
            case let .failed(failure):
                self.lastCommandResult = .failed(failure)
            default:
                self.lastCommandResult = .invalid
            }
        }
    }

    private func applyM8CanvasGesture(
        _ gesture: M8CanvasGesture,
        points: [CoreDocumentPoint]
    ) {
        guard let firstPoint = points.first, let lastPoint = points.last else {
            lastCommandResult = .invalid
            return
        }
        let first = CoreGeometryPoint(x: firstPoint.x, y: firstPoint.y)
        let last = CoreGeometryPoint(x: lastPoint.x, y: lastPoint.y)
        switch gesture.tool {
        case let .geometry(primitive):
            guard let plane = selectedPlane(for: gesture.context) else { return }
            let request = CoreGeometryRequest(
                primitive: primitive,
                planeID: plane.id,
                baseRevision: 0,
                outlineWidth: Float(m8GeometryOptions.outlineWidth),
                polygonSides: m8GeometryOptions.polygonSides,
                options: m8GeometryOptions.options,
                points: [first, last]
            )
            routeM8(.applyGeometry(request), context: gesture.context)
        case let .vectorEraser(mode):
            guard let plane = selectedPlane(for: gesture.context) else { return }
            routeM8(
                .vector(.erase(
                    planeID: plane.id,
                    x: last.x,
                    y: last.y,
                    radius: 8,
                    mode: mode
                )),
                context: gesture.context
            )
        case .annotationStroke:
            guard let layer = selectedLayer(for: gesture.context) else { return }
            let annotationPoints = points.map {
                CoreAnnotationPoint(
                    xMilli: Int32(clamping: Int64(($0.x * 1_000).rounded())),
                    yMilli: Int32(clamping: Int64(($0.y * 1_000).rounded()))
                )
            }
            let bounds = annotationBounds(annotationPoints)
            let object = CoreAnnotationObject(
                kind: .stroke,
                layerID: layer.id,
                output: .instruction,
                bounds: bounds,
                points: annotationPoints
            )
            routeM8(.annotation([.create(object)]), context: gesture.context)
        case .shootingFrameHandles:
            guard var frame = currentShootingFrame() else { return }
            frame.centerX = Double(last.x)
            frame.centerY = Double(last.y)
            routeM8(.shootingFrameUpdate(frame, preview: false), context: gesture.context)
        case .vanishingPointHandles:
            guard var point = m8State?.vanishingPoints.first else { return }
            point.xMilli = Int64((Double(last.x) * 1_000).rounded())
            point.yMilli = Int64((Double(last.y) * 1_000).rounded())
            routeM8(.vanishingPointUpdate(point, preview: false), context: gesture.context)
        }
    }

    func cancelM8CanvasGesture(viewID: WorkspaceViewID) {
        guard m8CanvasGesture?.viewID == viewID else { return }
        m8CanvasGesture = nil
        lastCommandResult = .cancelled
    }

    func cancelCommandInput() {
        pendingCommandInput = nil
        lastCommandResult = .cancelled
    }

    @discardableResult
    func submitCommandInput(_ input: WorkspaceCommandInput) -> CommandRouteResult {
        guard input.id == pendingCommandInput?.id, let context = commandContext else {
            return .stale
        }
        pendingCommandInput = nil
        let expectation = CoreCommandExpectation(
            documentRevision: context.documentRevision,
            viewRevision: context.viewRevision
        )
        switch input {
        case let .zoomPercent(percent):
            guard percent.isFinite, (1 ... 6_400).contains(percent), currentZoom > 0 else {
                return .invalid
            }
            routeView(
                .zoomAt(
                    factor: percent / 100 / currentZoom,
                    deviceX: drawableSize.width / 2,
                    deviceY: drawableSize.height / 2
                ),
                expectation: expectation,
                context: context
            )
        case let .boxZoom(x, y, width, height):
            guard width > 0, height > 0 else { return .invalid }
            routeView(
                .boxZoom(documentX: x, documentY: y, width: width, height: height),
                expectation: expectation,
                context: context
            )
        case let .addGuide(axis, position):
            routeDocument(.addGuide(axis: axis, position: position), context: context) {
                if let guideID = self.lastAffectedGuideID {
                    self.guidePositions[guideID] = position
                }
            }
        case let .moveGuide(id, position):
            routeDocument(.moveGuide(id: id, position: position), context: context) {
                self.guidePositions[id] = position
            }
        case let .grid(grid):
            guard grid.isValid else { return .invalid }
            routeDocument(.setGrid(grid), context: context)
        case let .selectionAdjust(operation, pixels):
            guard operation != .invert, (1 ... 16_384).contains(pixels) else {
                return .invalid
            }
            routeSelectionAdjust(operation, pixels: pixels, context: context)
        }
        return .started
    }

    func requestSnapshot(viewID: WorkspaceViewID? = nil) {
        guard let view = viewRecord(viewID), let route = routes[view.id] else { return }
        let generation = lifecycleGeneration
        observe(
            application.coreHost.buildSnapshot(route: route),
            generation: generation
        ) { [weak self] outcome in
            guard let self else { return }
            guard case let .snapshot(envelope) = outcome,
                  self.routes[view.id] == envelope.route
            else {
                if case let .snapshot(envelope) = outcome {
                    envelope.owner.release()
                }
                return
            }
            try? envelope.owner.withBorrowedRenderView { view in
                self.currentZoom = view.transform.zoom
                self.currentPanX = view.transform.panX
                self.currentPanY = view.transform.panY
                self.knownAnnotationIDs = view.annotations.map(\.objectID).sorted()
                self.annotationSeeds = Dictionary(uniqueKeysWithValues: view.annotations.map {
                    ($0.objectID, M8AnnotationSeed(
                        layerID: $0.layerID,
                        text: $0.text,
                        instructionOnly: $0.output == 2,
                        fontFamily: $0.fontFamily,
                        fontSize: Double($0.fontSizeMilli) / 1_000,
                        bounds: $0.bounds
                    ))
                })
                if self.selectedAnnotationID.map({ !self.knownAnnotationIDs.contains($0) }) == true {
                    self.selectedAnnotationID = self.knownAnnotationIDs.first
                }
            }
            let submission = self.application.rendererHost.submit(envelope)
            guard submission == .accepted || submission == .replacedPending else {
                return
            }
            self.observeRendererCompletion(generation: generation)
        }
    }

    func focusWindow() {
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate()
    }

    func resolveDirtyBeforeClose() async -> Bool {
        guard await resolveDirtyCutBeforeReplacement() else { return false }
        guard let projection, projection.isDirty else { return true }
        if let dirtyCloseDecisionForTesting {
            switch await dirtyCloseDecisionForTesting() {
            case .save:
                return await save(chooseDestination: documentURL == nil)
            case .discard:
                return true
            case .cancel:
                return false
            }
        }
        let alert = NSAlert()
        alert.messageText = "Do you want to save the changes?"
        alert.informativeText = documentURL?.lastPathComponent ?? "Untitled"
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Discard Changes")
        alert.addButton(withTitle: "Cancel")
        switch await alertResponse(alert) {
        case .alertFirstButtonReturn:
            return await save(chooseDestination: documentURL == nil)
        case .alertSecondButtonReturn:
            return true
        default:
            return false
        }
    }

    private func coordinated(
        _ action: CoordinatedFileAction,
        at url: URL
    ) async -> CoreRequestOutcome {
        let broker = application.fileAccessBroker
        let coreHost = application.coreHost
        return await Task.detached {
            do {
                switch action {
                case let .save(target, revision, allowClean):
                    return try broker.coordinatePreparedReplacement(url) { replacement in
                        coreHost.save(
                            target: target,
                            expectedDocumentRevision: revision,
                            pathUTF8: Array(replacement.destination.path.utf8),
                            stagingPathUTF8: Array(replacement.staging.path.utf8),
                            allowCleanSave: allowClean
                        ).wait(timeout: 120) ?? .failed(.cancelled)
                    }
                case let .open(target, revision):
                    return try broker.coordinateReading(url) { coordinatedURL in
                        coreHost.open(
                            target: target,
                            expectedDocumentRevision: revision,
                            pathUTF8: Array(coordinatedURL.path.utf8)
                        ).wait(timeout: 120) ?? .failed(.cancelled)
                    }
                case let .autosave(target, revision):
                    return try broker.coordinateReplacing(url) { coordinatedURL in
                        coreHost.autosave(
                            target: target,
                            expectedDocumentRevision: revision,
                            pathUTF8: Array(coordinatedURL.path.utf8)
                        ).wait(timeout: 120) ?? .failed(.cancelled)
                    }
                case let .recovery(target, revision):
                    return try broker.coordinateReading(url) { coordinatedURL in
                        coreHost.openRecovery(
                            target: target,
                            expectedDocumentRevision: revision,
                            pathUTF8: Array(coordinatedURL.path.utf8)
                        ).wait(timeout: 120) ?? .failed(.cancelled)
                    }
                case let .compact(target, revision, token):
                    return try broker.coordinateReplacing(url) { coordinatedURL in
                        coreHost.writeCompactedCopy(
                            target: target,
                            expectedDocumentRevision: revision,
                            pathUTF8: Array(coordinatedURL.path.utf8),
                            token: token
                        ).wait(timeout: 120) ?? .failed(.cancelled)
                    }
                }
            } catch {
                return .failed(.coreOperation(.ioError))
            }
        }.value
    }

    private func importRaster(
        url: URL,
        format: CoreCommonRasterFormat,
        target: CoreSessionTarget,
        revision: UInt64,
        documentUUID: CoreDocumentUUID
    ) async -> CoreRequestOutcome {
        let broker = application.fileAccessBroker
        let coreHost = application.coreHost
        return await Task.detached {
            do {
                return try broker.coordinateReading(url) { coordinatedURL in
                    let values = try coordinatedURL.resourceValues(forKeys: [.fileSizeKey])
                    guard let size = values.fileSize, size > 0, size <= 512 * 1_024 * 1_024 else {
                        return .failed(.invalidRequest)
                    }
                    let data = try Data(contentsOf: coordinatedURL, options: .mappedIfSafe)
                    return coreHost.importCommonRaster(
                        target: target,
                        expectedDocumentRevision: revision,
                        format: format,
                        bytes: Array(data),
                        documentUUID: documentUUID
                    ).wait(timeout: 120) ?? .failed(.cancelled)
                }
            } catch {
                return .failed(.coreOperation(.ioError))
            }
        }.value
    }

    private func handleFileOutcome(
        _ outcome: CoreRequestOutcome,
        sourceURL: URL?,
        identity: FileIdentity?,
        reservation: FileIdentityReservation?
    ) {
        switch outcome {
        case let .fileCompleted(file):
            updateSessionProjection(file.session)
            switch file.operation {
            case .open, .save:
                guard let sourceURL, identity != nil, let reservation else {
                    lastCommandResult = .stale
                    return
                }
                let publishedIdentity = FileIdentity.resolve(sourceURL)
                guard application.fileIdentityRegistry.commit(
                    reservation,
                    as: publishedIdentity
                )
                else {
                    lastCommandResult = .stale
                    return
                }
                documentURL = sourceURL
                fileIdentity = publishedIdentity
                sessionDocumentURLs[file.session.target] = sourceURL
                sessionFileIdentities[file.session.target] = publishedIdentity
                application.recordRecent(url: sourceURL, identity: publishedIdentity)
                window?.representedURL = sourceURL
                window?.title = sourceURL.lastPathComponent
                if file.operation == .open {
                    recoveryURL = nil
                } else if let recoveryURL {
                    try? application.recoveryStore.discardArtifact(at: recoveryURL)
                    self.recoveryURL = nil
                }
            case .openRecovery, .importRaster:
                application.fileIdentityRegistry.release(session: file.session.target)
                sessionDocumentURLs.removeValue(forKey: file.session.target)
                sessionFileIdentities.removeValue(forKey: file.session.target)
                documentURL = nil
                fileIdentity = nil
                window?.representedURL = nil
                window?.title = file.operation == .openRecovery ? "Recovered Document" : "Imported Image"
                if file.operation == .openRecovery {
                    recoveryURL = sourceURL
                } else {
                    recoveryURL = nil
                }
            case .autosave:
                recoveryURL = sourceURL
            case .revert, .revertPartial, .compactedCopy:
                break
            }
            lastCommandResult = .started
            requestSnapshot()
            refreshHistory(rebuildVisualization: true)
        case let .documentUpdated(updated), let .pasteCancelled(updated):
            updateSessionProjection(updated)
            lastCommandResult = .started
            requestSnapshot()
            refreshHistory(rebuildVisualization: true)
        case .noOp:
            if let reservation { application.fileIdentityRegistry.cancel(reservation) }
            lastCommandResult = .noOp
        case .failed(.staleTarget):
            if let reservation { application.fileIdentityRegistry.cancel(reservation) }
            lastCommandResult = .stale
        case let .failed(failure):
            if let reservation { application.fileIdentityRegistry.cancel(reservation) }
            lastCommandResult = .failed(failure)
        default:
            if let reservation { application.fileIdentityRegistry.cancel(reservation) }
            lastCommandResult = .invalid
        }
    }

    private func observeFileMutation(_ task: CoreTask, generation: UInt64) {
        Task {
            let outcome = await task.value()
            guard lifecycleGeneration == generation, phase == .ready else { return }
            handleFileOutcome(outcome, sourceURL: nil, identity: nil, reservation: nil)
        }
    }

    private func confirmReplacingDirtyDocument() async -> Bool {
        guard projection?.isDirty == true else { return true }
        return await resolveDirtyBeforeClose()
    }

    private func confirmRevert() async -> Bool {
        let alert = NSAlert()
        alert.messageText = "Revert to the last saved version?"
        alert.informativeText = "Unsaved changes and their history will be discarded."
        alert.addButton(withTitle: "Revert")
        alert.addButton(withTitle: "Cancel")
        return await alertResponse(alert) == .alertFirstButtonReturn
    }

    private func requestPastePlaneName() async -> String? {
        let alert = NSAlert()
        alert.messageText = "Paste into a New Raster Plane"
        alert.addButton(withTitle: "Continue")
        alert.addButton(withTitle: "Cancel")
        let field = NSTextField(string: "Pasted Raster")
        field.frame = NSRect(x: 0, y: 0, width: 280, height: 24)
        alert.accessoryView = field
        guard await alertResponse(alert) == .alertFirstButtonReturn else { return nil }
        let name = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.utf8.isEmpty || name.utf8.count > 4_096 ? nil : name
    }

    private func panelResponse(_ panel: NSSavePanel) async -> NSApplication.ModalResponse {
        if let filePanelResponseForTesting {
            return await filePanelResponseForTesting(panel)
        }
        if let window { return await panel.beginSheetModal(for: window) }
        return panel.runModal()
    }

    private func alertResponse(_ alert: NSAlert) async -> NSApplication.ModalResponse {
        if let window { return await alert.beginSheetModal(for: window) }
        return alert.runModal()
    }

    private func recoveryDirectory() throws -> URL {
        let root = application.recoveryStore.directory
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    func stop(removingFromApplication: Bool = true) async {
        guard !stopping else { return }
        stopping = true
        let floating = pendingPasteConfirmation
        let m8Editor = pendingM8Editor
        let cutTarget = cut?.target
        motionTask?.cancel()
        motionTask = nil
        lifecycleGeneration &+= 1
        if animation?.motion != nil, let projection {
            _ = await application.coreHost.performAnimation(
                target: projection.target,
                expectedDocumentRevision: projection.documentRevision,
                command: .motionStop
            ).value()
        }
        if let cutTarget {
            _ = await application.coreHost.closeCut(cutTarget).value()
        }
        cut = nil
        cutURL = nil
        animation = nil
        pendingM9Editor = nil
        if let floating {
            _ = await application.coreHost.cancelPaste(
                target: floating.context.session,
                expectedDocumentRevision: floating.context.documentRevision
            ).value()
        }
        pendingPasteConfirmation = nil
        pendingFloatingTransform = nil
        floatingTransformEditor = nil
        if let visualization = historyVisualization {
            historyVisualization = nil
            _ = await application.coreHost.releaseHistoryVisualization(visualization).value()
        }
        historyRows = []
        historyProgress = nil
        history = nil
        if let gesture = canvasPaintGesture,
           [.pencil, .brush, .eraser].contains(gesture.tool)
        {
            _ = await application.coreHost.cancelStroke(target: gesture.target).value()
        }
        canvasPaintGesture = nil
        filterPreviewDelay?.cancel()
        filterPreviewDelay = nil
        if case let .filter(filter)? = m8Editor,
           !filter.createsAdjustment, filter.adjustmentLayerID == nil
        {
            if let requestID = filterPreviewRequestID {
                _ = await application.coreHost.cancel(request: requestID).value()
            }
            _ = await application.coreHost.performM8(
                target: filter.context.session,
                expectedDocumentRevision: filter.context.documentRevision,
                command: .cancelFilterPreview
            ).value()
        }
        filterPreviewStarted = false
        filterPreviewRequestID = nil
        pendingFilterPreview = nil
        pendingFilterApply = nil
        pendingM8Editor = nil
        m8CanvasGesture = nil
        activeM8CanvasTool = nil
        if let preview = colorChartPreview {
            _ = await application.coreHost.cancelColorChartPreview(preview.id).value()
        }
        colorChartPreview = nil
        for surface in surfaces.values {
            _ = application.rendererHost.unregisterSurface(surface)
        }
        surfaces.removeAll(keepingCapacity: false)
        routes.removeAll(keepingCapacity: false)
        drawableSizes.removeAll(keepingCapacity: false)
        for target in sessionProjections.keys {
            application.fileIdentityRegistry.release(session: target)
            application.releaseSession(target, for: id)
        }
        sessionProjections.removeAll(keepingCapacity: false)
        animationProjections.removeAll(keepingCapacity: false)
        sessionDocumentURLs.removeAll(keepingCapacity: false)
        sessionFileIdentities.removeAll(keepingCapacity: false)
        treeProjections.removeAll(keepingCapacity: false)
        paintProjections.removeAll(keepingCapacity: false)
        paint = nil
        locator = nil
        projection = nil
        phase = .stopped
        if removingFromApplication {
            application.workspaceDidStop(id)
        }
    }

    private func installInitialSession(_ session: CoreSessionProjection) {
        projection = session
        sessionProjections[session.target] = session
        let view = WorkspaceViewRecord(
            id: WorkspaceViewID(rawValue: session.primaryView.id.rawValue),
            coreTarget: session.primaryView,
            session: session.target,
            viewRevision: session.viewRevision,
            title: documentTitle(for: session)
        )
        editorGraph = WorkspaceEditorGraph(initialView: view)
    }

    private func installTransferredInitial(_ transfer: WorkspaceViewTransfer) {
        projection = transfer.session
        sessionProjections[transfer.session.target] = transfer.session
        editorGraph = WorkspaceEditorGraph(initialView: transfer.view)
    }

    private func documentTitle(for session: CoreSessionProjection) -> String {
        if session.target == projection?.target, let documentURL {
            return documentURL.deletingPathExtension().lastPathComponent
        }
        return "Cell \(session.target.id.rawValue)"
    }

    private func viewRecord(_ id: WorkspaceViewID?) -> WorkspaceViewRecord? {
        guard let graph = editorGraph else { return nil }
        if let id { return graph.allViews.first { $0.id == id } }
        return graph.activeView
    }

    private func updateSessionProjection(_ session: CoreSessionProjection) {
        sessionProjections[session.target] = session
        if editorGraph?.activeView?.session == session.target {
            projection = session
        }
    }

    private func updateLogicalView(_ logicalView: CoreLogicalViewProjection) {
        updateSessionProjection(logicalView.session)
        guard var graph = editorGraph else { return }
        _ = graph.updateViewRevision(
            target: logicalView.target,
            revision: logicalView.viewRevision
        )
        editorGraph = graph
    }

    func refreshTree() {
        guard let context = layerPaneContext(),
              let targetProjection = sessionProjections[context.session]
        else {
            cellTree = nil
            return
        }
        let generation = lifecycleGeneration
        observe(
            application.coreHost.inspectTree(
                target: context.session,
                expectedDocumentRevision: targetProjection.documentRevision
            ),
            generation: generation
        ) { [weak self] outcome in
            guard let self else { return }
            if case let .tree(tree) = outcome {
                self.treeProjections[tree.session.target] = tree
                self.updateSessionProjection(tree.session)
                if let current = self.layerPaneContext(),
                   current.session == context.session,
                   current.view == context.view
                {
                    self.cellTree = tree
                }
            }
        }
    }

    func refreshPaint() {
        let visibleContext = colorPaneContext()
        var targets: [CoreSessionTarget: UInt64] = [:]
        if let visibleContext {
            targets[visibleContext.session] = visibleContext.documentRevision
        }
        if let active = commandContext {
            targets[active.session] = active.documentRevision
        }
        if targets.isEmpty {
            paint = nil
            return
        }
        let generation = lifecycleGeneration
        for (target, revision) in targets {
            observe(
                application.coreHost.inspectPaint(
                    target: target,
                    expectedDocumentRevision: revision
                ),
                generation: generation
            ) { [weak self] outcome in
                guard let self, case let .paint(projection) = outcome else { return }
                guard !self.isOlderPaintProjection(projection, for: target) else { return }
                self.paintProjections[target] = projection
                self.syncToolOptions(with: projection)
                self.updateSessionProjection(projection.editor.session)
                if self.colorPaneContext()?.session == target {
                    self.paint = projection
                    self.chartCursor = projection.chart.selectedIndex
                    self.chartPage = projection.chart.page
                }
            }
        }
    }

    func refreshHistory(rebuildVisualization: Bool = false) {
        guard let context = commandContext else {
            history = nil
            return
        }
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            guard let self else { return }
            let outcome = await application.coreHost.inspectHistory(
                target: context.session,
                expectedDocumentRevision: context.documentRevision
            ).value()
            guard lifecycleGeneration == generation, matches(context) else { return }
            switch outcome {
            case let .history(projection):
                history = projection
                if rebuildVisualization, inspectorSectionIsActive(.history) {
                    rebuildHistoryVisualization(context: context)
                }
            case .failed(.staleTarget):
                lastCommandResult = .stale
            case let .failed(failure):
                lastCommandResult = .failed(failure)
            default:
                break
            }
        }
    }

    func jumpHistory(to cursor: UInt64, context: CommandTargetContext) {
        guard matches(context) else {
            lastCommandResult = .stale
            return
        }
        routeCommand(
            application.coreHost.jumpHistory(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                cursor: cursor
            ),
            context: context
        )
    }

    func loadMoreHistoryRowsIfNeeded(after row: CoreHistoryVisualizationRow) {
        guard inspectorSectionIsActive(.history),
              row.id == historyRows.last?.id,
              let progress = historyProgress,
              UInt64(historyRows.count) < progress.rowCount
        else { return }
        loadHistoryRows(
            progress.id,
            start: UInt64(historyRows.count),
            count: 64,
            replacing: false
        )
    }

    func cancelHistoryVisualization() {
        guard let visualization = historyVisualization else { return }
        historyVisualization = nil
        historyRows = []
        historyProgress = nil
        let generation = lifecycleGeneration
        let releaseTask = application.coreHost.releaseHistoryVisualization(visualization)
        Task { @MainActor [weak self] in
            let outcome = await releaseTask.value()
            guard let self, lifecycleGeneration == generation, phase == .ready else { return }
            switch outcome {
            case .acknowledged, .noOp:
                lastCommandResult = .cancelled
            case let .failed(failure):
                lastCommandResult = .failed(failure)
            default:
                lastCommandResult = .invalid
            }
        }
    }

    private func rebuildHistoryVisualization(context: CommandTargetContext) {
        if let previous = historyVisualization {
            historyVisualization = nil
            _ = application.coreHost.releaseHistoryVisualization(previous)
        }
        historyRows = []
        historyProgress = nil
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            guard let self else { return }
            let outcome = await application.coreHost.beginHistoryVisualization(
                target: context.session,
                expectedDocumentRevision: context.documentRevision
            ).value()
            guard case let .historyVisualizationProgress(progress) = outcome else {
                if case let .failed(failure) = outcome { lastCommandResult = .failed(failure) }
                return
            }
            guard lifecycleGeneration == generation, matches(context) else {
                _ = await application.coreHost.releaseHistoryVisualization(progress.id).value()
                return
            }
            historyVisualization = progress.id
            historyProgress = progress
            advanceHistoryVisualization(progress.id, context: context)
        }
    }

    private func advanceHistoryVisualization(
        _ id: CoreHistoryVisualizationID,
        context: CommandTargetContext
    ) {
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            guard let self, historyVisualization == id else { return }
            let outcome = await application.coreHost.stepHistoryVisualization(
                id,
                maximumEvents: 32
            ).value()
            guard lifecycleGeneration == generation,
                  historyVisualization == id,
                  matches(context)
            else { return }
            guard case let .historyVisualizationProgress(progress) = outcome else {
                if case let .failed(failure) = outcome { lastCommandResult = .failed(failure) }
                return
            }
            historyProgress = progress
            if progress.isComplete {
                loadHistoryRows(
                    id,
                    start: 0,
                    count: 64,
                    replacing: true
                )
            } else {
                advanceHistoryVisualization(id, context: context)
            }
        }
    }

    private func loadHistoryRows(
        _ id: CoreHistoryVisualizationID,
        start: UInt64,
        count: UInt64,
        replacing: Bool
    ) {
        guard let progress = historyProgress, progress.id == id,
              start < progress.rowCount
        else { return }
        let end = min(progress.rowCount, start + count)
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            guard let self else { return }
            let outcome = await application.coreHost.historyVisualizationRows(
                id,
                range: start ..< end
            ).value()
            guard lifecycleGeneration == generation, historyVisualization == id else { return }
            guard case let .historyVisualizationRows(rows) = outcome else { return }
            if replacing { historyRows = rows } else { historyRows.append(contentsOf: rows) }
        }
    }

    private func paintExpectation(
        for view: WorkspaceViewRecord,
        paint: CorePaintProjection
    ) -> CorePaintExpectation? {
        guard paint.editor.session.target == view.session,
              paint.editor.activeLayerID != 0, paint.editor.activePlaneID != 0
        else { return nil }
        return CorePaintExpectation(
            documentRevision: paint.editor.session.documentRevision,
            viewRevision: view.viewRevision,
            editorRevision: paint.editor.editorRevision,
            layerID: paint.editor.activeLayerID,
            planeID: paint.editor.activePlaneID
        )
    }

    private func isOlderPaintProjection(
        _ candidate: CorePaintProjection,
        for target: CoreSessionTarget
    ) -> Bool {
        guard let current = paintProjections[target] else { return false }
        let currentDocument = current.editor.session.documentRevision
        let candidateDocument = candidate.editor.session.documentRevision
        return candidateDocument < currentDocument
            || (candidateDocument == currentDocument
                && candidate.editor.editorRevision < current.editor.editorRevision)
    }

    private func syncToolOptions(with projection: CorePaintProjection) {
        guard commandContext?.session == projection.editor.session.target else { return }
        var replacement = toolOptionsPresentation
        guard replacement.activeToolChanged(to: projection.editor.activeTool) else { return }
        toolOptionsPresentation = replacement
    }

    private func colorPaneContext() -> CommandTargetContext? {
        guard let view = colorPaneTarget.resolve(
            active: editorGraph?.activeView,
            liveViews: editorGraph?.allViews ?? []
        ), let session = sessionProjections[view.session]
        else { return nil }
        return CommandTargetContext(
            workspaceID: id,
            lifecycleGeneration: lifecycleGeneration,
            session: view.session,
            view: view.coreTarget,
            documentRevision: session.documentRevision,
            viewRevision: view.viewRevision
        )
    }

    private func locatorPaneContext(
        preferredViewID: WorkspaceViewID? = nil
    ) -> CommandTargetContext? {
        if case .followActiveView = locatorPaneTarget.mode,
           let preferredViewID,
           let view = viewRecord(preferredViewID),
           let session = sessionProjections[view.session]
        {
            return CommandTargetContext(
                workspaceID: id,
                lifecycleGeneration: lifecycleGeneration,
                session: view.session,
                view: view.coreTarget,
                documentRevision: session.documentRevision,
                viewRevision: view.viewRevision
            )
        }
        guard let view = locatorPaneTarget.resolve(
            active: editorGraph?.activeView,
            liveViews: editorGraph?.allViews ?? []
        ), let session = sessionProjections[view.session]
        else { return nil }
        return CommandTargetContext(
            workspaceID: id,
            lifecycleGeneration: lifecycleGeneration,
            session: view.session,
            view: view.coreTarget,
            documentRevision: session.documentRevision,
            viewRevision: view.viewRevision
        )
    }

    private func observePaintMutation(
        _ task: CoreTask,
        requestsSnapshot: Bool,
        viewID: WorkspaceViewID? = nil
    ) {
        let generation = lifecycleGeneration
        observe(task, generation: generation) { [weak self] outcome in
            guard let self else { return }
            switch outcome {
            case let .documentUpdated(session):
                self.updateSessionProjection(session)
                self.lastCommandResult = .started
                self.refreshPaint()
                self.refreshHistory(rebuildVisualization: true)
            case let .paintUpdated(projection), let .eyedropperSampled(projection):
                guard !self.isOlderPaintProjection(
                    projection,
                    for: projection.editor.session.target
                ) else { return }
                self.paintProjections[projection.editor.session.target] = projection
                self.syncToolOptions(with: projection)
                self.updateSessionProjection(projection.editor.session)
                if self.colorPaneContext()?.session == projection.editor.session.target {
                    self.paint = projection
                }
                self.lastCommandResult = .started
            case let .fillApplied(result):
                self.updateSessionProjection(result.session)
                self.lastCommandResult = result.changedPixelCount == 0 ? .noOp : .started
                self.refreshPaint()
                self.refreshHistory(rebuildVisualization: true)
            case let .outputColorGuardApplied(result):
                self.updateSessionProjection(result.session)
                self.lastCommandResult = result.selectedPixelCount == 0 ? .noOp : .started
                self.refreshTree()
                self.refreshHistory(rebuildVisualization: true)
            case let .colorChartPreview(preview):
                self.colorChartPreview = preview
                self.lastCommandResult = .presentedInput
            case .acknowledged:
                self.lastCommandResult = .started
                self.refreshPaint()
            case .noOp:
                self.lastCommandResult = .noOp
                self.refreshPaint()
            case .failed(.staleTarget):
                self.lastCommandResult = .stale
                self.refreshPaint()
            case .failed(.cancelled), .failed(.coreOperation(.cancelled)):
                self.lastCommandResult = .cancelled
            case let .failed(failure):
                self.lastCommandResult = .failed(failure)
            default:
                self.lastCommandResult = .invalid
            }
            if requestsSnapshot {
                self.requestSnapshot(viewID: viewID)
            }
        }
    }

    private func isM8Cancellation(_ command: CoreM8Command) -> Bool {
        switch command {
        case .cancelFilterPreview, .cancelGeometryPreview,
             .shootingFramePreviewCancel, .vanishingPointPreviewCancel:
            true
        default:
            false
        }
    }

    private func layerPaneContext() -> CommandTargetContext? {
        guard let view = layerPaneTarget.resolve(
            active: editorGraph?.activeView,
            liveViews: editorGraph?.allViews ?? []
        ), let session = sessionProjections[view.session]
        else {
            layerPaneAccessibilityNotice = layerPaneTarget.consumeAccessibilityNotice()
            return nil
        }
        layerPaneAccessibilityNotice = layerPaneTarget.consumeAccessibilityNotice()
        return CommandTargetContext(
            workspaceID: id,
            lifecycleGeneration: lifecycleGeneration,
            session: view.session,
            view: view.coreTarget,
            documentRevision: session.documentRevision,
            viewRevision: view.viewRevision
        )
    }

    private func requestAllVisibleSnapshots() {
        guard let graph = editorGraph else { return }
        for group in graph.groups {
            requestSnapshot(viewID: group.activeViewID)
        }
    }

    private func observeCoreMutation(
        _ task: CoreTask,
        requestsSnapshot: Bool,
        viewID: WorkspaceViewID? = nil
    ) {
        let generation = lifecycleGeneration
        observe(task, generation: generation) { [weak self] outcome in
            guard let self else { return }
            switch outcome {
            case let .viewUpdated(projection):
                self.updateSessionProjection(projection)
                if let view = self.viewRecord(viewID), var graph = self.editorGraph {
                    _ = graph.updateViewRevision(
                        target: view.coreTarget,
                        revision: projection.viewRevision
                    )
                    self.editorGraph = graph
                }
            case let .documentUpdated(projection),
                 let .inspected(projection):
                self.updateSessionProjection(projection)
            case let .logicalViewUpdated(view):
                self.updateLogicalView(view)
            case .acknowledged, .noOp:
                break
            case let .failed(failure):
                if failure == .staleTarget || failure == .invalidTarget {
                    self.phase = .failed(failure)
                }
            default:
                break
            }
            if requestsSnapshot {
                self.requestSnapshot(viewID: viewID)
            }
        }
    }

    private func matches(_ context: CommandTargetContext) -> Bool {
        guard lifecycleGeneration == context.lifecycleGeneration,
              id == context.workspaceID,
              let session = sessionProjections[context.session],
              let view = editorGraph?.allViews.first(where: {
                  $0.coreTarget == context.view && $0.session == context.session
              })
        else {
            return false
        }
        return session.documentRevision == context.documentRevision
            && view.viewRevision == context.viewRevision
    }

    private func routeView(
        _ command: CoreViewCommand,
        expectation: CoreCommandExpectation,
        context: CommandTargetContext,
        onCommit: (@MainActor () -> Void)? = nil
    ) {
        routeCommand(
            application.coreHost.applyView(
                target: context.view,
                command: command,
                expectation: expectation
            ),
            context: context,
            onCommit: onCommit
        )
    }

    private func m8CommandIsSelected(_ command: InkpodCommandID) -> Bool {
        switch activeM8CanvasTool {
        case let .geometry(primitive):
            return geometryPrimitive(for: command) == primitive
        case let .vectorEraser(mode):
            if command == .vectorEraser { return true }
            return (command == .vectorErasePartial && mode == .partial)
                || (command == .vectorEraseIntersection && mode == .toIntersection)
                || (command == .vectorEraseWhole && mode == .wholePath)
        default:
            return vectorSelectionMode(for: command) == vectorSelectionMode
                && [.vectorSelectCut, .vectorSelectTouch, .vectorSelectContained,
                    .vectorSelectLine, .vectorSelectWholeLine, .vectorSelectIntersection,
                    .vectorSelectFillBoundary, .vectorSelectFill].contains(command)
        }
    }

    private func filterKind(for command: InkpodCommandID) -> CoreFilterKind? {
        switch command {
        case .filterInvert: .invert
        case .filterBlurWeak: .blurWeak
        case .filterSharpenWeak: .sharpenWeak
        case .filterSharpenStrong: .sharpenStrong
        case .filterBlurStrong: .blurStrong
        case .filterGaussian: .gaussianBlur
        case .filterAutoContrast: .autoContrast
        case .filterBrightness: .brightnessContrast
        case .filterToneCurve: .toneCurve
        case .filterLevels: .levels
        case .filterHSV: .hsv
        case .filterColorBalance: .colorBalance
        case .filterUnsharp: .unsharpMask
        default: nil
        }
    }

    private func defaultFilterParameters(
        _ kind: CoreFilterKind
    ) -> ([Int32], [CoreCurvePoint]) {
        switch kind {
        case .gaussianBlur: ([2, 1_000], [])
        case .brightnessContrast: ([0, 0], [])
        case .toneCurve: ([], [
            CoreCurvePoint(input: 0, output: 0),
            CoreCurvePoint(input: 65_535, output: 65_535),
        ])
        case .levels: ([0, 1_000, 65_535, 0, 65_535], [])
        case .hsv, .colorBalance: ([0, 0, 0], [])
        case .unsharpMask: ([2, 1_000, 0], [])
        default: ([], [])
        }
    }

    private func presentFilter(
        kind: CoreFilterKind,
        planeID: UInt64,
        context: CommandTargetContext,
        adjustmentLayerID: UInt64? = nil,
        createsAdjustment: Bool = false
    ) {
        let defaults = defaultFilterParameters(kind)
        let draft = M8FilterDraft(
            context: context,
            planeID: planeID,
            adjustmentLayerID: adjustmentLayerID,
            createsAdjustment: createsAdjustment,
            kind: kind,
            parameters: defaults.0,
            curvePoints: defaults.1
        )
        pendingM8Editor = .filter(draft)
        filterPreviewStarted = false
        filterPreviewRequestID = nil
        pendingFilterPreview = nil
        pendingFilterApply = nil
        guard adjustmentLayerID == nil, !createsAdjustment else { return }
        launchFilterPreview(draft, begin: true)
    }

    private func launchFilterPreview(_ draft: M8FilterDraft, begin: Bool) {
        guard matches(draft.context), draft.request.isValid,
              pendingM8Editor?.id == draft.id || pendingFilterApply?.id == draft.id
        else {
            lastCommandResult = .stale
            return
        }
        if !begin, let running = filterPreviewRequestID {
            pendingFilterPreview = draft
            _ = application.coreHost.cancel(request: running)
            return
        }
        let task = application.coreHost.performM8(
            target: draft.context.session,
            expectedDocumentRevision: draft.context.documentRevision,
            command: begin ? .beginFilterPreview(draft.request)
                : .updateFilterPreview(draft.request)
        )
        filterPreviewRequestID = task.requestID
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            guard let self, self.lifecycleGeneration == generation else { return }
            if self.filterPreviewRequestID == task.requestID {
                self.filterPreviewRequestID = nil
            }
            switch outcome {
            case .filterPreview:
                self.filterPreviewStarted = true
                self.lastCommandResult = .presentedInput
                self.requestSnapshot(viewID: self.viewID(for: draft.context))
            case .failed(.cancelled), .failed(.coreOperation(.cancelled)):
                break
            case .failed(.staleTarget):
                self.lastCommandResult = .stale
            case let .failed(failure):
                self.lastCommandResult = .failed(failure)
            default:
                self.lastCommandResult = .invalid
            }
            if let apply = self.pendingFilterApply {
                guard case .filterPreview = outcome
                    else {
                        if case .failed(.cancelled) = outcome {
                            self.launchFilterPreview(apply, begin: !self.filterPreviewStarted)
                        } else if case .failed(.coreOperation(.cancelled)) = outcome {
                            self.launchFilterPreview(apply, begin: !self.filterPreviewStarted)
                        } else {
                            self.pendingFilterApply = nil
                        }
                        return
                    }
                if apply.request == draft.request {
                    self.pendingFilterApply = nil
                    self.routeM8(.applyFilterPreview, context: apply.context)
                    self.filterPreviewStarted = false
                } else {
                    self.launchFilterPreview(apply, begin: false)
                }
                return
            }
            if let pending = self.pendingFilterPreview,
               self.pendingM8Editor?.id == pending.id
            {
                self.pendingFilterPreview = nil
                self.launchFilterPreview(pending, begin: !self.filterPreviewStarted)
            }
        }
    }

    private func geometryPrimitive(for command: InkpodCommandID) -> CoreGeometryPrimitive {
        switch command {
        case .vectorCurve: .curve
        case .vectorRectangle: .rectangle
        case .vectorEllipse: .ellipse
        case .vectorPolyline: .polyline
        case .vectorPolygon: .polygon
        default: .line
        }
    }

    private func vectorSelectionMode(
        for command: InkpodCommandID
    ) -> CoreVectorSelectionMode {
        switch command {
        case .vectorSelectCut: .cutBySelection
        case .vectorSelectContained: .fullyContained
        case .vectorSelectLine: .line
        case .vectorSelectWholeLine: .wholeLine
        case .vectorSelectIntersection: .toIntersection
        case .vectorSelectFillBoundary: .fillBoundary
        case .vectorSelectFill: .fill
        default: .touching
        }
    }

    private func effectCommand(from draft: M8EffectDraft) -> CoreEffectCommand? {
        let width = Double(projection?.documentWidth ?? 1)
        let height = Double(projection?.documentHeight ?? 1)
        switch draft.command {
        case .effectGradient, .effectAlphaGradient:
            return .gradient(
                CoreGradientRequest(
                    planeID: draft.planeID,
                    startX: 0,
                    startY: 0,
                    endX: width,
                    endY: height,
                    stops: [
                        CoreGradientStop(
                            positionMilli: 0,
                            color: .rgba8(red: 0, green: 0, blue: 0)
                        ),
                        CoreGradientStop(
                            positionMilli: 1_000,
                            color: .rgba8(red: 255, green: 255, blue: 255)
                        ),
                    ]
                ),
                alphaOnly: draft.command == .effectAlphaGradient
            )
        case .effectAirbrush:
            return .airbrush(
                planeID: draft.planeID,
                x: width / 2,
                y: height / 2,
                radius: max(draft.primary, 1),
                hardnessMilli: UInt32(clamping: Int64(draft.secondary.rounded())),
                opacityMilli: 750,
                color: .rgba8(red: 0, green: 0, blue: 0)
            )
        case .effectBoundaryAirbrush:
            return .boundaryAirbrush(
                planeID: draft.planeID,
                width: UInt32(clamping: Int64(max(draft.primary, 1).rounded())),
                strengthMilli: UInt32(clamping: Int64(draft.secondary.rounded())),
                colors: [.rgba8(red: 0, green: 0, blue: 0)]
            )
        case .effectBlur:
            return .blur(
                planeID: draft.planeID,
                radius: UInt32(clamping: Int64(max(draft.primary, 1).rounded())),
                strengthMilli: UInt32(clamping: Int64(draft.secondary.rounded()))
            )
        case .effectStamp:
            let side = UInt32(clamping: Int64(max(draft.primary, 1).rounded()))
            return .stamp(
                planeID: draft.planeID,
                sourceX: 0,
                sourceY: 0,
                destinationX: Int32(clamping: Int64(width / 2)),
                destinationY: Int32(clamping: Int64(height / 2)),
                width: side,
                height: side,
                opacityMilli: UInt32(clamping: Int64(draft.secondary.rounded()))
            )
        case .effectDust:
            return .dust(
                planeID: draft.planeID,
                mode: .removeForeground,
                maximumPixels: draft.maximumPixels
            )
        default:
            return nil
        }
    }

    private func presentAnnotation(
        layerID: UInt64,
        objectID: UInt64?,
        context: CommandTargetContext
    ) {
        let seed = objectID.flatMap { annotationSeeds[$0] }
        pendingM8Editor = .annotation(M8AnnotationDraft(
            context: context,
            layerID: seed?.layerID ?? layerID,
            objectID: objectID,
            text: seed?.text ?? "Note",
            instructionOnly: seed?.instructionOnly ?? true,
            fontFamily: seed?.fontFamily ?? "",
            fontSize: seed?.fontSize ?? 18,
            x: seed?.bounds.x ?? 24,
            y: seed?.bounds.y ?? 24,
            width: seed?.bounds.width ?? 320,
            height: seed?.bounds.height ?? 80
        ))
    }

    private func presentShootingFrame(context: CommandTargetContext) {
        let frame = currentShootingFrame() ?? CoreShootingFrame(
            centerX: Double(projection?.documentWidth ?? 1) / 2,
            centerY: Double(projection?.documentHeight ?? 1) / 2,
            width: Double(projection?.documentWidth ?? 1) * 0.8,
            height: Double(projection?.documentHeight ?? 1) * 0.8
        )
        let draft = M8ShootingFrameDraft(context: context, frame: frame)
        pendingM8Editor = .shootingFrame(draft)
        routeM8(
            frame.id == 0 ? .shootingFrameCreate(frame, preview: true)
                : .shootingFrameUpdate(frame, preview: true),
            context: context
        )
    }

    private func currentShootingFrame() -> CoreShootingFrame? {
        guard let frame = m8State?.shootingFrame else { return nil }
        return CoreShootingFrame(
            id: frame.id,
            anchor: frame.anchor,
            centerX: Double(frame.centerXMilli) / 1_000,
            centerY: Double(frame.centerYMilli) / 1_000,
            width: Double(frame.widthMilli) / 1_000,
            height: Double(frame.heightMilli) / 1_000,
            rotationDegrees: Double(frame.rotationTurns) * 360 / 4_294_967_296,
            visible: frame.visible,
            includeInInstructionExport: frame.includeInInstructionExport
        )
    }

    private func presentVanishingPoint(layerID: UInt64, context: CommandTargetContext) {
        let point = m8State?.vanishingPoints.first ?? CoreVanishingPoint(
            layerID: layerID,
            xMilli: Int64(projection?.documentWidth ?? 1) * 500,
            yMilli: Int64(projection?.documentHeight ?? 1) * 500
        )
        let draft = M8VanishingPointDraft(context: context, point: point)
        pendingM8Editor = .vanishingPoint(draft)
        routeM8(
            point.id == 0 ? .vanishingPointCreate(point, preview: true)
                : .vanishingPointUpdate(point, preview: true),
            context: context
        )
    }

    private func selectAdjacentAdjustment(
        previous: Bool,
        context: CommandTargetContext
    ) {
        guard let tree = treeProjections[context.session] else { return }
        let adjustments = tree.layers.filter { $0.kind == .adjustment }
        guard !adjustments.isEmpty else { return }
        let current = adjustments.firstIndex { $0.id == tree.activeLayerID } ?? 0
        let offset = previous ? -1 : 1
        let next = (current + offset + adjustments.count) % adjustments.count
        let layer = adjustments[next]
        selectNode(layerID: layer.id, planeID: layer.planes.first?.id ?? 0)
    }

    private func selectAdjacentAnnotation(previous: Bool) {
        guard !knownAnnotationIDs.isEmpty else { return }
        let current = selectedAnnotationID.flatMap { knownAnnotationIDs.firstIndex(of: $0) } ?? 0
        let offset = previous ? -1 : 1
        selectedAnnotationID = knownAnnotationIDs[
            (current + offset + knownAnnotationIDs.count) % knownAnnotationIDs.count
        ]
    }

    private func firstEditableRasterPlane() -> CoreNodeProjection? {
        cellTree?.layers.lazy
            .filter { $0.kind != .adjustment && $0.kind != .annotation }
            .flatMap(\.planes)
            .first { $0.pixelFormat != .none && $0.isEditable }
    }

    private func annotationBounds(_ points: [CoreAnnotationPoint]) -> CoreFrameRect {
        guard let first = points.first else {
            return CoreFrameRect(x: 0, y: 0, width: 0, height: 0)
        }
        let minX = points.reduce(first.xMilli) { min($0, $1.xMilli) }
        let minY = points.reduce(first.yMilli) { min($0, $1.yMilli) }
        let maxX = points.reduce(first.xMilli) { max($0, $1.xMilli) }
        let maxY = points.reduce(first.yMilli) { max($0, $1.yMilli) }
        return CoreFrameRect(
            x: minX / 1_000,
            y: minY / 1_000,
            width: max(0, (maxX - minX) / 1_000),
            height: max(0, (maxY - minY) / 1_000)
        )
    }

    private func routeM8(_ command: CoreM8Command, context: CommandTargetContext) {
        guard matches(context), command.isValid else {
            lastCommandResult = matches(context) ? .invalid : .stale
            return
        }
        let generation = lifecycleGeneration
        let task = application.coreHost.performM8(
            target: context.session,
            expectedDocumentRevision: context.documentRevision,
            command: command
        )
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            guard let self, self.lifecycleGeneration == generation, self.phase == .ready else {
                return
            }
            switch outcome {
            case let .m8State(state):
                self.m8State = state
                self.updateSessionProjection(state.session)
                self.lastCommandResult = .started
            case let .m8Mutation(mutation):
                self.m8State = mutation.state
                self.updateSessionProjection(mutation.state.session)
                if case .annotation = command {
                    self.knownAnnotationIDs.append(contentsOf: mutation.createdIDs)
                    self.knownAnnotationIDs = Array(Set(self.knownAnnotationIDs)).sorted()
                    if let created = mutation.createdIDs.last {
                        self.selectedAnnotationID = created
                    }
                    if case let .annotation(edits) = command {
                        for edit in edits {
                            if case let .delete(id) = edit {
                                self.knownAnnotationIDs.removeAll { $0 == id }
                                if self.selectedAnnotationID == id {
                                    self.selectedAnnotationID = self.knownAnnotationIDs.first
                                }
                            }
                        }
                    }
                }
                self.lastCommandResult = mutation.state.session.documentRevision
                    == context.documentRevision ? .noOp : .started
                self.refreshTree()
                self.refreshPaint()
                self.refreshHistory(rebuildVisualization: true)
            case let .filterPreview(preview):
                self.updateSessionProjection(preview.session)
                self.lastCommandResult = .presentedInput
            case let .geometryPreview(preview):
                self.updateSessionProjection(preview.session)
                self.lastCommandResult = .presentedInput
            case let .vectorSelection(selection):
                self.vectorSelection = selection
                self.updateSessionProjection(selection.session)
                self.lastCommandResult = selection.ranges.isEmpty && selection.fillIDs.isEmpty
                    ? .noOp : .started
            case let .noOp(session):
                if let session { self.updateSessionProjection(session) }
                self.lastCommandResult = .noOp
            case .failed(.staleTarget):
                self.lastCommandResult = .stale
            case .failed(.cancelled), .failed(.coreOperation(.cancelled)):
                self.lastCommandResult = .cancelled
            case .failed(.coreOperation(.invalidState))
                where self.isM8Cancellation(command):
                self.lastCommandResult = .cancelled
            case let .failed(failure):
                self.lastCommandResult = .failed(failure)
            default:
                self.lastCommandResult = .invalid
            }
            self.requestAllVisibleSnapshots()
        }
    }

    private func routeDocument(
        _ command: CoreDocumentCommand,
        context: CommandTargetContext,
        onCommit: (@MainActor () -> Void)? = nil
    ) {
        routeCommand(
            application.coreHost.applyDocument(
                target: context.session,
                command: command,
                expectedDocumentRevision: context.documentRevision
            ),
            context: context,
            onCommit: onCommit
        )
    }

    private func routeCommand(
        _ task: CoreTask,
        context _: CommandTargetContext,
        onCommit: (@MainActor () -> Void)? = nil
    ) {
        let generation = lifecycleGeneration
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            guard let self, self.lifecycleGeneration == generation, self.phase == .ready else {
                return
            }
            switch outcome {
            case let .viewUpdated(projection):
                self.updateSessionProjection(projection)
                onCommit?()
                self.lastCommandResult = .started
                self.requestAllVisibleSnapshots()
                self.refreshTree()
                self.refreshPaint()
            case let .documentUpdated(projection):
                self.updateSessionProjection(projection)
                onCommit?()
                self.lastCommandResult = .started
                self.requestAllVisibleSnapshots()
                self.refreshTree()
                self.refreshPaint()
                self.refreshHistory(rebuildVisualization: true)
            case let .logicalViewUpdated(view):
                self.updateLogicalView(view)
                onCommit?()
                self.lastCommandResult = .started
                self.requestSnapshot(viewID: self.editorGraph?.activeView?.id)
            case let .cellUpdated(session):
                self.updateSessionProjection(session)
                onCommit?()
                self.lastCommandResult = .started
                self.requestAllVisibleSnapshots()
                self.refreshTree()
                self.refreshPaint()
                self.refreshHistory(rebuildVisualization: true)
            case let .treeUpdated(update):
                self.treeProjections[update.tree.session.target] = update.tree
                self.updateSessionProjection(update.tree.session)
                if self.layerPaneContext()?.session == update.tree.session.target {
                    self.cellTree = update.tree
                }
                onCommit?()
                self.lastCommandResult = .started
                self.requestAllVisibleSnapshots()
                self.refreshPaint()
                self.refreshHistory(rebuildVisualization: true)
            case let .documentCommandUpdated(update):
                self.updateSessionProjection(update.session)
                self.lastAffectedGuideID = update.affectedGuideID
                onCommit?()
                self.lastAffectedGuideID = nil
                self.lastCommandResult = .started
                self.requestAllVisibleSnapshots()
                self.refreshHistory(rebuildVisualization: true)
            case .noOp:
                self.lastCommandResult = .noOp
            case .failed(.staleTarget):
                self.lastCommandResult = .stale
            case let .failed(failure):
                self.lastCommandResult = .failed(failure)
            default:
                self.lastCommandResult = .invalid
            }
        }
    }

    private func observe(
        _ task: CoreTask,
        generation: UInt64,
        completion: @escaping @MainActor (CoreRequestOutcome) -> Void
    ) {
        Task { @MainActor [weak self] in
            let outcome = await task.value()
            guard let self,
                  self.lifecycleGeneration == generation,
                  self.phase != .stopped
            else {
                if case let .snapshot(envelope) = outcome {
                    envelope.owner.release()
                }
                return
            }
            completion(outcome)
        }
    }

    private func observeRendererCompletion(generation: UInt64) {
        let renderer = application.rendererHost
        Task.detached {
            guard renderer.waitUntilIdle(timeout: 10) else { return }
            let frameCount = renderer.metrics().presentedFrameCount
            await MainActor.run { [weak self] in
                guard let self,
                      self.lifecycleGeneration == generation,
                      self.phase == .ready
                else {
                    return
                }
                self.presentedFrameCount = frameCount
            }
        }
    }
}

@MainActor
private extension WorkspaceModel {
    enum M9PaneKind {
        case sequence
        case lightTable
        case subpalette
    }

    func animationPaneContext() -> CommandTargetContext? {
        m9PaneContext(for: .sequenceGoto)
    }

    func m9PaneContext(for command: InkpodCommandID) -> CommandTargetContext? {
        guard let active = editorGraph?.activeView else { return nil }
        let kind: M9PaneKind
        if (command.rawValue >= InkpodCommandID.lightTableSetNew.rawValue
            && command.rawValue <= InkpodCommandID.lightTableBulkBoth.rawValue)
            || command == .lightTablePin || command == .windowLightTable
        {
            kind = .lightTable
        } else if command == .subpaletteSet || command == .subpaletteSample
            || command == .subpalettePin || command == .windowSubpalette
        {
            kind = .subpalette
        } else {
            kind = .sequence
        }
        let originalTarget: PaneTargetRecord = switch kind {
        case .sequence: sequencePaneTarget
        case .lightTable: lightTablePaneTarget
        case .subpalette: subpalettePaneTarget
        }
        var target = originalTarget
        guard let view = target.resolve(
            active: active,
            liveViews: editorGraph?.allViews ?? []
        ), let session = sessionProjections[view.session]
        else {
            if target != originalTarget {
                switch kind {
                case .sequence: sequencePaneTarget = target
                case .lightTable: lightTablePaneTarget = target
                case .subpalette: subpalettePaneTarget = target
                }
            }
            return nil
        }
        return CommandTargetContext(
            workspaceID: id,
            lifecycleGeneration: lifecycleGeneration,
            session: view.session,
            view: view.coreTarget,
            documentRevision: session.documentRevision,
            viewRevision: view.viewRevision
        )
    }

    func toggleM9Pane(_ kind: M9PaneKind, context: CommandTargetContext) {
        guard let issuedView = editorGraph?.allViews.first(where: {
            $0.session == context.session && $0.coreTarget == context.view
        }) else {
            lastCommandResult = .stale
            return
        }
        var target: PaneTargetRecord = switch kind {
        case .sequence: sequencePaneTarget
        case .lightTable: lightTablePaneTarget
        case .subpalette: subpalettePaneTarget
        }
        if target.isPinned {
            target.follow()
        } else {
            target.pin(to: issuedView)
        }
        switch kind {
        case .sequence: sequencePaneTarget = target
        case .lightTable: lightTablePaneTarget = target
        case .subpalette: subpalettePaneTarget = target
        }
        refreshAnimation()
    }

    func motionFPSValue(for command: InkpodCommandID) -> UInt32 {
        switch command {
        case .motionFPS30: 30
        case .motionFPS25: 25
        case .motionFPS12: 12
        case .motionFPS10: 10
        case .motionFPS8: 8
        default: 24
        }
    }

    func installAnimation(_ state: CoreAnimationProjection) {
        let updatesActiveSession = projection?.target == state.session.target
        let previousUUID = updatesActiveSession ? projection?.documentUUID : nil
        updateSessionProjection(state.session)
        animationProjections[state.session.target] = state
        if animationPaneContext()?.session == state.session.target {
            animation = state
        }
        if updatesActiveSession, previousUUID != nil,
           previousUUID != state.session.documentUUID
        {
            application.fileIdentityRegistry.release(session: state.session.target)
            sessionDocumentURLs.removeValue(forKey: state.session.target)
            sessionFileIdentities.removeValue(forKey: state.session.target)
            documentURL = nil
            fileIdentity = nil
        }
        if m9PaneContext(for: .lightTableSetNew)?.session == state.session.target {
            if selectedLightTableSetID == nil
                || !state.lightTableSets.contains(where: { $0.id == selectedLightTableSetID })
            {
                selectedLightTableSetID = state.lightTableSets.first(where: \.isActive)?.id
                    ?? state.lightTableSets.first?.id
                selectedLightTableItemID = nil
            }
            if let itemID = selectedLightTableItemID,
               !state.lightTableSets.flatMap(\.items).contains(where: { $0.id == itemID })
            {
                selectedLightTableItemID = nil
            }
        }
        requestAllVisibleSnapshots()
    }

    func routeM9Animation(
        _ command: CoreAnimationCommand,
        context explicitContext: CommandTargetContext? = nil
    ) {
        guard let context = explicitContext ?? m9IssueContext ?? animationPaneContext(),
              matches(context)
        else {
            lastCommandResult = .stale
            return
        }
        let generation = lifecycleGeneration
        observe(
            application.coreHost.performAnimation(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                command: command
            ),
            generation: generation
        ) { [weak self] outcome in
            guard let self else { return }
            switch outcome {
            case let .animation(state):
                self.installAnimation(state)
                self.lastCommandResult = .started
            case let .animationMutation(mutation):
                self.installAnimation(mutation.state)
                self.lastCommandResult = mutation.applied ? .started : .noOp
                self.refreshTree()
                self.refreshPaint()
                self.refreshM8()
                self.refreshHistory(rebuildVisualization: true)
            case let .lightTableBulkPreview(preview):
                self.pendingM9Editor = .bulk(M9LightTableBulkDraft(
                    context: context,
                    preview: preview
                ))
                self.lastCommandResult = .presentedInput
            case .motion:
                self.lastCommandResult = .started
                self.refreshAnimation()
            case let .animationSample(color):
                self.chooseColor(color)
                self.lastCommandResult = .started
            case .noOp:
                self.lastCommandResult = .noOp
            case .failed(.staleTarget):
                self.lastCommandResult = .stale
                self.refreshAnimation()
            case .failed(.cancelled), .failed(.coreOperation(.cancelled)):
                self.lastCommandResult = .cancelled
            case let .failed(failure):
                self.lastCommandResult = .failed(failure)
            default:
                self.lastCommandResult = .invalid
            }
        }
    }

    func createCut(_ draft: M9CutEditorDraft) {
        guard cut == nil,
              let options = draft.defaults.cellCreationOptions(count: draft.cellCount),
              let graph = editorGraph,
              graph.allViews.count + Int(draft.cellCount) <= graph.maximumViewCount
        else {
            lastCommandResult = .invalid
            return
        }
        Task { await createCutWorkflow(draft: draft, options: options) }
    }

    func createCutWorkflow(
        draft: M9CutEditorDraft,
        options: CoreCellCreationOptions
    ) async {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.message = application.languageController.text("m9.cut.chooseFolder.message")
        panel.prompt = application.languageController.text("m9.cut.chooseFolder.action")
        guard await panelResponse(panel) == .OK, let directory = panel.url,
              let stem = cutFileStem(draft.metadata.cutName)
        else {
            lastCommandResult = .cancelled
            return
        }
        let descriptor = directory.appending(path: "\(stem).inkpod")
        let cellURLs = (1 ... Int(draft.cellCount)).map {
            directory.appending(path: String(format: "\(stem)-%04d.inkpod", $0))
        }
        let outputURLs = [descriptor] + cellURLs
        guard Set(outputURLs.map { $0.standardizedFileURL.path }).count == outputURLs.count,
              outputURLs.allSatisfy({ !FileManager.default.fileExists(atPath: $0.path) })
        else {
            lastCommandResult = .invalid
            return
        }

        isFileOperationActive = true
        defer { isFileOperationActive = false }
        let generation = lifecycleGeneration
        let host = application.coreHost
        let planOutcome = await host.prepareCellCreation(options).value()
        guard case let .cellPlan(plan) = planOutcome else {
            handleM9Failure(planOutcome)
            return
        }
        guard lifecycleGeneration == generation else {
            _ = await host.cancelCellCreation(plan.id).value()
            lastCommandResult = .stale
            return
        }
        let documentUUIDs = plan.items.map { _ in WorkspaceID().coreDocumentUUID }
        let createOutcome = await host.commitCellCreation(
            plan: plan.id,
            documentUUIDs: documentUUIDs
        ).value()
        guard case let .cellsCreated(created) = createOutcome,
              created.count == cellURLs.count
        else {
            handleM9Failure(createOutcome)
            return
        }

        var writtenURLs: [URL] = []
        var reservations: [FileIdentityReservation] = []
        var createdCutTarget: CoreCutTarget?
        func rollback() async {
            if let createdCutTarget { _ = await host.closeCut(createdCutTarget).value() }
            for session in created {
                application.fileIdentityRegistry.release(session: session.target)
                _ = await host.closeSession(session.target).value()
            }
            for reservation in reservations {
                application.fileIdentityRegistry.cancel(reservation)
            }
            for url in writtenURLs.reversed() {
                try? FileManager.default.removeItem(at: url)
            }
        }

        for (session, url) in zip(created, cellURLs) {
            let identity = FileIdentity.resolve(url)
            guard application.fileIdentityRegistry.owner(of: identity) == nil,
                  let reservation = application.fileIdentityRegistry.reserve(
                      identity,
                      for: session.target
                  )
            else {
                await rollback()
                lastCommandResult = .stale
                return
            }
            reservations.append(reservation)
        }

        var savedSessions: [CoreSessionProjection] = []
        for (session, url) in zip(created, cellURLs) {
            let outcome = await coordinatedM9Save(
                target: session.target,
                revision: session.documentRevision,
                destination: url
            )
            guard case let .fileCompleted(file) = outcome,
                  file.operation == .save
            else {
                await rollback()
                handleM9Failure(outcome)
                return
            }
            savedSessions.append(file.session)
            writtenURLs.append(url)
        }

        let members = zip(savedSessions, cellURLs).enumerated().map { index, pair in
            CoreCutMember(
                displayNumber: UInt32(index + 1),
                cellID: pair.0.cellID,
                documentUUID: pair.0.documentUUID,
                relativePath: pair.1.lastPathComponent
            )
        }
        guard let animationStates = await installCutMemberSequences(
            members: members,
            sessions: savedSessions
        ) else {
            await rollback()
            return
        }
        let cutUUID = WorkspaceID().coreDocumentUUID
        let cutOutcome = await host.createCut(
            cutUUID: CoreCutUUID(high: cutUUID.high, low: cutUUID.low),
            metadata: draft.metadata,
            defaults: draft.defaults,
            members: members
        ).value()
        guard case let .cut(createdCut) = cutOutcome else {
            await rollback()
            handleM9Failure(cutOutcome)
            return
        }
        createdCutTarget = createdCut.target
        let saveCutOutcome = await coordinatedM9CutSave(
            target: createdCut.target,
            revision: createdCut.revision,
            destination: descriptor
        )
        guard case let .cut(savedCut) = saveCutOutcome else {
            await rollback()
            handleM9Failure(saveCutOutcome)
            return
        }
        writtenURLs.append(descriptor)

        guard lifecycleGeneration == generation, var graph = editorGraph else {
            await rollback()
            lastCommandResult = .stale
            return
        }
        let records = zip(savedSessions, cellURLs).map { session, url in
            WorkspaceViewRecord(
                id: WorkspaceViewID(rawValue: session.primaryView.id.rawValue),
                coreTarget: session.primaryView,
                session: session.target,
                viewRevision: session.viewRevision,
                title: url.deletingPathExtension().lastPathComponent
            )
        }
        guard graph.appendAtomically(records, to: graph.activeGroupID) else {
            await rollback()
            lastCommandResult = .failed(.sessionLimit)
            return
        }
        for (reservation, url) in zip(reservations, cellURLs) {
            guard application.fileIdentityRegistry.commit(
                reservation,
                as: FileIdentity.resolve(url)
            ) else {
                await rollback()
                lastCommandResult = .stale
                return
            }
        }
        for (session, url) in zip(savedSessions, cellURLs) {
            sessionProjections[session.target] = session
            sessionDocumentURLs[session.target] = url
            sessionFileIdentities[session.target] = FileIdentity.resolve(url)
            application.claimSession(session.target, for: id)
        }
        for state in animationStates {
            animationProjections[state.session.target] = state
        }
        editorGraph = graph
        projection = savedSessions.last
        documentURL = cellURLs.last
        fileIdentity = cellURLs.last.map { FileIdentity.resolve($0) }
        cut = savedCut
        cutURL = descriptor
        selectedCutMemberIndex = savedCut.members.isEmpty ? nil : 0
        application.recordRecent(url: descriptor, identity: FileIdentity.resolve(descriptor))
        window?.representedURL = documentURL
        window?.title = documentURL?.lastPathComponent ?? stem
        lastCommandResult = .started
        refreshTree()
        refreshPaint()
        refreshHistory(rebuildVisualization: true)
        refreshM8()
        refreshAnimation()
        requestAllVisibleSnapshots()
    }

    func coordinatedM9Save(
        target: CoreSessionTarget,
        revision: UInt64,
        destination: URL
    ) async -> CoreRequestOutcome {
        let broker = application.fileAccessBroker
        let host = application.coreHost
        return await Task.detached {
            do {
                return try broker.coordinateReplacing(destination) { coordinatedURL in
                    host.save(
                        target: target,
                        expectedDocumentRevision: revision,
                        pathUTF8: Array(coordinatedURL.path.utf8),
                        allowCleanSave: true
                    ).wait(timeout: 120) ?? .failed(.cancelled)
                }
            } catch {
                return .failed(.coreOperation(.ioError))
            }
        }.value
    }

    func coordinatedM9CutSave(
        target: CoreCutTarget,
        revision: UInt64,
        destination: URL
    ) async -> CoreRequestOutcome {
        let broker = application.fileAccessBroker
        let host = application.coreHost
        return await Task.detached {
            do {
                return try broker.coordinateReplacing(destination) { coordinatedURL in
                    host.saveCut(
                        target: target,
                        expectedRevision: revision,
                        pathUTF8: Array(coordinatedURL.path.utf8)
                    ).wait(timeout: 120) ?? .failed(.cancelled)
                }
            } catch {
                return .failed(.coreOperation(.ioError))
            }
        }.value
    }

    func cutFileStem(_ value: String) -> String? {
        let invalid = CharacterSet.controlCharacters.union(
            CharacterSet(charactersIn: "/\\:")
        )
        let replaced = value.unicodeScalars.map {
            invalid.contains($0) ? "-" : String($0)
        }.joined().trimmingCharacters(in: .whitespacesAndNewlines)
        guard !replaced.isEmpty, replaced != ".", replaced != "..",
              replaced.utf8.count <= 180
        else { return nil }
        return replaced
    }

    func updateCut(_ draft: M9CutEditorDraft) {
        guard let target = draft.cutTarget else {
            lastCommandResult = .invalid
            return
        }
        routeCut(application.coreHost.updateCut(
            target: target,
            expectedRevision: draft.expectedCutRevision,
            metadata: draft.metadata,
            defaults: draft.defaults
        ))
    }

    func routeCut(_ task: CoreTask, onSuccess: (@MainActor () -> Void)? = nil) {
        let generation = lifecycleGeneration
        observe(task, generation: generation) { [weak self] outcome in
            guard let self else { return }
            switch outcome {
            case let .cut(value):
                self.cut = value
                self.normalizeCutSelection()
                self.lastCommandResult = .started
                onSuccess?()
            case let .cutMutation(value):
                self.cut = value.cut
                self.normalizeCutSelection()
                self.lastCommandResult = value.applied ? .started : .noOp
                if value.applied {
                    Task { _ = await self.synchronizeCutMemberSequences() }
                }
            case .acknowledged:
                self.lastCommandResult = .started
            case .noOp:
                self.lastCommandResult = .noOp
            case .failed(.staleTarget):
                self.lastCommandResult = .stale
            case .failed(.cancelled), .failed(.coreOperation(.cancelled)):
                self.lastCommandResult = .cancelled
            case let .failed(failure):
                self.lastCommandResult = .failed(failure)
            default:
                self.lastCommandResult = .invalid
            }
        }
    }

    func normalizeCutSelection() {
        guard let index = selectedCutMemberIndex else { return }
        if let count = cut?.members.count, count > 0 {
            selectedCutMemberIndex = min(index, count - 1)
        } else {
            selectedCutMemberIndex = nil
        }
    }

    func activateCutMember(_ member: CoreCutMember) {
        guard let session = sessionProjections.values.first(where: {
            $0.documentUUID == member.documentUUID && $0.cellID == member.cellID
        }), let view = editorGraph?.allViews.first(where: { $0.session == session.target }),
              let group = editorGraph?.groups.first(where: { group in
                  group.views.contains(where: { $0.id == view.id })
              })
        else {
            lastCommandResult = .stale
            return
        }
        activate(groupID: group.id, viewID: view.id)
        lastCommandResult = .started
    }

    func installCutMemberSequences(
        members: [CoreCutMember],
        sessions: [CoreSessionProjection]
    ) async -> [CoreAnimationProjection]? {
        guard !members.isEmpty, members.count == sessions.count else {
            lastCommandResult = .invalid
            return nil
        }
        let byUUID = Dictionary(uniqueKeysWithValues: sessions.map { ($0.documentUUID, $0) })
        var sources: [CoreIdentifiedNamedRaster] = []
        sources.reserveCapacity(members.count)
        for (index, member) in members.enumerated() {
            guard let session = byUUID[member.documentUUID], session.cellID == member.cellID else {
                lastCommandResult = .stale
                return nil
            }
            let exported = await application.coreHost.exportCommonRaster(
                target: session.target,
                expectedDocumentRevision: session.documentRevision,
                format: .png,
                compositeWhite: false
            ).value()
            guard case let .rasterExported(raster) = exported else {
                handleM9Failure(exported)
                return nil
            }
            let name = String(
                format: "cut-%05d-cell-%u.png",
                index + 1,
                member.displayNumber
            )
            sources.append(CoreIdentifiedNamedRaster(
                raster: CoreNamedRaster(name: name, format: .png, bytes: raster.bytes),
                documentUUID: member.documentUUID,
                sourceGeneration: session.documentRevision
            ))
        }
        var states: [CoreAnimationProjection] = []
        states.reserveCapacity(sessions.count)
        for session in sessions {
            let installed = await application.coreHost.performAnimation(
                target: session.target,
                expectedDocumentRevision: session.documentRevision,
                command: .importIdentifiedSequence(sources)
            ).value()
            guard case let .animation(state) = installed else {
                handleM9Failure(installed)
                return nil
            }
            states.append(state)
        }
        return states
    }

    func synchronizeCutMemberSequences() async -> Bool {
        guard let cut else { return true }
        let sessions = cut.members.compactMap { member in
            sessionProjections.values.first {
                $0.documentUUID == member.documentUUID && $0.cellID == member.cellID
            }
        }
        guard sessions.count == cut.members.count,
              let states = await installCutMemberSequences(
                  members: cut.members,
                  sessions: sessions
              )
        else {
            lastCommandResult = .stale
            return false
        }
        for state in states { installAnimation(state) }
        return true
    }

    func addCurrentCellToCut() {
        guard let cut, let cutURL, let documentURL, let projection,
              cutURL.deletingLastPathComponent().standardizedFileURL
                == documentURL.deletingLastPathComponent().standardizedFileURL
        else {
            lastCommandResult = .invalid
            return
        }
        let member = CoreCutMember(
            displayNumber: UInt32(clamping: cut.members.count + 1),
            cellID: projection.cellID,
            documentUUID: projection.documentUUID,
            relativePath: documentURL.lastPathComponent
        )
        routeCut(application.coreHost.editCutSequence(
            target: cut.target,
            expectedRevision: cut.revision,
            operations: [.insert(member, position: UInt32(clamping: cut.members.count))]
        ))
    }

    func moveSelectedCutMember(delta: Int) {
        guard let cut, let source = selectedCutMemberIndex,
              cut.members.indices.contains(source),
              cut.members.indices.contains(source + delta)
        else { lastCommandResult = .noOp; return }
        let member = cut.members[source]
        let anchor = cut.members[source + delta]
        let operation: CoreCutSequenceOperation = delta < 0
            ? .moveBefore(
                cellID: member.cellID,
                documentUUID: member.documentUUID,
                anchorCellID: anchor.cellID,
                anchorDocumentUUID: anchor.documentUUID
            )
            : .moveAfter(
                cellID: member.cellID,
                documentUUID: member.documentUUID,
                anchorCellID: anchor.cellID,
                anchorDocumentUUID: anchor.documentUUID
            )
        selectedCutMemberIndex = source + delta
        routeCut(application.coreHost.editCutSequence(
            target: cut.target,
            expectedRevision: cut.revision,
            operations: [operation]
        ))
    }

    func reorderSelectedLightTableSet(delta: Int) {
        guard let state = animation, let set = selectedLightTableSet,
              let source = state.lightTableSets.firstIndex(where: { $0.id == set.id })
        else { lastCommandResult = .noOp; return }
        let destination = source + delta
        guard state.lightTableSets.indices.contains(destination) else {
            lastCommandResult = .noOp
            return
        }
        routeM9Animation(.editLightTable(.reorderSet(
            id: set.id,
            destinationIndex: UInt32(clamping: destination)
        )))
    }

    func reorderSelectedLightTableItem(delta: Int) {
        guard let set = selectedLightTableSet, let item = selectedLightTableItem,
              let source = set.items.firstIndex(where: { $0.id == item.id })
        else { lastCommandResult = .noOp; return }
        let destination = source + delta
        guard set.items.indices.contains(destination) else {
            lastCommandResult = .noOp
            return
        }
        routeM9Animation(.editLightTable(.reorderItem(
            id: item.id,
            destinationIndex: UInt32(clamping: destination)
        )))
    }

    func previewLightTableBulk(_ direction: CoreLightTableBulkDirection) {
        guard let set = selectedLightTableSet else { return }
        routeM9Animation(.previewLightTableBulk(
            setID: set.id,
            direction: direction,
            neighborCount: 2,
            baseOpacityMilli: 800,
            distanceStepMilli: 150
        ))
    }
}

@MainActor
private extension WorkspaceModel {
    func autosaveCutNow() async {
        guard !isFileOperationActive, let cut else { return }
        do {
            let directory = try recoveryDirectory()
            let destination = directory.appending(
                path: "\(id.rawValue.uuidString)-cut.inkpod"
            )
            let broker = application.fileAccessBroker
            let host = application.coreHost
            isFileOperationActive = true
            let outcome = await Task.detached {
                do {
                    return try broker.coordinateReplacing(destination) { coordinatedURL in
                        host.autosaveCut(
                            target: cut.target,
                            expectedRevision: cut.revision,
                            pathUTF8: Array(coordinatedURL.path.utf8)
                        ).wait(timeout: 120) ?? .failed(.cancelled)
                    }
                } catch {
                    return .failed(.coreOperation(.ioError))
                }
            }.value
            isFileOperationActive = false
            switch outcome {
            case let .cut(value):
                self.cut = value
                do {
                    _ = try application.recoveryStore.publish(
                        artifactURL: destination,
                        session: CoreSessionTarget(
                            id: CoreSessionID(rawValue: value.target.id.rawValue),
                            generation: CoreSessionGeneration(
                                rawValue: value.target.generation.rawValue
                            )
                        ),
                        documentUUID: CoreDocumentUUID(
                            high: value.cutUUID.high,
                            low: value.cutUUID.low
                        ),
                        originalPath: cutURL?.path,
                        writtenAtMilliseconds: UInt64(
                            max(1, Date().timeIntervalSince1970 * 1_000)
                        )
                    )
                } catch {
                    try? FileManager.default.removeItem(at: destination)
                    try? FileManager.default.removeItem(
                        at: application.recoveryStore.metadataURL(for: destination)
                    )
                    lastCommandResult = .failed(.coreOperation(.ioError))
                    return
                }
                lastCommandResult = .started
            default:
                handleM9Failure(outcome)
            }
        } catch {
            isFileOperationActive = false
            lastCommandResult = .failed(.coreOperation(.ioError))
        }
    }

    func saveCut(chooseDestination: Bool) async -> Bool {
        guard !isFileOperationActive, let cut else { return false }
        var destination = cutURL
        if chooseDestination || destination == nil {
            let panel = NSSavePanel()
            panel.allowedContentTypes = [FileTypeCatalog.native]
            panel.nameFieldStringValue = cutURL?.lastPathComponent
                ?? "\(cut.metadata.cutName).inkpod"
            guard await panelResponse(panel) == .OK, let selected = panel.url else {
                lastCommandResult = .cancelled
                return false
            }
            destination = selected
        }
        guard let destination else { return false }
        let broker = application.fileAccessBroker
        let host = application.coreHost
        isFileOperationActive = true
        let outcome = await Task.detached {
            do {
                return try broker.coordinateReplacing(destination) { coordinatedURL in
                    host.saveCut(
                        target: cut.target,
                        expectedRevision: cut.revision,
                        pathUTF8: Array(coordinatedURL.path.utf8)
                    ).wait(timeout: 120) ?? .failed(.cancelled)
                }
            } catch {
                return .failed(.coreOperation(.ioError))
            }
        }.value
        isFileOperationActive = false
        switch outcome {
        case let .cut(saved):
            self.cut = saved
            cutURL = destination
            lastCommandResult = .started
            return true
        case .noOp:
            cutURL = destination
            lastCommandResult = .noOp
            return true
        case .failed(.staleTarget):
            lastCommandResult = .stale
        case .failed(.cancelled), .failed(.coreOperation(.cancelled)):
            lastCommandResult = .cancelled
        case let .failed(failure):
            lastCommandResult = .failed(failure)
        default:
            lastCommandResult = .invalid
        }
        return false
    }

    func tryOpenCut(
        _ url: URL,
        recovery: Bool,
        memberBaseURL: URL? = nil
    ) async -> Bool {
        let broker = application.fileAccessBroker
        let host = application.coreHost
        isFileOperationActive = true
        let outcome = await Task.detached {
            do {
                return try broker.coordinateReading(url) { coordinatedURL in
                    let path = Array(coordinatedURL.path.utf8)
                    let task = recovery
                        ? host.openCutRecovery(pathUTF8: path)
                        : host.openCut(pathUTF8: path)
                    return task.wait(timeout: 120) ?? .failed(.cancelled)
                }
            } catch {
                return .failed(.coreOperation(.ioError))
            }
        }.value
        isFileOperationActive = false
        guard case let .cut(opened) = outcome else { return false }
        if cut?.isDirty == true, !(await resolveDirtyCutBeforeReplacement()) {
            _ = await host.closeCut(opened.target).value()
            lastCommandResult = .cancelled
            return true
        }
        guard await openCutMembers(
            opened.members,
            relativeTo: memberBaseURL ?? url.deletingLastPathComponent()
        ) else {
            _ = await host.closeCut(opened.target).value()
            return true
        }
        if let old = cut { _ = await host.closeCut(old.target).value() }
        cut = opened
        cutURL = recovery ? nil : url
        selectedCutMemberIndex = opened.members.isEmpty ? nil : 0
        if let first = opened.members.first { activateCutMember(first) }
        lastCommandResult = .started
        return true
    }

    func openCutMembers(
        _ members: [CoreCutMember],
        relativeTo directory: URL
    ) async -> Bool {
        guard !members.isEmpty, var graph = editorGraph else {
            lastCommandResult = .invalid
            return false
        }
        let directory = directory.standardizedFileURL
        let memberURLs = members.map {
            directory.appending(path: $0.relativePath).standardizedFileURL
        }
        guard memberURLs.allSatisfy({
            $0.deletingLastPathComponent() == directory
                && FileManager.default.fileExists(atPath: $0.path)
        }) else {
            lastCommandResult = .failed(.coreOperation(.ioError))
            return false
        }

        var sessions: [CoreSessionProjection] = []
        var newSessions: [(CoreSessionProjection, URL, FileIdentityReservation)] = []
        func rollback() async {
            for (session, _, reservation) in newSessions {
                application.fileIdentityRegistry.cancel(reservation)
                _ = await application.coreHost.closeSession(session.target).value()
            }
        }

        for (member, url) in zip(members, memberURLs) {
            let identity = FileIdentity.resolve(url)
            if let owner = application.fileIdentityRegistry.owner(of: identity) {
                guard let session = sessionProjections[owner],
                      session.documentUUID == member.documentUUID,
                      session.cellID == member.cellID
                else {
                    application.focusSession(owner)
                    await rollback()
                    lastCommandResult = .stale
                    return false
                }
                sessions.append(session)
                continue
            }
            let createdOutcome = await application.coreHost.createSession(
                documentUUID: WorkspaceID().coreDocumentUUID
            ).value()
            guard case let .created(created) = createdOutcome,
                  let reservation = application.fileIdentityRegistry.reserve(
                      identity,
                      for: created.target
                  )
            else {
                await rollback()
                handleM9Failure(createdOutcome)
                return false
            }
            let openedOutcome = await coordinatedM9Open(
                target: created.target,
                revision: created.documentRevision,
                source: url
            )
            guard case let .fileCompleted(file) = openedOutcome,
                  file.operation == .open,
                  file.session.documentUUID == member.documentUUID,
                  file.session.cellID == member.cellID
            else {
                application.fileIdentityRegistry.cancel(reservation)
                _ = await application.coreHost.closeSession(created.target).value()
                await rollback()
                handleM9Failure(openedOutcome)
                return false
            }
            newSessions.append((file.session, url, reservation))
            sessions.append(file.session)
        }

        let records = newSessions.map { session, url, _ in
            WorkspaceViewRecord(
                id: WorkspaceViewID(rawValue: session.primaryView.id.rawValue),
                coreTarget: session.primaryView,
                session: session.target,
                viewRevision: session.viewRevision,
                title: url.deletingPathExtension().lastPathComponent
            )
        }
        if !records.isEmpty,
           !graph.appendAtomically(records, to: graph.activeGroupID)
        {
            await rollback()
            lastCommandResult = .failed(.sessionLimit)
            return false
        }
        guard let animationStates = await installCutMemberSequences(
            members: members,
            sessions: sessions
        ) else {
            await rollback()
            return false
        }
        var committedSessions: [CoreSessionTarget] = []
        for (session, url, reservation) in newSessions {
            let identity = FileIdentity.resolve(url)
            guard application.fileIdentityRegistry.commit(reservation, as: identity) else {
                for target in committedSessions {
                    application.fileIdentityRegistry.release(session: target)
                }
                await rollback()
                lastCommandResult = .stale
                return false
            }
            committedSessions.append(session.target)
        }
        for (session, url, _) in newSessions {
            let identity = FileIdentity.resolve(url)
            sessionProjections[session.target] = session
            sessionDocumentURLs[session.target] = url
            sessionFileIdentities[session.target] = identity
            application.claimSession(session.target, for: id)
        }
        editorGraph = graph
        for state in animationStates {
            animationProjections[state.session.target] = state
        }
        return true
    }

    func coordinatedM9Open(
        target: CoreSessionTarget,
        revision: UInt64,
        source: URL
    ) async -> CoreRequestOutcome {
        let broker = application.fileAccessBroker
        let host = application.coreHost
        return await Task.detached {
            do {
                return try broker.coordinateReading(source) { coordinatedURL in
                    host.open(
                        target: target,
                        expectedDocumentRevision: revision,
                        pathUTF8: Array(coordinatedURL.path.utf8)
                    ).wait(timeout: 120) ?? .failed(.cancelled)
                }
            } catch {
                return .failed(.coreOperation(.ioError))
            }
        }.value
    }

    func resolveDirtyCutBeforeReplacement() async -> Bool {
        guard cut?.isDirty == true else { return true }
        let alert = NSAlert()
        alert.messageText = "Do you want to save the Cut changes?"
        alert.informativeText = cutURL?.lastPathComponent ?? cut?.metadata.cutName ?? "Untitled Cut"
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Discard Changes")
        alert.addButton(withTitle: "Cancel")
        switch await alertResponse(alert) {
        case .alertFirstButtonReturn:
            return await saveCut(chooseDestination: cutURL == nil)
        case .alertSecondButtonReturn:
            return true
        default:
            return false
        }
    }

    func presentSequenceImportPanel(context: CommandTargetContext) {
        Task {
            let panel = NSOpenPanel()
            panel.allowsMultipleSelection = true
            panel.canChooseDirectories = false
            panel.allowedContentTypes = FileTypeCatalog.rasterContentTypes
            guard await panelResponse(panel) == .OK, !panel.urls.isEmpty else {
                lastCommandResult = .cancelled
                return
            }
            guard let rasters = await readNamedRasters(panel.urls) else { return }
            routeM9Animation(.importSequence(rasters), context: context)
        }
    }

    func presentLightTableRasterPanel(
        reloading itemID: UInt64?,
        context: CommandTargetContext
    ) {
        Task {
            let panel = NSOpenPanel()
            panel.allowsMultipleSelection = false
            panel.canChooseDirectories = false
            panel.allowedContentTypes = FileTypeCatalog.rasterContentTypes
            guard await panelResponse(panel) == .OK, let url = panel.url else {
                lastCommandResult = .cancelled
                return
            }
            guard let raster = await readNamedRasters([url])?.first else { return }
            let uuid = WorkspaceID().coreDocumentUUID
            if let itemID {
                routeM9Animation(.reloadLightTableRaster(
                    itemID: itemID,
                    raster: raster,
                    documentUUID: uuid,
                    sourceRevision: 1
                ), context: context)
            } else {
                routeM9Animation(.addLightTableRaster(
                    raster,
                    documentUUID: uuid,
                    sourceRevision: 1
                ), context: context)
            }
        }
    }

    func readNamedRasters(_ urls: [URL]) async -> [CoreNamedRaster]? {
        let broker = application.fileAccessBroker
        let outcome: Result<[CoreNamedRaster], Error> = await Task.detached {
            Result {
                try urls.map { url in
                    guard case let .raster(format)? = FileTypeCatalog.classify(url) else {
                        throw CocoaError(.fileReadUnsupportedScheme)
                    }
                    return try broker.coordinateReading(url) { coordinatedURL in
                        let values = try coordinatedURL.resourceValues(forKeys: [.fileSizeKey])
                        guard let size = values.fileSize, size > 0,
                              size <= 512 * 1_024 * 1_024
                        else { throw CocoaError(.fileReadTooLarge) }
                        return CoreNamedRaster(
                            name: coordinatedURL.lastPathComponent,
                            format: format,
                            bytes: Array(try Data(
                                contentsOf: coordinatedURL,
                                options: .mappedIfSafe
                            ))
                        )
                    }
                }
            }
        }.value
        switch outcome {
        case let .success(rasters):
            return rasters
        case .failure:
            lastCommandResult = .failed(.coreOperation(.ioError))
            return nil
        }
    }

    func exportSequence(context: CommandTargetContext) {
        let generation = lifecycleGeneration
        observe(
            application.coreHost.performAnimation(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                command: .exportSequence(.png, compositeWhite: false)
            ),
            generation: generation
        ) { [weak self] outcome in
            guard let self else { return }
            guard case let .sequenceExported(items) = outcome else {
                if case let .failed(failure) = outcome {
                    self.lastCommandResult = failure == .staleTarget
                        ? .stale : .failed(failure)
                } else {
                    self.lastCommandResult = .invalid
                }
                return
            }
            Task { await self.chooseSequenceExportDirectory(items) }
        }
    }

    func chooseSequenceExportDirectory(_ items: [CoreSequenceExportItem]) async {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        guard await panelResponse(panel) == .OK, let directory = panel.url else {
            lastCommandResult = .cancelled
            return
        }
        let safe = items.allSatisfy {
            !$0.name.isEmpty && URL(filePath: $0.name).lastPathComponent == $0.name
                && $0.bytes.count <= 512 * 1_024 * 1_024
        }
        guard safe, Set(items.map(\.name)).count == items.count else {
            lastCommandResult = .invalid
            return
        }
        let result: Result<Void, Error> = await Task.detached {
            Result {
                for item in items {
                    try AtomicFileWriter().write(
                        Data(item.bytes),
                        to: directory.appending(path: item.name)
                    )
                }
            }
        }.value
        lastCommandResult = switch result {
        case .success: .started
        case .failure: .failed(.coreOperation(.ioError))
        }
    }

    func stepSequence(
        _ direction: CoreSequenceDirection,
        context issuedContext: CommandTargetContext
    ) {
        Task {
            guard matches(issuedContext) else {
                lastCommandResult = .stale
                return
            }
            if sessionProjections[issuedContext.session]?.isDirty == true {
                guard projection?.target == issuedContext.session else {
                    lastCommandResult = .stale
                    return
                }
                guard await save(chooseDestination: documentURL == nil) else {
                    lastCommandResult = .cancelled
                    return
                }
            }
            if cut != nil, !(await synchronizeCutMemberSequences()) {
                return
            }
            guard let session = sessionProjections[issuedContext.session],
                  let view = editorGraph?.allViews.first(where: {
                      $0.coreTarget == issuedContext.view
                          && $0.session == issuedContext.session
                  })
            else {
                lastCommandResult = .stale
                return
            }
            let current = CommandTargetContext(
                workspaceID: id,
                lifecycleGeneration: lifecycleGeneration,
                session: issuedContext.session,
                view: issuedContext.view,
                documentRevision: session.documentRevision,
                viewRevision: view.viewRevision
            )
            let outcome = await application.coreHost.performAnimation(
                target: current.session,
                expectedDocumentRevision: current.documentRevision,
                command: .resolveStep(direction, endpointPolicy)
            ).value()
            guard case let .sequenceStepPlan(plan) = outcome else {
                handleM9Failure(outcome)
                return
            }
            switch plan.result {
            case .empty, .singleCell, .stopped:
                lastCommandResult = .noOp
            case .advanced, .wrapped:
                if let targetUUID = plan.targetDocumentUUID,
                   let targetSession = sessionProjections.values.first(where: {
                       $0.documentUUID == targetUUID
                   })
                {
                    let validation = await application.coreHost.performAnimation(
                        target: current.session,
                        expectedDocumentRevision: current.documentRevision,
                        command: .resolveStep(direction, plan.endpointPolicy)
                    ).value()
                    guard case let .sequenceStepPlan(validatedPlan) = validation,
                          validatedPlan == plan
                    else {
                        lastCommandResult = .stale
                        return
                    }
                    guard targetSession.documentRevision == plan.targetGeneration,
                          let member = cut?.members.first(where: {
                              $0.documentUUID == targetUUID
                                  && $0.cellID == targetSession.cellID
                          })
                    else {
                        lastCommandResult = .stale
                        return
                    }
                    activateCutMember(member)
                    return
                }
                let commit = await application.coreHost.performAnimation(
                    target: current.session,
                    expectedDocumentRevision: current.documentRevision,
                    command: .commitStep(plan)
                ).value()
                handleAnimationOutcome(commit)
            }
        }
    }

    func startMotion(context: CommandTargetContext) {
        motionTask?.cancel()
        Task {
            let outcome = await application.coreHost.performAnimation(
                target: context.session,
                expectedDocumentRevision: context.documentRevision,
                command: .motionStart(
                    fps: motionFPS,
                    loop: motionLoops,
                    includeSelection: true,
                    includeLightTable: true
                )
            ).value()
            guard case .motion = outcome else {
                handleM9Failure(outcome)
                return
            }
            handleAnimationOutcome(outcome)
            let target = context.session
            let revision = context.documentRevision
            let interval = UInt64(1_000_000_000 / max(motionFPS, 1))
            motionTask = Task { @MainActor [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(nanoseconds: interval)
                    guard let self, !Task.isCancelled,
                          self.animation?.motion?.isPaused != true
                    else { continue }
                    let next = await self.application.coreHost.performAnimation(
                        target: target,
                        expectedDocumentRevision: revision,
                        command: .motionStep(.next)
                    ).value()
                    guard case .motion = next else {
                        self.handleM9Failure(next)
                        self.motionTask = nil
                        return
                    }
                    self.handleAnimationOutcome(next)
                }
            }
        }
    }

    func stopMotion() {
        motionTask?.cancel()
        motionTask = nil
        routeM9Animation(.motionStop)
    }

    func handleAnimationOutcome(_ outcome: CoreRequestOutcome) {
        switch outcome {
        case let .animation(state):
            installAnimation(state)
            lastCommandResult = .started
        case let .animationMutation(value):
            installAnimation(value.state)
            lastCommandResult = value.applied ? .started : .noOp
            refreshTree(); refreshPaint(); refreshM8(); refreshHistory(rebuildVisualization: true)
        case .motion:
            lastCommandResult = .started
            refreshAnimation()
        case let .animationSample(color):
            chooseColor(color)
            lastCommandResult = .started
        default:
            handleM9Failure(outcome)
        }
    }

    func handleM9Failure(_ outcome: CoreRequestOutcome) {
        switch outcome {
        case .failed(.staleTarget): lastCommandResult = .stale
        case .failed(.cancelled), .failed(.coreOperation(.cancelled)):
            lastCommandResult = .cancelled
        case let .failed(failure): lastCommandResult = .failed(failure)
        case .noOp: lastCommandResult = .noOp
        default: lastCommandResult = .invalid
        }
    }
}
