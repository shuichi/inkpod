import Foundation

public struct CoreRequestID: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public struct CoreSessionID: RawRepresentable, Hashable, Comparable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }

    public static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

public struct CoreSessionGeneration: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public struct CoreViewID: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public struct CoreViewGeneration: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public struct CoreSurfaceID: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public struct CoreSurfaceGeneration: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public struct CoreDocumentUUID: Hashable, Sendable {
    public let high: UInt64
    public let low: UInt64

    public init(high: UInt64, low: UInt64) {
        self.high = high
        self.low = low
    }

    var isValid: Bool {
        high != 0 || low != 0
    }
}

public struct CoreSessionTarget: Equatable, Hashable, Sendable {
    public let id: CoreSessionID
    public let generation: CoreSessionGeneration

    public init(id: CoreSessionID, generation: CoreSessionGeneration) {
        self.id = id
        self.generation = generation
    }
}

public struct CoreViewTarget: Equatable, Hashable, Sendable {
    public let session: CoreSessionTarget
    public let id: CoreViewID
    public let generation: CoreViewGeneration

    public init(
        session: CoreSessionTarget,
        id: CoreViewID,
        generation: CoreViewGeneration
    ) {
        self.session = session
        self.id = id
        self.generation = generation
    }
}

public struct CoreSurfaceTarget: Equatable, Hashable, Sendable {
    public let id: CoreSurfaceID
    public let generation: CoreSurfaceGeneration

    public init(id: CoreSurfaceID, generation: CoreSurfaceGeneration) {
        self.id = id
        self.generation = generation
    }
}

public struct CoreSnapshotRoute: Equatable, Hashable, Sendable {
    public let session: CoreSessionTarget
    public let view: CoreViewTarget
    public let surface: CoreSurfaceTarget

    public init(
        session: CoreSessionTarget,
        view: CoreViewTarget,
        surface: CoreSurfaceTarget
    ) {
        self.session = session
        self.view = view
        self.surface = surface
    }
}

public struct CorePointerSample: Equatable, Sendable {
    public let deviceX: Float
    public let deviceY: Float
    public let pressure: Float
    public let tiltX: Float
    public let tiltY: Float

    public init(
        deviceX: Float,
        deviceY: Float,
        pressure: Float,
        tiltX: Float = 0,
        tiltY: Float = 0
    ) {
        self.deviceX = deviceX
        self.deviceY = deviceY
        self.pressure = pressure
        self.tiltX = tiltX
        self.tiltY = tiltY
    }

    var isValid: Bool {
        deviceX.isFinite && deviceY.isFinite && pressure.isFinite
            && tiltX.isFinite && tiltY.isFinite && (0 ... 1).contains(pressure)
    }
}

public enum CoreViewCommand: Equatable, Sendable {
    case viewportResized(width: Double, height: Double)
    case panBy(deviceDX: Double, deviceDY: Double)
    case zoomAt(factor: Double, deviceX: Double, deviceY: Double)
    case fit(viewportWidth: Double, viewportHeight: Double)
    case oneToOne(viewportWidth: Double, viewportHeight: Double)
    case boxZoom(documentX: Int32, documentY: Int32, width: Int32, height: Int32)
    case flipHorizontal
    case flipVertical
    case setRulerVisible(Bool)
    case setGuidesVisible(Bool)
    case setGridVisible(Bool)
    case setGuideSnapEnabled(Bool)
    case setGridSnapEnabled(Bool)
    case setTransparentVisible(Bool)
    case setAlphaVisible(Bool)
    case setVectorAntialias(Bool)
    case setVectorCenterlineMode(UInt32)
    case setVectorEndpointsVisible(Bool)

    var isValid: Bool {
        switch self {
        case let .viewportResized(width, height):
            width.isFinite && height.isFinite && width > 0 && height > 0
        case let .panBy(deviceDX, deviceDY):
            deviceDX.isFinite && deviceDY.isFinite
        case let .zoomAt(factor, deviceX, deviceY):
            factor.isFinite && factor > 0 && deviceX.isFinite && deviceY.isFinite
        case let .fit(width, height), let .oneToOne(width, height):
            width.isFinite && height.isFinite && width > 0 && height > 0
        case let .boxZoom(_, _, width, height):
            width > 0 && height > 0
        case .flipHorizontal, .flipVertical, .setRulerVisible, .setGuidesVisible,
             .setGridVisible, .setGuideSnapEnabled, .setGridSnapEnabled,
             .setTransparentVisible, .setAlphaVisible, .setVectorAntialias,
             .setVectorEndpointsVisible:
            true
        case let .setVectorCenterlineMode(mode):
            mode <= 2
        }
    }
}

public struct CoreCommandExpectation: Equatable, Sendable {
    public let documentRevision: UInt64
    public let viewRevision: UInt64

    public init(documentRevision: UInt64, viewRevision: UInt64) {
        self.documentRevision = documentRevision
        self.viewRevision = viewRevision
    }
}

public enum CoreGuideAxis: UInt32, Equatable, Sendable {
    case horizontal = 1
    case vertical = 2
}

public struct CoreGridDefinition: Equatable, Sendable {
    public let originX: Int32
    public let originY: Int32
    public let spacingX: UInt32
    public let spacingY: UInt32
    public let subdivisions: UInt32

    public init(
        originX: Int32,
        originY: Int32,
        spacingX: UInt32,
        spacingY: UInt32,
        subdivisions: UInt32
    ) {
        self.originX = originX
        self.originY = originY
        self.spacingX = spacingX
        self.spacingY = spacingY
        self.subdivisions = subdivisions
    }

    var isValid: Bool {
        spacingX > 0 && spacingY > 0 && subdivisions > 0
    }
}

public enum CoreDocumentCommand: Equatable, Sendable {
    case addGuide(axis: CoreGuideAxis, position: Int32)
    case moveGuide(id: UInt64, position: Int32)
    case deleteAllGuides
    case setGrid(CoreGridDefinition)

    var isValid: Bool {
        switch self {
        case .addGuide, .deleteAllGuides:
            true
        case let .moveGuide(id, _):
            id != 0
        case let .setGrid(grid):
            grid.isValid
        }
    }
}

public enum CoreCommonRasterFormat: UInt32, CaseIterable, Equatable, Sendable {
    case png = 1
    case tiff = 2
    case tga = 3
    case bmp = 4
}

public enum CoreFileOperation: Equatable, Sendable {
    case save
    case open
    case autosave
    case openRecovery
    case revert
    case revertPartial
    case importRaster
    case compactedCopy
}

public struct CoreCompactionToken: Equatable, Sendable {
    public let historyEventCount: UInt64
    public let historyProcedureCount: UInt64
    public let documentDigest: [UInt8]
    public let editorDigest: [UInt8]
    public let journalDigest: [UInt8]

    public init(
        historyEventCount: UInt64,
        historyProcedureCount: UInt64,
        documentDigest: [UInt8],
        editorDigest: [UInt8],
        journalDigest: [UInt8]
    ) {
        self.historyEventCount = historyEventCount
        self.historyProcedureCount = historyProcedureCount
        self.documentDigest = documentDigest
        self.editorDigest = editorDigest
        self.journalDigest = journalDigest
    }

    var isValid: Bool {
        documentDigest.count == 32 && editorDigest.count == 32 && journalDigest.count == 32
    }
}

public struct CoreClipboardID: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public struct CoreClipboardRaster: Equatable, Sendable {
    public let originX: Int32
    public let originY: Int32
    public let width: UInt32
    public let height: UInt32
    public let rowStrideBytes: UInt64
    public let rgba8: [UInt8]

    public init(
        originX: Int32,
        originY: Int32,
        width: UInt32,
        height: UInt32,
        rowStrideBytes: UInt64,
        rgba8: [UInt8]
    ) {
        self.originX = originX
        self.originY = originY
        self.width = width
        self.height = height
        self.rowStrideBytes = rowStrideBytes
        self.rgba8 = rgba8
    }

    var isValid: Bool {
        let required = rowStrideBytes.multipliedReportingOverflow(by: UInt64(height))
        guard width > 0, height > 0,
              rowStrideBytes >= UInt64(width) * 4,
              !required.overflow,
              required.partialValue <= UInt64(Int.max)
        else {
            return false
        }
        return UInt64(rgba8.count) == required.partialValue
    }
}

public struct CoreNewPlanePaste: Equatable, Sendable {
    public let name: String
    public let opacityMilli: UInt32

    public init(name: String, opacityMilli: UInt32 = 1_000) {
        self.name = name
        self.opacityMilli = opacityMilli
    }

    var isValid: Bool {
        !name.utf8.isEmpty && name.utf8.count <= 4_096 && opacityMilli <= 1_000
    }
}

public enum CorePasteMode: Equatable, Sendable {
    case compatible
    case activePlaneConverted
    case newRasterPlane(CoreNewPlanePaste)

    var isValid: Bool {
        switch self {
        case .compatible, .activePlaneConverted:
            true
        case let .newRasterPlane(input):
            input.isValid
        }
    }
}

public struct CoreSessionProjection: Equatable, Sendable {
    public let target: CoreSessionTarget
    public let primaryView: CoreViewTarget
    public let documentUUID: CoreDocumentUUID
    public let cellID: UInt64
    public let documentRevision: UInt64
    public let viewRevision: UInt64
    public let abiVersion: UInt32
    public let replayEpoch: UInt32
    public let procedureFormatVersion: UInt32
    public let ownerThreadID: UInt64
    public let hasActiveTransient: Bool
    public let canUndo: Bool
    public let canRedo: Bool
    public let isDirty: Bool
    public let isRecovered: Bool
    public let documentWidth: UInt32
    public let documentHeight: UInt32
    public let dpiXMilli: UInt32
    public let dpiYMilli: UInt32
    public let paperFrames: CorePaperFrames
}

public struct CoreSessionCloseProjection: Equatable, Sendable {
    public let target: CoreSessionTarget
    public let ownerThreadID: UInt64
    public let cancelledActiveTransient: Bool
}

public struct CoreDocumentCommandProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let affectedGuideID: UInt64?
}

public struct CoreFileProjection: Equatable, Sendable {
    public let operation: CoreFileOperation
    public let session: CoreSessionProjection

    public init(operation: CoreFileOperation, session: CoreSessionProjection) {
        self.operation = operation
        self.session = session
    }
}

public struct CoreRasterExport: Equatable, Sendable {
    public let format: CoreCommonRasterFormat
    public let bytes: [UInt8]

    public init(format: CoreCommonRasterFormat, bytes: [UInt8]) {
        self.format = format
        self.bytes = bytes
    }
}

public struct CoreClipboardProjection: Equatable, Sendable {
    public let id: CoreClipboardID
    public let raster: CoreClipboardRaster
    public let session: CoreSessionProjection?

    public init(
        id: CoreClipboardID,
        raster: CoreClipboardRaster,
        session: CoreSessionProjection?
    ) {
        self.id = id
        self.raster = raster
        self.session = session
    }
}

public struct CoreShutdownProjection: Equatable, Sendable {
    public let ownerThreadID: UInt64
    public let destroyedSessionIDs: [CoreSessionID]
    public let cancelledRequestCount: Int
}

public enum CoreHostFailure: Equatable, Sendable {
    case invalidRequest
    case invalidTarget
    case staleTarget
    case sessionLimit
    case queueFull
    case allocationFailed
    case cancelled
    case hostStopped
    case identityOverflow
    case coreCreate(CoreStatus)
    case coreOperation(CoreStatus)
}

public enum CoreRequestOutcome: Sendable {
    case created(CoreSessionProjection)
    case cellPlan(CoreCellCreationPlanProjection)
    case cellsCreated([CoreSessionProjection])
    case inspected(CoreSessionProjection)
    case closed(CoreSessionCloseProjection)
    case shutdown(CoreShutdownProjection)
    case viewUpdated(CoreSessionProjection)
    case viewCreated(CoreLogicalViewProjection)
    case logicalViewUpdated(CoreLogicalViewProjection)
    case viewClosed(CoreViewTarget)
    case documentUpdated(CoreSessionProjection)
    case cellUpdated(CoreSessionProjection)
    case tree(CoreTreeProjection)
    case treeUpdated(CoreTreeMutationProjection)
    case paint(CorePaintProjection)
    case paintUpdated(CorePaintProjection)
    case fillApplied(CoreFillProjection)
    case eyedropperSampled(CorePaintProjection)
    case locator(CoreLocatorProjection)
    case colorChartPreview(CoreColorChartPreviewProjection)
    case colorReplacePreview(CoreColorReplacePreviewProjection)
    case outputColorGuardApplied(CoreOutputColorGuardProjection)
    case documentCommandUpdated(CoreDocumentCommandProjection)
    case snapshot(CoreSnapshotEnvelope)
    case fileCompleted(CoreFileProjection)
    case rasterExported(CoreRasterExport)
    case compactionPlanned(CoreCompactionToken)
    case clipboardCopied(CoreClipboardProjection)
    case pasteStarted(CoreSessionProjection)
    case floatingTransformed(CoreSessionProjection)
    case pasteCancelled(CoreSessionProjection)
    case history(CoreHistoryProjection)
    case historyVisualizationProgress(CoreHistoryVisualizationProgressProjection)
    case historyVisualizationRows([CoreHistoryVisualizationRow])
    case m8State(CoreM8Projection)
    case m8Mutation(CoreM8MutationProjection)
    case cut(CoreCutProjection)
    case cutMutation(CoreCutMutationProjection)
    case animation(CoreAnimationProjection)
    case animationMutation(CoreAnimationMutationProjection)
    case sequenceStepPlan(CoreSequenceStepPlan)
    case sequenceExported([CoreSequenceExportItem])
    case lightTableBulkPreview(CoreLightTableBulkPreview)
    case motion(CoreMotionProjection)
    case animationSample(CoreColorValue)
    case batchGraph(CoreBatchGraphSummary)
    case batchPreview(CoreBatchPreviewProjection)
    case batchReport(CoreBatchReportProjection)
    case batchPairPreview(CoreBatchPairPreviewProjection)
    case filterPreview(CoreFilterPreviewProjection)
    case geometryPreview(CoreGeometryPreviewProjection)
    case vectorSelection(CoreVectorSelectionProjection)
    case documentPoints([CoreDocumentPoint])
    case noOp(CoreSessionProjection?)
    case acknowledged
    case failed(CoreHostFailure)
}

extension CoreRequestOutcome: Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool {
        switch (lhs, rhs) {
        case let (.created(left), .created(right)),
             let (.inspected(left), .inspected(right)),
             let (.viewUpdated(left), .viewUpdated(right)),
             let (.documentUpdated(left), .documentUpdated(right)):
            left == right
        case let (.cellPlan(left), .cellPlan(right)):
            left == right
        case let (.cellsCreated(left), .cellsCreated(right)):
            left == right
        case let (.viewCreated(left), .viewCreated(right)),
             let (.logicalViewUpdated(left), .logicalViewUpdated(right)):
            left == right
        case let (.viewClosed(left), .viewClosed(right)):
            left == right
        case let (.cellUpdated(left), .cellUpdated(right)):
            left == right
        case let (.tree(left), .tree(right)):
            left == right
        case let (.treeUpdated(left), .treeUpdated(right)):
            left == right
        case let (.paint(left), .paint(right)):
            left == right
        case let (.paintUpdated(left), .paintUpdated(right)):
            left == right
        case let (.fillApplied(left), .fillApplied(right)):
            left == right
        case let (.eyedropperSampled(left), .eyedropperSampled(right)):
            left == right
        case let (.locator(left), .locator(right)):
            left == right
        case let (.colorChartPreview(left), .colorChartPreview(right)):
            left == right
        case let (.colorReplacePreview(left), .colorReplacePreview(right)):
            left == right
        case let (.outputColorGuardApplied(left), .outputColorGuardApplied(right)):
            left == right
        case let (.closed(left), .closed(right)):
            left == right
        case let (.documentCommandUpdated(left), .documentCommandUpdated(right)):
            left == right
        case let (.shutdown(left), .shutdown(right)):
            left == right
        case let (.snapshot(left), .snapshot(right)):
            left == right
        case let (.fileCompleted(left), .fileCompleted(right)):
            left == right
        case let (.rasterExported(left), .rasterExported(right)):
            left == right
        case let (.compactionPlanned(left), .compactionPlanned(right)):
            left == right
        case let (.clipboardCopied(left), .clipboardCopied(right)):
            left == right
        case let (.pasteStarted(left), .pasteStarted(right)):
            left == right
        case let (.floatingTransformed(left), .floatingTransformed(right)):
            left == right
        case let (.pasteCancelled(left), .pasteCancelled(right)):
            left == right
        case let (.history(left), .history(right)):
            left == right
        case let (.historyVisualizationProgress(left), .historyVisualizationProgress(right)):
            left == right
        case let (.historyVisualizationRows(left), .historyVisualizationRows(right)):
            left == right
        case let (.m8State(left), .m8State(right)):
            left == right
        case let (.m8Mutation(left), .m8Mutation(right)):
            left == right
        case let (.cut(left), .cut(right)):
            left == right
        case let (.cutMutation(left), .cutMutation(right)):
            left == right
        case let (.animation(left), .animation(right)):
            left == right
        case let (.animationMutation(left), .animationMutation(right)):
            left == right
        case let (.sequenceStepPlan(left), .sequenceStepPlan(right)):
            left == right
        case let (.sequenceExported(left), .sequenceExported(right)):
            left == right
        case let (.lightTableBulkPreview(left), .lightTableBulkPreview(right)):
            left == right
        case let (.motion(left), .motion(right)):
            left == right
        case let (.animationSample(left), .animationSample(right)):
            left == right
        case let (.batchGraph(left), .batchGraph(right)):
            left == right
        case let (.batchPreview(left), .batchPreview(right)):
            left == right
        case let (.batchReport(left), .batchReport(right)):
            left == right
        case let (.batchPairPreview(left), .batchPairPreview(right)):
            left == right
        case let (.filterPreview(left), .filterPreview(right)):
            left == right
        case let (.geometryPreview(left), .geometryPreview(right)):
            left == right
        case let (.vectorSelection(left), .vectorSelection(right)):
            left == right
        case let (.documentPoints(left), .documentPoints(right)):
            left == right
        case let (.noOp(left), .noOp(right)):
            left == right
        case (.acknowledged, .acknowledged):
            true
        case let (.failed(left), .failed(right)):
            left == right
        default:
            false
        }
    }
}

enum CoreRequest: Sendable {
    case createSession(CoreDocumentUUID)
    case prepareCellCreation(CoreCellCreationOptions)
    case commitCellCreation(CoreCellCreationPlanID, [CoreDocumentUUID])
    case cancelCellCreation(CoreCellCreationPlanID)
    case inspectSession(CoreSessionTarget)
    case closeSession(CoreSessionTarget)
    case createCut(CoreCutUUID, CoreCutMetadata, CoreCutDefaults, [CoreCutMember])
    case inspectCut(CoreCutTarget)
    case closeCut(CoreCutTarget)
    case openCut([UInt8], Bool)
    case updateCut(CoreCutTarget, UInt64, CoreCutMetadata, CoreCutDefaults)
    case cancelCutUpdate(CoreCutTarget)
    case editCutSequence(CoreCutTarget, UInt64, [CoreCutSequenceOperation])
    case cancelCutSequence(CoreCutTarget)
    case undoCut(CoreCutTarget, UInt64)
    case redoCut(CoreCutTarget, UInt64)
    case saveCut(CoreCutTarget, UInt64, [UInt8], Bool)
    case createView(CoreSessionTarget, UInt64)
    case closeView(CoreViewTarget)
    case applyView(CoreViewTarget, CoreViewCommand, CoreCommandExpectation?)
    case resolveDocumentPoints(CoreViewTarget, UInt64, UInt64, [CorePointerSample])
    case applyDocument(CoreSessionTarget, CoreDocumentCommand, UInt64)
    case editCell(CoreSessionTarget, UInt64, CoreCellEditCommand)
    case inspectTree(CoreSessionTarget, UInt64?)
    case setActiveNode(CoreSessionTarget, UInt64, UInt64, UInt64)
    case editTree(CoreSessionTarget, UInt64, CoreTreeEditCommand)
    case inspectPaint(CoreSessionTarget, UInt64?)
    case updateEditor(CoreViewTarget, CorePaintExpectation, CoreEditorUpdate)
    case beginRasterStroke(CoreViewTarget, CorePaintExpectation?, [CorePointerSample])
    case appendRasterStroke(CoreViewTarget, [CorePointerSample])
    case endStroke(CoreViewTarget)
    case cancelStroke(CoreViewTarget)
    case applyFill(CoreViewTarget, CorePaintExpectation, CoreFillGesture)
    case eyedropper(
        CoreViewTarget,
        CorePaintExpectation,
        CoreEyedropperSource,
        CorePointerSample
    )
    case replacePalette(CoreSessionTarget, UInt64, [CoreColorValue])
    case generatePalette(CoreSessionTarget, UInt64, UInt32, UInt32)
    case savePaletteFile(CoreSessionTarget, UInt64, [UInt8])
    case loadPaletteFile(CoreSessionTarget, UInt64, [UInt8])
    case replaceColorChart(CoreSessionTarget, UInt64, [CoreColorChartEntry], Bool)
    case saveColorChartFile(CoreSessionTarget, UInt64, [UInt8])
    case loadColorChartFile(CoreSessionTarget, UInt64, [UInt8])
    case createColorChartPreview(CoreSessionTarget, UInt64, UInt32, UInt32)
    case applyColorChartPreview(CoreSessionTarget, UInt64, CoreColorChartPreviewID)
    case cancelColorChartPreview(CoreColorChartPreviewID)
    case setColorCheck(CoreViewTarget, UInt64, CoreColorCheckMode)
    case inspectLocator(CoreViewTarget, UInt64, CorePointerSample, UInt32)
    case paintLocatorPixel(CoreViewTarget, CorePaintExpectation, Int32, Int32)
    case previewColorReplace(
        CoreViewTarget,
        CorePaintExpectation,
        CoreColorReplaceRequest
    )
    case applyColorReplace(
        CoreViewTarget,
        CorePaintExpectation,
        CoreColorReplaceRequest
    )
    case selectOutputColorGuard(CoreSessionTarget, UInt64, CoreSelectionOperation)
    case applySelection(CoreViewTarget, CorePaintExpectation, [CorePointerSample])
    case selectionAdjust(
        CoreSessionTarget,
        UInt64,
        CoreSelectionAdjustOperation,
        UInt32
    )
    case clearSelection(CoreSessionTarget, UInt64)
    case selectColor(
        CoreViewTarget,
        CorePaintExpectation,
        Bool,
        CoreSelectionOperation
    )
    case selectionToLayer(CoreSessionTarget, UInt64, [UInt8])
    case selectionFromLayer(
        CoreSessionTarget,
        UInt64,
        UInt64,
        CoreSelectionLayerOperation
    )
    case undo(CoreSessionTarget, UInt64?)
    case redo(CoreSessionTarget, UInt64?)
    case inspectHistory(CoreSessionTarget, UInt64?)
    case jumpHistory(CoreSessionTarget, UInt64, UInt64)
    case buildSnapshot(CoreSnapshotRoute)
    case save(CoreSessionTarget, UInt64, [UInt8], [UInt8]?, Bool)
    case open(CoreSessionTarget, UInt64, [UInt8])
    case autosave(CoreSessionTarget, UInt64, [UInt8])
    case openRecovery(CoreSessionTarget, UInt64, [UInt8])
    case revert(CoreSessionTarget, UInt64)
    case revertPartial(CoreSessionTarget, UInt64)
    case importCommonRaster(
        CoreSessionTarget,
        UInt64,
        CoreCommonRasterFormat,
        [UInt8],
        CoreDocumentUUID
    )
    case exportCommonRaster(CoreSessionTarget, UInt64, CoreCommonRasterFormat, Bool)
    case compactionPlan(CoreSessionTarget, UInt64)
    case writeCompactedCopy(CoreSessionTarget, UInt64, [UInt8], CoreCompactionToken)
    case copyClipboard(CoreSessionTarget, UInt64, Bool)
    case createClipboard(CoreClipboardRaster)
    case releaseClipboard(CoreClipboardID)
    case beginPaste(CoreSessionTarget, UInt64, CoreClipboardID, CorePasteMode)
    case transformFloatingPaste(CoreSessionTarget, UInt64, CoreFloatingTransform)
    case commitPaste(CoreSessionTarget, UInt64)
    case cancelPaste(CoreSessionTarget, UInt64)
    case beginHistoryVisualization(CoreSessionTarget, UInt64)
    case stepHistoryVisualization(CoreHistoryVisualizationID, UInt32)
    case historyVisualizationRows(CoreHistoryVisualizationID, Range<UInt64>)
    case releaseHistoryVisualization(CoreHistoryVisualizationID)
    case inspectM8(CoreSessionTarget, UInt64)
    case performM8(CoreSessionTarget, UInt64, CoreM8Command)
    case inspectAnimation(CoreSessionTarget, UInt64)
    case performAnimation(CoreSessionTarget, UInt64, CoreAnimationCommand)
    case exportInstructionRaster(
        CoreSessionTarget,
        UInt64,
        CoreCommonRasterFormat,
        Bool
    )
    case previewBatch(CoreSessionTarget, UInt64, CoreBatchGraphDraft, CoreBatchRunScope)
    case executeBatch(CoreSessionTarget, UInt64, CoreBatchGraphDraft, CoreBatchRunOptions)
    case saveBatchGraph(CoreBatchGraphDraft, [UInt8])
    case inspectBatchGraph([UInt8])
    case previewSavedBatch(
        CoreSessionTarget, UInt64, [UInt8], [CoreBatchOperation], CoreBatchRunScope
    )
    case executeSavedBatch(
        CoreSessionTarget, UInt64, [UInt8], [CoreBatchOperation], CoreBatchRunOptions
    )
    case extractBatchPairs(CoreSessionTarget, UInt64, UInt32, UInt32)
    case selectAll(CoreSessionTarget, UInt64)
    case cancel(CoreRequestID)
    case shutdown
    case beginTransientForTesting(CoreSessionTarget)
    case setNormalProcessingEnabledForTesting(Bool)
}

enum CoreRequestLane: Sendable {
    case normal
    case inputSample
    case inputBoundary
    case control
}

struct CoreRequestEnvelope: Sendable {
    let requestID: CoreRequestID
    let request: CoreRequest
}

struct CoreHostTestConfiguration: Sendable {
    var createABIMismatchCount: Int
    var normalAdmissionFailureCount: Int

    init(
        createABIMismatchCount: Int = 0,
        normalAdmissionFailureCount: Int = 0
    ) {
        precondition(createABIMismatchCount >= 0)
        precondition(normalAdmissionFailureCount >= 0)
        self.createABIMismatchCount = createABIMismatchCount
        self.normalAdmissionFailureCount = normalAdmissionFailureCount
    }
}
