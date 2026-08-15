import Foundation
import SwiftUI

public enum InkpodCommandID: UInt32, CaseIterable, Codable, Sendable {
    case fileNew = 40_001
    case fileOpen = 40_002
    case fileSave = 40_003
    case fileSaveAs = 40_004
    case fileRevert = 40_005
    case fileAutosaveNow = 40_007
    case fileOpenRecovery = 40_008
    case fileRevertPartial = 40_009
    case fileImportRaster = 40_010
    case fileExportRaster = 40_011
    case fileOpenRecent = 40_012
    case fileRestorePrevious = 40_020
    case fileCompactCopy = 40_021
    case fileSequenceAutosave = 40_022
    case fileNewCut = 40_023
    case cutProperties = 40_024
    case cutSave = 40_025
    case cutUndo = 40_026
    case cutRedo = 40_027
    case cutSequenceAdd = 40_028
    case cutSequenceRemove = 40_029
    case cutSequenceMoveUp = 40_030
    case cutSequenceMoveDown = 40_031
    case cutSequenceRenumber = 40_032
    case fileExportInstructionRaster = 40_033
    case appExit = 40_006
    case undo = 40_101
    case redo = 40_102
    case editCopy = 40_103
    case editPaste = 40_104
    case editMirrorHorizontal = 40_105
    case historyBack = 40_106
    case historyForward = 40_107
    case editCut = 40_108
    case editPasteSelected = 40_109
    case editPasteConverted = 40_110
    case floatingTransform = 40_111
    case floatingCommit = 40_112
    case floatingCancel = 40_113
    case zoomIn = 40_201
    case zoomOut = 40_202
    case fit = 40_203
    case oneToOne = 40_204
    case flipHorizontal = 40_205
    case flipVertical = 40_206
    case grid = 40_207
    case zoomPercent = 40_209
    case boxZoom = 40_210
    case ruler = 40_211
    case guides = 40_212
    case snapGuides = 40_213
    case snapGrid = 40_214
    case transparent = 40_215
    case guideVertical = 40_216
    case guideHorizontal = 40_217
    case guideDeleteAll = 40_218
    case gridSettings = 40_219
    case guideMove = 40_220
    case viewVectorAntialias = 40_235
    case viewVectorCenterline = 40_236
    case viewVectorCenterlineOnly = 40_237
    case viewVectorEndpoints = 40_238
    case toolPencil = 40_301
    case toolBrush = 40_302
    case toolEraser = 40_303
    case toolFill = 40_304
    case toolEyedropper = 40_305
    case toolFillOptions = 40_306
    case toolClosedFill = 40_307
    case toolFillExtension = 40_308
    case toolColorReplaceTarget = 40_309
    case toolColorReplacePen = 40_310
    case toolColorReplaceRectangle = 40_311
    case toolColorReplacePolyline = 40_312
    case toolColorReplaceLasso = 40_313
    case toolColorReplaceAll = 40_314
    case colorChoose = 40_501
    case colorCheckOff = 40_502
    case colorCheckLegacy = 40_503
    case colorCheckNative = 40_504
    case colorEditor = 40_505
    case colorSourceTopmost = 40_506
    case colorSourceSelected = 40_507
    case colorSourceComposite = 40_508
    case colorSourceLightTable = 40_509
    case paletteRegister = 40_510
    case paletteDelete = 40_511
    case paletteClear = 40_512
    case paletteSave = 40_513
    case paletteLoad = 40_514
    case paletteNextGroup = 40_515
    case chartGenerate = 40_516
    case chartSearch = 40_517
    case chartNext = 40_518
    case chartLock = 40_519
    case chartCopy = 40_520
    case chartPaste = 40_521
    case chartCut = 40_522
    case chartRename = 40_523
    case chartSave = 40_524
    case chartLoad = 40_525
    case chartNextPage = 40_526
    case helpAbout = 40_601
    case helpManual = 40_602
    case helpFileFormat = 40_603
    case helpWebPage = 40_604
    case helpAcknowledgements = 40_605
    case shortcutReset = 40_901
    case shortcutEdit = 40_902
    case languageSystem = 40_903
    case languageJapanese = 40_904
    case languageEnglish = 40_905
    case documentClose = 40_222
    case viewNew = 40_208
    case viewClose = 40_221
    case tabNext = 40_223
    case tabPrevious = 40_224
    case editorSplitRight = 40_225
    case editorSplitDown = 40_226
    case editorMoveOtherGroup = 40_227
    case editorNewViewOtherGroup = 40_228
    case editorGroupClose = 40_229
    case editorGroupNext = 40_230
    case tabMoveLeft = 40_231
    case tabMoveRight = 40_232
    case viewMoveNextWindow = 40_233
    case viewDuplicateNextWindow = 40_234
    case planeMainLine = 40_401
    case planeColor = 40_402
    case selectionAll = 40_801
    case selectionInvert = 40_802
    case selectionExpand = 40_803
    case selectionShrink = 40_804
    case selectionClear = 40_805
    case selectionRectangle = 40_806
    case selectionEllipse = 40_807
    case selectionLasso = 40_808
    case selectionPolyline = 40_809
    case selectionTrace = 40_810
    case selectionWand = 40_811
    case selectionModeNew = 40_812
    case selectionModeAdd = 40_813
    case selectionModeSubtract = 40_814
    case selectionModeIntersect = 40_815
    case selectionColor = 40_816
    case selectionColorDifferent = 40_817
    case selectionColorAdd = 40_818
    case selectionToLayer = 40_819
    case selectionFromLayer = 40_820
    case selectionLayerAdd = 40_821
    case selectionLayerSubtract = 40_822
    case selectionOptions = 40_823
    case selectionOutputColorGuard = 40_824
    case layerDuplicate = 40_701
    case layerDelete = 40_702
    case layerMoveTop = 40_703
    case cellPaperSettings = 41_301
    case cellFrameHundred = 41_302
    case cellFrameReference = 41_303
    case cellFrameDrawing = 41_304
    case cellFrameSafe = 41_305
    case cellMargins = 41_306
    case cellMirrorVertical = 41_307
    case cellRotateLeft = 41_308
    case cellRotateRight = 41_309
    case cellImageSize = 41_310
    case cellResolution = 41_311
    case cellFitCaptureFrame = 41_312
    case layerNew = 41_401
    case layerMoveUp = 41_402
    case layerMoveDown = 41_403
    case layerToggleVisible = 41_404
    case layerToggleEditable = 41_405
    case layerOpacity = 41_406
    case layerConvert = 41_407
    case layerMerge = 41_408
    case layerDeleteHidden = 41_409
    case layerProperties = 41_410
    case planeNew = 41_501
    case planeDuplicate = 41_502
    case planeDelete = 41_503
    case planeMoveUp = 41_504
    case planeMoveDown = 41_505
    case planeToggleVisible = 41_506
    case planeToggleEditable = 41_507
    case planeOpacity = 41_508
    case planeConvert = 41_509
    case planeMerge = 41_510
    case planeProperties = 41_511
    case windowLayerPalette = 41_925
    case windowToolPalette = 41_900
    case windowToolOptions = 41_926
    case windowColorPane = 41_927
    case windowLocator = 41_932
    case locatorPin = 41_933
    case locatorFixed = 41_934
    case locatorAutoscroll = 41_935
    case colorPin = 41_943
    case workspaceReset = 41_928
    case workspaceSave = 41_929
    case workspaceRestore = 41_930
    case workspaceMirror = 41_931
    case workspacePresetColoring = 41_944
    case workspacePresetLineCleanup = 41_945
    case workspacePresetReference = 41_946
    case workspacePresetBatch = 41_947
    case workspacePresetFocus = 41_948
    case workspaceSaveAs = 41_949
    case workspaceNewWindow = 41_955
    case viewMoveNewWindow = 41_956
    case viewDuplicateNewWindow = 41_957
    case filterLast = 41_001
    case filterInvert = 41_002
    case filterBlurWeak = 41_003
    case filterSharpenWeak = 41_004
    case filterSharpenStrong = 41_005
    case filterBlurStrong = 41_006
    case filterGaussian = 41_007
    case filterAutoContrast = 41_008
    case filterBrightness = 41_009
    case filterToneCurve = 41_010
    case filterLevels = 41_011
    case filterHSV = 41_012
    case filterColorBalance = 41_013
    case filterUnsharp = 41_014
    case effectGradient = 41_101
    case effectAirbrush = 41_102
    case effectBoundaryAirbrush = 41_103
    case effectBlur = 41_104
    case effectStamp = 41_105
    case effectDust = 41_106
    case effectAlphaGradient = 41_107
    case effectAlphaView = 41_108
    case adjustmentCreate = 41_201
    case adjustmentEdit = 41_202
    case adjustmentToggle = 41_203
    case adjustmentMoveTop = 41_204
    case adjustmentPrevious = 41_205
    case adjustmentNext = 41_206
    case cellShootingFrameProperties = 41_313
    case cellShootingFrameEditHandles = 41_314
    case cellShootingFrameDelete = 41_315
    case cellVanishingPointProperties = 41_316
    case cellVanishingPointEditHandles = 41_317
    case cellVanishingPointDeleteAll = 41_318
    case annotationAddText = 41_411
    case annotationEditText = 41_412
    case annotationDrawInstruction = 41_413
    case annotationSelectPrevious = 41_414
    case annotationSelectNext = 41_415
    case annotationMoveLeft = 41_416
    case annotationMoveRight = 41_417
    case annotationDelete = 41_418
    case vectorLine = 41_801
    case vectorCurve = 41_802
    case vectorRectangle = 41_803
    case vectorEllipse = 41_804
    case vectorPolyline = 41_805
    case vectorEraser = 41_806
    case vectorErasePartial = 41_807
    case vectorEraseIntersection = 41_808
    case vectorEraseWhole = 41_809
    case vectorConnect = 41_810
    case vectorWidth = 41_811
    case vectorSelectCut = 41_812
    case vectorSelectTouch = 41_813
    case vectorSelectContained = 41_814
    case vectorSelectLine = 41_815
    case vectorSelectWholeLine = 41_816
    case vectorSelectIntersection = 41_817
    case vectorSelectFillBoundary = 41_818
    case vectorSelectFill = 41_819
    case vectorRasterize = 41_820
    case vectorVectorize = 41_821
    case vectorPolygon = 41_822
    case geometryOptions = 41_823
    case lightTableSetNew = 41_601
    case lightTableSetDuplicate = 41_602
    case lightTableSetDelete = 41_603
    case lightTableSetRename = 41_604
    case lightTableSetUp = 41_605
    case lightTableSetDown = 41_606
    case lightTableGlobalOpacity = 41_607
    case lightTableItemAdd = 41_608
    case lightTableItemReload = 41_609
    case lightTableItemDelete = 41_610
    case lightTableItemUp = 41_611
    case lightTableItemDown = 41_612
    case lightTableItemProperties = 41_613
    case lightTableItemSample = 41_614
    case lightTableItemSwap = 41_615
    case lightTableItemMove = 41_616
    case lightTableBulkPrevious = 41_617
    case lightTableBulkNext = 41_618
    case lightTableBulkBoth = 41_619
    case sequenceImport = 41_701
    case sequenceExport = 41_702
    case sequencePrevious = 41_703
    case sequenceNext = 41_704
    case sequenceGoto = 41_705
    case subpaletteSet = 41_706
    case subpaletteSample = 41_707
    case motionStart = 41_708
    case motionPause = 41_709
    case motionPrevious = 41_710
    case motionNext = 41_711
    case motionStop = 41_712
    case motionFirst = 41_713
    case motionLast = 41_714
    case motionFPS30 = 41_715
    case motionFPS25 = 41_716
    case motionFPS24 = 41_717
    case motionFPS12 = 41_718
    case motionFPS10 = 41_719
    case motionFPS8 = 41_720
    case sequenceWrapEndpoints = 41_721
    case windowSequence = 41_936
    case sequencePin = 41_937
    case windowLightTable = 41_938
    case lightTablePin = 41_939
    case windowSubpalette = 41_940
    case subpalettePin = 41_941
    case windowBatch = 41_901
    case batchInputFile = 41_902
    case batchInputFolder = 41_903
    case batchInputCurrent = 41_904
    case batchOperationRemove = 41_906
    case batchOperationUp = 41_907
    case batchOperationDown = 41_908
    case batchOperationEdit = 41_909
    case batchReplaceSwap = 41_910
    case batchOutputDuplicate = 41_911
    case batchOutputNew = 41_912
    case batchOutputOverwrite = 41_913
    case batchFailureContinue = 41_914
    case batchFailureStop = 41_915
    case batchPreview = 41_916
    case batchDryRun = 41_917
    case batchRunCurrent = 41_918
    case batchRunAll = 41_919
    case batchSaveSet = 41_920
    case batchLoadSet = 41_921
    case batchCancel = 41_922
    case batchInputRange = 41_923
    case batchOutputSettings = 41_924
    case batchPin = 41_942
    case windowJobProgress = 41_958
    case batchAddColorReplace = 42_001
    case batchAddContinuousFill = 42_002
    case batchAddSeparation = 42_003
    case batchAddVisibility = 42_004
    case batchAddLineWidth = 42_005
    case batchAddBoundaryAirbrush = 42_006
    case batchAddDust = 42_007
    case batchAddMirror = 42_008
    case batchAddRotate = 42_009
    case batchAddResize = 42_010
    case batchAddConvert = 42_011
    case batchExtractPairs = 42_012
    case batchAddFilterSharpenWeak = 42_020
    case batchAddFilterSharpenStrong = 42_021
    case batchAddFilterBlurWeak = 42_022
    case batchAddFilterBlurStrong = 42_023
    case batchAddFilterGaussian = 42_024
    case batchAddFilterInvert = 42_025
    case batchAddFilterAutoContrast = 42_026
    case batchAddFilterBrightness = 42_027
    case batchAddFilterToneCurve = 42_028
    case batchAddFilterLevels = 42_029
    case batchAddFilterHSV = 42_030
    case batchAddFilterColorBalance = 42_031
    case batchAddFilterUnsharp = 42_032
}

public enum CommandRouteOwner: String, Sendable {
    case application = "ApplicationCommandRouter"
    case fileLifecycle = "FileLifecycleRouter"
    case edit = "EditCommandRouter"
    case history = "HistoryCommandRouter"
    case session = "SessionCommandRouter"
    case view = "ViewCommandRouter"
    case workspace = "WorkspaceCommandRouter"
    case cell = "CellCommandRouter"
    case tool = "ToolCommandRouter"
    case color = "ColorCommandRouter"
    case paneTarget = "PaneTargetRouter"
    case image = "ImageCommandRouter"
    case cut = "CutCommandRouter"
    case animation = "AnimationCommandRouter"
    case batch = "BatchCommandRouter"
}

public enum CommandStateOwner: String, Sendable {
    case application = "ApplicationStateProvider"
    case fileLifecycle = "FileLifecycleStateProvider"
    case edit = "EditStateProvider"
    case history = "HistoryStateProvider"
    case session = "SessionStateProvider"
    case view = "ViewStateProvider"
    case workspace = "WorkspaceStateProvider"
    case cell = "CellStateProvider"
    case tool = "ToolStateProvider"
    case color = "ColorStateProvider"
    case paneTarget = "PaneTargetStateProvider"
    case image = "ImageStateProvider"
    case cut = "CutStateProvider"
    case animation = "AnimationStateProvider"
    case batch = "BatchStateProvider"
}

public enum CommandSurface: String, Hashable, Sendable {
    case standardMenu
    case fileMenu
    case editMenu
    case viewMenu
    case toolbar
    case contextMenu
    case cellMenu
    case windowMenu
    case inspector
    case tabStrip
    case settings
    case helpMenu
    case toolsMenu
    case colorMenu
    case selectionMenu
    case sidebar
    case imageMenu
    case animationMenu
    case timeline
    case batchWindow
}

public enum CommandTargetScope: String, Sendable {
    case application
    case documentSession
    case documentView
    case workspace
    case pane
    case cutSession
    case job
}

public struct CommandDescriptor: Identifiable, Sendable {
    public let id: InkpodCommandID
    public let localizationKey: String
    public let routeOwner: CommandRouteOwner
    public let stateOwner: CommandStateOwner
    public let targetScope: CommandTargetScope
    public let surfaces: Set<CommandSurface>
    public let parityTestID: String
}

public struct CommandState: Equatable, Sendable {
    public let enabled: Bool
    public let checked: Bool

    public init(enabled: Bool, checked: Bool = false) {
        self.enabled = enabled
        self.checked = checked
    }
}

public struct ViewPresentationState: Equatable, Sendable {
    public var rulerVisible = false
    public var guidesVisible = false
    public var gridVisible = false
    public var guideSnapEnabled = false
    public var gridSnapEnabled = false
    public var transparentVisible = false
    public var flipHorizontal = false
    public var flipVertical = false
    public var vectorAntialias = true
    public var vectorCenterlineMode: UInt32 = 0
    public var vectorEndpointsVisible = false
    public var alphaVisible = false

    public init() {}
}

public struct CommandTargetContext: Equatable, Sendable {
    public let workspaceID: WorkspaceID
    public let lifecycleGeneration: UInt64
    public let session: CoreSessionTarget
    public let view: CoreViewTarget
    public let documentRevision: UInt64
    public let viewRevision: UInt64

    public init(
        workspaceID: WorkspaceID,
        lifecycleGeneration: UInt64,
        session: CoreSessionTarget,
        view: CoreViewTarget,
        documentRevision: UInt64,
        viewRevision: UInt64
    ) {
        self.workspaceID = workspaceID
        self.lifecycleGeneration = lifecycleGeneration
        self.session = session
        self.view = view
        self.documentRevision = documentRevision
        self.viewRevision = viewRevision
    }
}

public enum CommandRouteResult: Equatable, Sendable {
    case started
    case presentedInput
    case noOp
    case cancelled
    case invalid
    case stale
    case failed(CoreHostFailure)
}

public enum WorkspaceCommandInput: Identifiable, Equatable, Sendable {
    case zoomPercent(Double)
    case boxZoom(x: Int32, y: Int32, width: Int32, height: Int32)
    case addGuide(axis: CoreGuideAxis, position: Int32)
    case moveGuide(id: UInt64, position: Int32)
    case grid(CoreGridDefinition)
    case selectionAdjust(CoreSelectionAdjustOperation, UInt32)

    public var id: InkpodCommandID {
        switch self {
        case .zoomPercent: .zoomPercent
        case .boxZoom: .boxZoom
        case let .addGuide(axis, _):
            axis == .vertical ? .guideVertical : .guideHorizontal
        case .moveGuide: .guideMove
        case .grid: .gridSettings
        case let .selectionAdjust(operation, _):
            operation == .expand ? .selectionExpand : .selectionShrink
        }
    }
}

struct PasteConfirmation: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    let mode: CorePasteMode
}

struct FloatingTransformDraft: Identifiable, Equatable {
    let id = UUID()
    let context: CommandTargetContext
    let mode: CorePasteMode
    let sourceWidth: Double
    let sourceHeight: Double
    var anchor: CoreFloatingAnchor
    var targetX: Double
    var targetY: Double
    var scaleX: Double
    var scaleY: Double
    var rotationDegrees: Double

    init(
        context: CommandTargetContext,
        mode: CorePasteMode,
        raster: CoreClipboardRaster
    ) {
        self.context = context
        self.mode = mode
        sourceWidth = Double(raster.width)
        sourceHeight = Double(raster.height)
        anchor = .topLeft
        targetX = Double(raster.originX)
        targetY = Double(raster.originY)
        scaleX = 1
        scaleY = 1
        rotationDegrees = 0
    }

    var transform: CoreFloatingTransform {
        CoreFloatingTransform(
            anchor: anchor,
            targetX: targetX,
            targetY: targetY,
            scaleX: scaleX,
            scaleY: scaleY,
            rotationDegrees: rotationDegrees
        )
    }
}

public struct InkpodCommandTargetFocusedKey: FocusedValueKey {
    public typealias Value = CommandTargetContext
}

extension FocusedValues {
    public var inkpodCommandTarget: CommandTargetContext? {
        get { self[InkpodCommandTargetFocusedKey.self] }
        set { self[InkpodCommandTargetFocusedKey.self] = newValue }
    }
}

public enum CommandCatalog {
    public static let parityCommandIDs = Set(InkpodCommandID.allCases)

    public static let descriptors: [CommandDescriptor] = InkpodCommandID.allCases.map {
        descriptor(for: $0)
    }

    public static func descriptor(for command: InkpodCommandID) -> CommandDescriptor {
        let cutCommands: Set<InkpodCommandID> = [
            .fileNewCut, .cutProperties, .cutSave, .cutUndo, .cutRedo,
            .cutSequenceAdd, .cutSequenceRemove, .cutSequenceMoveUp,
            .cutSequenceMoveDown, .cutSequenceRenumber,
        ]
        if cutCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .cut,
                stateOwner: .cut,
                targetScope: .cutSession,
                surfaces: [.fileMenu, .timeline],
                parityTestID: command.rawValue >= InkpodCommandID.cutSequenceAdd.rawValue
                    ? "MAC-SEQUENCE-STRUCTURE-001" : "MAC-CUT-WORKFLOW-001"
            )
        }
        let animationCommands: Set<InkpodCommandID> = [
            .lightTableSetNew, .lightTableSetDuplicate, .lightTableSetDelete,
            .lightTableSetRename, .lightTableSetUp, .lightTableSetDown,
            .lightTableGlobalOpacity, .lightTableItemAdd, .lightTableItemReload,
            .lightTableItemDelete, .lightTableItemUp, .lightTableItemDown,
            .lightTableItemProperties, .lightTableItemSample,
            .lightTableItemSwap, .lightTableItemMove, .lightTableBulkPrevious,
            .lightTableBulkNext, .lightTableBulkBoth, .sequenceImport,
            .sequenceExport, .sequencePrevious, .sequenceNext, .sequenceGoto,
            .subpaletteSet, .subpaletteSample, .motionStart, .motionPause,
            .motionPrevious, .motionNext, .motionStop, .motionFirst, .motionLast,
            .motionFPS30, .motionFPS25, .motionFPS24, .motionFPS12,
            .motionFPS10, .motionFPS8, .sequenceWrapEndpoints,
        ]
        if animationCommands.contains(command) {
            let testID: String
            if command.rawValue <= InkpodCommandID.lightTableBulkBoth.rawValue {
                testID = "MAC-LIGHT-TABLE-001"
            } else if command == .sequenceWrapEndpoints {
                testID = "MAC-SEQUENCE-ENDPOINT-001"
            } else if command.rawValue >= InkpodCommandID.subpaletteSet.rawValue {
                testID = "MAC-MOTION-SUBPALETTE-001"
            } else {
                testID = "MAC-SEQUENCE-WORKFLOW-001"
            }
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .animation,
                stateOwner: .animation,
                targetScope: .cutSession,
                surfaces: [.animationMenu, .timeline, .inspector],
                parityTestID: testID
            )
        }
        let m9WorkspaceCommands: Set<InkpodCommandID> = [
            .windowSequence, .windowLightTable, .windowSubpalette,
        ]
        if m9WorkspaceCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .workspace,
                stateOwner: .workspace,
                targetScope: .workspace,
                surfaces: [.windowMenu],
                parityTestID: "MAC-ANIMATION-SURFACE-001"
            )
        }
        let m9PaneCommands: Set<InkpodCommandID> = [
            .sequencePin, .lightTablePin, .subpalettePin,
        ]
        if m9PaneCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .paneTarget,
                stateOwner: .paneTarget,
                targetScope: .pane,
                surfaces: [.windowMenu, .inspector],
                parityTestID: "MAC-ANIMATION-SURFACE-001"
            )
        }
        if command == .windowBatch || command == .windowJobProgress {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .workspace,
                stateOwner: .workspace,
                targetScope: .workspace,
                surfaces: [.windowMenu],
                parityTestID: "MAC-BATCH-WORKFLOW-001"
            )
        }
        if BatchCommandCatalog.surfaceCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .batch,
                stateOwner: .batch,
                targetScope: .job,
                surfaces: command == .batchPin ? [.windowMenu, .batchWindow] : [.batchWindow],
                parityTestID: "MAC-BATCH-WORKFLOW-001"
            )
        }
        let applicationCommands: Set<InkpodCommandID> = [
            .appExit, .helpAbout, .helpManual, .helpFileFormat, .helpWebPage,
            .helpAcknowledgements, .shortcutReset, .shortcutEdit, .languageSystem,
            .languageJapanese, .languageEnglish,
        ]
        if applicationCommands.contains(command) {
            let help = command.rawValue >= InkpodCommandID.helpAbout.rawValue
                && command.rawValue <= InkpodCommandID.helpAcknowledgements.rawValue
            let settings = command.rawValue >= InkpodCommandID.shortcutReset.rawValue
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .application,
                stateOwner: .application,
                targetScope: .application,
                surfaces: settings ? [.settings] : (help ? [.helpMenu] : [.standardMenu]),
                parityTestID: "MAC-COMMAND-SURFACE-001"
            )
        }
        let m7EditCommands: Set<InkpodCommandID> = [
            .undo, .redo, .editMirrorHorizontal, .historyBack, .historyForward,
            .floatingTransform, .floatingCommit, .floatingCancel,
        ]
        if m7EditCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .edit,
                stateOwner: .edit,
                targetScope: .documentSession,
                surfaces: [.editMenu],
                parityTestID: "MAC-SELECTION-HISTORY-001"
            )
        }
        let fileCommands: Set<InkpodCommandID> = [
            .fileNew, .fileOpen, .fileSave, .fileSaveAs, .fileRevert, .fileAutosaveNow,
            .fileOpenRecovery, .fileRevertPartial, .fileImportRaster,
            .fileExportRaster, .fileOpenRecent, .fileRestorePrevious,
            .fileCompactCopy, .fileSequenceAutosave, .fileExportInstructionRaster,
        ]
        if fileCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .fileLifecycle,
                stateOwner: .fileLifecycle,
                targetScope: .documentSession,
                surfaces: [.fileMenu],
                parityTestID: command == .fileExportInstructionRaster
                    ? "MAC-ANNOTATION-WORKFLOW-001" : "MAC-FILE-LIFECYCLE-001"
            )
        }
        let imageCommands: Set<InkpodCommandID> = [
            .filterLast, .filterInvert, .filterBlurWeak, .filterSharpenWeak,
            .filterSharpenStrong, .filterBlurStrong, .filterGaussian,
            .filterAutoContrast, .filterBrightness, .filterToneCurve,
            .filterLevels, .filterHSV, .filterColorBalance, .filterUnsharp,
            .effectGradient, .effectAirbrush, .effectBoundaryAirbrush,
            .effectBlur, .effectStamp, .effectDust, .effectAlphaGradient,
            .effectAlphaView, .adjustmentCreate, .adjustmentEdit,
            .adjustmentToggle, .adjustmentMoveTop, .adjustmentPrevious,
            .adjustmentNext,
        ]
        if imageCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .image,
                stateOwner: .image,
                targetScope: .documentSession,
                surfaces: [.imageMenu, .inspector],
                parityTestID: "MAC-FILTER-EFFECT-001"
            )
        }
        let editCommands: Set<InkpodCommandID> = [
            .editCopy, .editPaste, .editCut, .editPasteSelected,
            .editPasteConverted,
        ]
        if editCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .edit,
                stateOwner: .edit,
                targetScope: .documentSession,
                surfaces: [.editMenu],
                parityTestID: "MAC-CLIPBOARD-001"
            )
        }
        let selectionCommands: Set<InkpodCommandID> = [
            .selectionAll, .selectionInvert, .selectionExpand, .selectionShrink,
            .selectionClear, .selectionRectangle, .selectionEllipse,
            .selectionLasso, .selectionPolyline, .selectionTrace, .selectionWand,
            .selectionModeNew, .selectionModeAdd, .selectionModeSubtract,
            .selectionModeIntersect, .selectionColor, .selectionColorDifferent,
            .selectionColorAdd, .selectionToLayer, .selectionFromLayer,
            .selectionLayerAdd, .selectionLayerSubtract, .selectionOptions,
        ]
        if selectionCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .edit,
                stateOwner: .edit,
                targetScope: .documentSession,
                surfaces: [.selectionMenu],
                parityTestID: "MAC-SELECTION-HISTORY-001"
            )
        }
        if command == .documentClose {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .session,
                stateOwner: .session,
                targetScope: .documentSession,
                surfaces: [.fileMenu],
                parityTestID: "MAC-FILE-LIFECYCLE-001"
            )
        }
        let toolCommands: Set<InkpodCommandID> = [
            .toolPencil, .toolBrush, .toolEraser, .toolFill, .toolEyedropper,
            .toolFillOptions, .toolClosedFill, .toolFillExtension,
            .toolColorReplaceTarget, .toolColorReplacePen,
            .toolColorReplaceRectangle, .toolColorReplacePolyline,
            .toolColorReplaceLasso, .toolColorReplaceAll,
            .vectorLine, .vectorCurve, .vectorRectangle, .vectorEllipse,
            .vectorPolyline, .vectorEraser, .vectorErasePartial,
            .vectorEraseIntersection, .vectorEraseWhole, .vectorConnect,
            .vectorWidth, .vectorSelectCut, .vectorSelectTouch,
            .vectorSelectContained, .vectorSelectLine, .vectorSelectWholeLine,
            .vectorSelectIntersection, .vectorSelectFillBoundary,
            .vectorSelectFill, .vectorRasterize, .vectorVectorize,
            .vectorPolygon, .geometryOptions,
        ]
        if toolCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .tool,
                stateOwner: .tool,
                targetScope: .documentView,
                surfaces: [.toolsMenu, .sidebar, .contextMenu],
                parityTestID: command.rawValue >= InkpodCommandID.vectorLine.rawValue
                    ? "MAC-VECTOR-WORKFLOW-001" : "MAC-PAINT-FILL-001"
            )
        }
        let colorCommands: Set<InkpodCommandID> = [
            .colorChoose, .colorCheckOff, .colorCheckLegacy, .colorCheckNative,
            .colorEditor, .colorSourceTopmost, .colorSourceSelected,
            .colorSourceComposite, .colorSourceLightTable, .paletteRegister,
            .paletteDelete, .paletteClear, .paletteSave, .paletteLoad,
            .paletteNextGroup, .chartGenerate, .chartSearch, .chartNext,
            .chartLock, .chartCopy, .chartPaste, .chartCut, .chartRename,
            .chartSave, .chartLoad, .chartNextPage,
        ]
        if colorCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .color,
                stateOwner: .color,
                targetScope: .documentSession,
                surfaces: [.colorMenu, .inspector],
                parityTestID: "MAC-COLOR-WORKFLOW-001"
            )
        }
        if command == .colorPin {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .paneTarget,
                stateOwner: .paneTarget,
                targetScope: .pane,
                surfaces: [.colorMenu, .inspector],
                parityTestID: "MAC-COLOR-WORKFLOW-001"
            )
        }
        if command == .selectionOutputColorGuard {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .edit,
                stateOwner: .edit,
                targetScope: .documentSession,
                surfaces: [.selectionMenu],
                parityTestID: "MAC-COLOR-OUTPUT-QA-001"
            )
        }
        let locatorCommands: Set<InkpodCommandID> = [
            .locatorPin, .locatorFixed, .locatorAutoscroll,
        ]
        if locatorCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .view,
                stateOwner: .view,
                targetScope: .documentView,
                surfaces: [.windowMenu, .inspector],
                parityTestID: "MAC-LOCATOR-001"
            )
        }
        let workspaceCommands: Set<InkpodCommandID> = [
            .viewNew, .viewClose, .tabNext, .tabPrevious,
            .editorSplitRight, .editorSplitDown, .editorMoveOtherGroup,
            .editorNewViewOtherGroup, .editorGroupClose, .editorGroupNext,
            .tabMoveLeft, .tabMoveRight, .viewMoveNextWindow,
            .viewDuplicateNextWindow, .windowLayerPalette, .workspaceReset,
            .workspaceSave, .workspaceRestore, .workspaceMirror,
            .workspacePresetColoring, .workspacePresetLineCleanup,
            .workspacePresetReference, .workspacePresetBatch,
            .workspacePresetFocus, .workspaceSaveAs, .workspaceNewWindow,
            .viewMoveNewWindow, .viewDuplicateNewWindow, .windowToolPalette,
            .windowToolOptions, .windowColorPane, .windowLocator,
        ]
        if workspaceCommands.contains(command) {
            let tabSurface = command.rawValue >= InkpodCommandID.viewNew.rawValue
                && command.rawValue <= InkpodCommandID.viewDuplicateNextWindow.rawValue
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .workspace,
                stateOwner: .workspace,
                targetScope: .workspace,
                surfaces: tabSurface ? [.viewMenu, .tabStrip] : [.windowMenu],
                parityTestID: [
                    .windowToolPalette, .windowToolOptions, .windowColorPane,
                    .windowLocator,
                ].contains(command) ? "MAC-PAINT-SURFACE-001" : "MAC-WORKSPACE-001"
            )
        }
        let cellCommands: Set<InkpodCommandID> = [
            .planeMainLine, .planeColor, .layerDuplicate, .layerDelete,
            .layerMoveTop, .cellPaperSettings, .cellFrameHundred,
            .cellFrameReference, .cellFrameDrawing, .cellFrameSafe,
            .cellMargins, .cellMirrorVertical, .cellRotateLeft,
            .cellRotateRight, .cellImageSize, .cellResolution,
            .cellFitCaptureFrame, .layerNew, .layerMoveUp, .layerMoveDown,
            .layerToggleVisible, .layerToggleEditable, .layerOpacity,
            .layerConvert, .layerMerge, .layerDeleteHidden, .layerProperties,
            .planeNew, .planeDuplicate, .planeDelete, .planeMoveUp,
            .planeMoveDown, .planeToggleVisible, .planeToggleEditable,
            .planeOpacity, .planeConvert, .planeMerge, .planeProperties,
            .cellShootingFrameProperties, .cellShootingFrameEditHandles,
            .cellShootingFrameDelete, .cellVanishingPointProperties,
            .cellVanishingPointEditHandles, .cellVanishingPointDeleteAll,
            .annotationAddText, .annotationEditText, .annotationDrawInstruction,
            .annotationSelectPrevious, .annotationSelectNext,
            .annotationMoveLeft, .annotationMoveRight, .annotationDelete,
        ]
        if cellCommands.contains(command) {
            return CommandDescriptor(
                id: command,
                localizationKey: "command.\(command)",
                routeOwner: .cell,
                stateOwner: .cell,
                targetScope: .documentSession,
                surfaces: [.cellMenu, .inspector],
                parityTestID: command.rawValue >= 41_313
                    ? (command.rawValue >= 41_411
                        ? "MAC-ANNOTATION-WORKFLOW-001" : "MAC-FRAME-GUIDE-001")
                    : "MAC-CELL-WORKFLOW-001"
            )
        }
        var surfaces: Set<CommandSurface> = [.viewMenu]
        if [.zoomIn, .zoomOut, .fit, .oneToOne, .grid].contains(command) {
            surfaces.insert(.toolbar)
            surfaces.insert(.contextMenu)
        }
        return CommandDescriptor(
            id: command,
            localizationKey: "command.\(command)",
            routeOwner: .view,
            stateOwner: .view,
            targetScope: .documentView,
            surfaces: surfaces,
            parityTestID: command.rawValue >= InkpodCommandID.viewVectorAntialias.rawValue
                && command.rawValue <= InkpodCommandID.viewVectorEndpoints.rawValue
                ? "MAC-RENDER-DIAGNOSTICS-001" : "MAC-COMMAND-SURFACE-001"
        )
    }
}

extension InkpodCommandID: CustomStringConvertible {
    public var description: String {
        switch self {
        case .fileNew: "file.new"
        case .fileOpen: "file.open"
        case .fileSave: "file.save"
        case .fileSaveAs: "file.save.as"
        case .fileRevert: "file.revert"
        case .fileAutosaveNow: "file.autosave.now"
        case .fileOpenRecovery: "file.open.recovery"
        case .fileRevertPartial: "file.revert.partial"
        case .fileImportRaster: "file.import.raster"
        case .fileExportRaster: "file.export.raster"
        case .fileOpenRecent: "file.open.recent"
        case .fileRestorePrevious: "file.restore.previous"
        case .fileCompactCopy: "file.compact.copy"
        case .fileSequenceAutosave: "file.sequence.autosave"
        case .fileNewCut: "file.new.cut"
        case .cutProperties: "cut.properties"
        case .cutSave: "cut.save"
        case .cutUndo: "cut.undo"
        case .cutRedo: "cut.redo"
        case .cutSequenceAdd: "cut.sequence.add"
        case .cutSequenceRemove: "cut.sequence.remove"
        case .cutSequenceMoveUp: "cut.sequence.move.up"
        case .cutSequenceMoveDown: "cut.sequence.move.down"
        case .cutSequenceRenumber: "cut.sequence.renumber"
        case .fileExportInstructionRaster: "file.export.instruction.raster"
        case .appExit: "app.exit"
        case .undo: "edit.undo"
        case .redo: "edit.redo"
        case .editCopy: "edit.copy"
        case .editPaste: "edit.paste"
        case .editMirrorHorizontal: "edit.mirror.horizontal"
        case .historyBack: "edit.history.back"
        case .historyForward: "edit.history.forward"
        case .editCut: "edit.cut"
        case .editPasteSelected: "edit.paste.selected"
        case .editPasteConverted: "edit.paste.converted"
        case .floatingTransform: "edit.floating.transform"
        case .floatingCommit: "edit.floating.commit"
        case .floatingCancel: "edit.floating.cancel"
        case .zoomIn: "view.zoom.in"
        case .zoomOut: "view.zoom.out"
        case .fit: "view.fit"
        case .oneToOne: "view.one.to.one"
        case .flipHorizontal: "view.flip.horizontal"
        case .flipVertical: "view.flip.vertical"
        case .grid: "view.grid"
        case .zoomPercent: "view.zoom.percent"
        case .boxZoom: "view.box.zoom"
        case .ruler: "view.ruler"
        case .guides: "view.guides"
        case .snapGuides: "view.snap.guides"
        case .snapGrid: "view.snap.grid"
        case .transparent: "view.transparent"
        case .guideVertical: "view.guide.vertical"
        case .guideHorizontal: "view.guide.horizontal"
        case .guideDeleteAll: "view.guide.delete.all"
        case .gridSettings: "view.grid.settings"
        case .guideMove: "view.guide.move"
        case .viewVectorAntialias: "view.vector.antialias"
        case .viewVectorCenterline: "view.vector.centerline"
        case .viewVectorCenterlineOnly: "view.vector.centerline.only"
        case .viewVectorEndpoints: "view.vector.endpoints"
        case .toolPencil: "tool.pencil"
        case .toolBrush: "tool.brush"
        case .toolEraser: "tool.eraser"
        case .toolFill: "tool.fill"
        case .toolEyedropper: "tool.eyedropper"
        case .toolFillOptions: "tool.fill.options"
        case .toolClosedFill: "tool.closed.fill"
        case .toolFillExtension: "tool.fill.extension"
        case .toolColorReplaceTarget: "tool.color.replace.target"
        case .toolColorReplacePen: "tool.color.replace.pen"
        case .toolColorReplaceRectangle: "tool.color.replace.rectangle"
        case .toolColorReplacePolyline: "tool.color.replace.polyline"
        case .toolColorReplaceLasso: "tool.color.replace.lasso"
        case .toolColorReplaceAll: "tool.color.replace.all"
        case .colorChoose: "color.choose"
        case .colorCheckOff: "color.check.off"
        case .colorCheckLegacy: "color.check.legacy"
        case .colorCheckNative: "color.check.native"
        case .colorEditor: "color.editor"
        case .colorSourceTopmost: "color.source.topmost"
        case .colorSourceSelected: "color.source.selected"
        case .colorSourceComposite: "color.source.composite"
        case .colorSourceLightTable: "color.source.light.table"
        case .paletteRegister: "palette.register"
        case .paletteDelete: "palette.delete"
        case .paletteClear: "palette.clear"
        case .paletteSave: "palette.save"
        case .paletteLoad: "palette.load"
        case .paletteNextGroup: "palette.next.group"
        case .chartGenerate: "chart.generate"
        case .chartSearch: "chart.search"
        case .chartNext: "chart.next"
        case .chartLock: "chart.lock"
        case .chartCopy: "chart.copy"
        case .chartPaste: "chart.paste"
        case .chartCut: "chart.cut"
        case .chartRename: "chart.rename"
        case .chartSave: "chart.save"
        case .chartLoad: "chart.load"
        case .chartNextPage: "chart.next.page"
        case .helpAbout: "help.about"
        case .helpManual: "help.manual"
        case .helpFileFormat: "help.file.format"
        case .helpWebPage: "help.web.page"
        case .helpAcknowledgements: "help.acknowledgements"
        case .shortcutReset: "shortcut.reset"
        case .shortcutEdit: "shortcut.edit"
        case .languageSystem: "language.system"
        case .languageJapanese: "language.japanese"
        case .languageEnglish: "language.english"
        case .documentClose: "document.close"
        case .viewNew: "view.new"
        case .viewClose: "view.close"
        case .tabNext: "tab.next"
        case .tabPrevious: "tab.previous"
        case .editorSplitRight: "editor.split.right"
        case .editorSplitDown: "editor.split.down"
        case .editorMoveOtherGroup: "editor.move.other.group"
        case .editorNewViewOtherGroup: "editor.new.view.other.group"
        case .editorGroupClose: "editor.group.close"
        case .editorGroupNext: "editor.group.next"
        case .tabMoveLeft: "tab.move.left"
        case .tabMoveRight: "tab.move.right"
        case .viewMoveNextWindow: "view.move.next.window"
        case .viewDuplicateNextWindow: "view.duplicate.next.window"
        case .planeMainLine: "plane.main.line"
        case .planeColor: "plane.color"
        case .selectionAll: "selection.all"
        case .selectionInvert: "selection.invert"
        case .selectionExpand: "selection.expand"
        case .selectionShrink: "selection.shrink"
        case .selectionClear: "selection.clear"
        case .selectionRectangle: "selection.rectangle"
        case .selectionEllipse: "selection.ellipse"
        case .selectionLasso: "selection.lasso"
        case .selectionPolyline: "selection.polyline"
        case .selectionTrace: "selection.trace"
        case .selectionWand: "selection.wand"
        case .selectionModeNew: "selection.mode.new"
        case .selectionModeAdd: "selection.mode.add"
        case .selectionModeSubtract: "selection.mode.subtract"
        case .selectionModeIntersect: "selection.mode.intersect"
        case .selectionColor: "selection.color"
        case .selectionColorDifferent: "selection.color.different"
        case .selectionColorAdd: "selection.color.add"
        case .selectionToLayer: "selection.to.layer"
        case .selectionFromLayer: "selection.from.layer"
        case .selectionLayerAdd: "selection.layer.add"
        case .selectionLayerSubtract: "selection.layer.subtract"
        case .selectionOptions: "selection.options"
        case .selectionOutputColorGuard: "selection.output.color.guard"
        case .layerDuplicate: "layer.duplicate"
        case .layerDelete: "layer.delete"
        case .layerMoveTop: "layer.move.top"
        case .cellPaperSettings: "cell.paper.settings"
        case .cellFrameHundred: "cell.frame.hundred"
        case .cellFrameReference: "cell.frame.reference"
        case .cellFrameDrawing: "cell.frame.drawing"
        case .cellFrameSafe: "cell.frame.safe"
        case .cellMargins: "cell.margins"
        case .cellMirrorVertical: "cell.mirror.vertical"
        case .cellRotateLeft: "cell.rotate.left"
        case .cellRotateRight: "cell.rotate.right"
        case .cellImageSize: "cell.image.size"
        case .cellResolution: "cell.resolution"
        case .cellFitCaptureFrame: "cell.fit.capture.frame"
        case .layerNew: "layer.new"
        case .layerMoveUp: "layer.move.up"
        case .layerMoveDown: "layer.move.down"
        case .layerToggleVisible: "layer.toggle.visible"
        case .layerToggleEditable: "layer.toggle.editable"
        case .layerOpacity: "layer.opacity"
        case .layerConvert: "layer.convert"
        case .layerMerge: "layer.merge"
        case .layerDeleteHidden: "layer.delete.hidden"
        case .layerProperties: "layer.properties"
        case .planeNew: "plane.new"
        case .planeDuplicate: "plane.duplicate"
        case .planeDelete: "plane.delete"
        case .planeMoveUp: "plane.move.up"
        case .planeMoveDown: "plane.move.down"
        case .planeToggleVisible: "plane.toggle.visible"
        case .planeToggleEditable: "plane.toggle.editable"
        case .planeOpacity: "plane.opacity"
        case .planeConvert: "plane.convert"
        case .planeMerge: "plane.merge"
        case .planeProperties: "plane.properties"
        case .windowLayerPalette: "window.layer.palette"
        case .windowToolPalette: "window.tool.palette"
        case .windowToolOptions: "window.tool.options"
        case .windowColorPane: "window.color.pane"
        case .windowLocator: "window.locator"
        case .locatorPin: "locator.pin"
        case .locatorFixed: "locator.fixed"
        case .locatorAutoscroll: "locator.autoscroll"
        case .colorPin: "color.pin"
        case .workspaceReset: "workspace.reset"
        case .workspaceSave: "workspace.save"
        case .workspaceRestore: "workspace.restore"
        case .workspaceMirror: "workspace.mirror"
        case .workspacePresetColoring: "workspace.preset.coloring"
        case .workspacePresetLineCleanup: "workspace.preset.line.cleanup"
        case .workspacePresetReference: "workspace.preset.reference"
        case .workspacePresetBatch: "workspace.preset.batch"
        case .workspacePresetFocus: "workspace.preset.focus"
        case .workspaceSaveAs: "workspace.save.as"
        case .workspaceNewWindow: "workspace.new.window"
        case .viewMoveNewWindow: "view.move.new.window"
        case .viewDuplicateNewWindow: "view.duplicate.new.window"
        case .filterLast: "filter.last"
        case .filterInvert: "filter.invert"
        case .filterBlurWeak: "filter.blur.weak"
        case .filterSharpenWeak: "filter.sharpen.weak"
        case .filterSharpenStrong: "filter.sharpen.strong"
        case .filterBlurStrong: "filter.blur.strong"
        case .filterGaussian: "filter.gaussian"
        case .filterAutoContrast: "filter.auto.contrast"
        case .filterBrightness: "filter.brightness"
        case .filterToneCurve: "filter.tone.curve"
        case .filterLevels: "filter.levels"
        case .filterHSV: "filter.hsv"
        case .filterColorBalance: "filter.color.balance"
        case .filterUnsharp: "filter.unsharp"
        case .effectGradient: "effect.gradient"
        case .effectAirbrush: "effect.airbrush"
        case .effectBoundaryAirbrush: "effect.boundary.airbrush"
        case .effectBlur: "effect.blur"
        case .effectStamp: "effect.stamp"
        case .effectDust: "effect.dust"
        case .effectAlphaGradient: "effect.alpha.gradient"
        case .effectAlphaView: "effect.alpha.view"
        case .adjustmentCreate: "adjustment.create"
        case .adjustmentEdit: "adjustment.edit"
        case .adjustmentToggle: "adjustment.toggle"
        case .adjustmentMoveTop: "adjustment.move.top"
        case .adjustmentPrevious: "adjustment.previous"
        case .adjustmentNext: "adjustment.next"
        case .cellShootingFrameProperties: "cell.shooting.frame.properties"
        case .cellShootingFrameEditHandles: "cell.shooting.frame.edit.handles"
        case .cellShootingFrameDelete: "cell.shooting.frame.delete"
        case .cellVanishingPointProperties: "cell.vanishing.point.properties"
        case .cellVanishingPointEditHandles: "cell.vanishing.point.edit.handles"
        case .cellVanishingPointDeleteAll: "cell.vanishing.point.delete.all"
        case .annotationAddText: "annotation.add.text"
        case .annotationEditText: "annotation.edit.text"
        case .annotationDrawInstruction: "annotation.draw.instruction"
        case .annotationSelectPrevious: "annotation.select.previous"
        case .annotationSelectNext: "annotation.select.next"
        case .annotationMoveLeft: "annotation.move.left"
        case .annotationMoveRight: "annotation.move.right"
        case .annotationDelete: "annotation.delete"
        case .vectorLine: "vector.line"
        case .vectorCurve: "vector.curve"
        case .vectorRectangle: "vector.rectangle"
        case .vectorEllipse: "vector.ellipse"
        case .vectorPolyline: "vector.polyline"
        case .vectorEraser: "vector.eraser"
        case .vectorErasePartial: "vector.erase.partial"
        case .vectorEraseIntersection: "vector.erase.intersection"
        case .vectorEraseWhole: "vector.erase.whole"
        case .vectorConnect: "vector.connect"
        case .vectorWidth: "vector.width"
        case .vectorSelectCut: "vector.select.cut"
        case .vectorSelectTouch: "vector.select.touch"
        case .vectorSelectContained: "vector.select.contained"
        case .vectorSelectLine: "vector.select.line"
        case .vectorSelectWholeLine: "vector.select.whole.line"
        case .vectorSelectIntersection: "vector.select.intersection"
        case .vectorSelectFillBoundary: "vector.select.fill.boundary"
        case .vectorSelectFill: "vector.select.fill"
        case .vectorRasterize: "vector.rasterize"
        case .vectorVectorize: "vector.vectorize"
        case .vectorPolygon: "vector.polygon"
        case .geometryOptions: "geometry.options"
        case .lightTableSetNew: "lt.set.new"
        case .lightTableSetDuplicate: "lt.set.duplicate"
        case .lightTableSetDelete: "lt.set.delete"
        case .lightTableSetRename: "lt.set.rename"
        case .lightTableSetUp: "lt.set.up"
        case .lightTableSetDown: "lt.set.down"
        case .lightTableGlobalOpacity: "lt.global.opacity"
        case .lightTableItemAdd: "lt.item.add"
        case .lightTableItemReload: "lt.item.reload"
        case .lightTableItemDelete: "lt.item.delete"
        case .lightTableItemUp: "lt.item.up"
        case .lightTableItemDown: "lt.item.down"
        case .lightTableItemProperties: "lt.item.properties"
        case .lightTableItemSample: "lt.item.sample"
        case .lightTableItemSwap: "lt.item.swap"
        case .lightTableItemMove: "lt.item.move"
        case .lightTableBulkPrevious: "lt.bulk.previous"
        case .lightTableBulkNext: "lt.bulk.next"
        case .lightTableBulkBoth: "lt.bulk.both"
        case .sequenceImport: "seq.import"
        case .sequenceExport: "seq.export"
        case .sequencePrevious: "seq.previous"
        case .sequenceNext: "seq.next"
        case .sequenceGoto: "seq.goto"
        case .subpaletteSet: "subpalette.set"
        case .subpaletteSample: "subpalette.sample"
        case .motionStart: "motion.start"
        case .motionPause: "motion.pause"
        case .motionPrevious: "motion.previous"
        case .motionNext: "motion.next"
        case .motionStop: "motion.stop"
        case .motionFirst: "motion.first"
        case .motionLast: "motion.last"
        case .motionFPS30: "motion.fps.30"
        case .motionFPS25: "motion.fps.25"
        case .motionFPS24: "motion.fps.24"
        case .motionFPS12: "motion.fps.12"
        case .motionFPS10: "motion.fps.10"
        case .motionFPS8: "motion.fps.8"
        case .sequenceWrapEndpoints: "seq.wrap.endpoints"
        case .windowSequence: "window.sequence"
        case .sequencePin: "sequence.pin"
        case .windowLightTable: "window.light.table"
        case .lightTablePin: "light.table.pin"
        case .windowSubpalette: "window.subpalette"
        case .subpalettePin: "subpalette.pin"
        case .windowBatch: "window.batch"
        case .batchInputFile: "batch.input.file"
        case .batchInputFolder: "batch.input.folder"
        case .batchInputCurrent: "batch.input.current"
        case .batchOperationRemove: "batch.operation.remove"
        case .batchOperationUp: "batch.operation.up"
        case .batchOperationDown: "batch.operation.down"
        case .batchOperationEdit: "batch.operation.edit"
        case .batchReplaceSwap: "batch.replace.swap"
        case .batchOutputDuplicate: "batch.output.duplicate"
        case .batchOutputNew: "batch.output.new"
        case .batchOutputOverwrite: "batch.output.overwrite"
        case .batchFailureContinue: "batch.failure.continue"
        case .batchFailureStop: "batch.failure.stop"
        case .batchPreview: "batch.preview"
        case .batchDryRun: "batch.dry.run"
        case .batchRunCurrent: "batch.run.current"
        case .batchRunAll: "batch.run.all"
        case .batchSaveSet: "batch.save.set"
        case .batchLoadSet: "batch.load.set"
        case .batchCancel: "batch.cancel"
        case .batchInputRange: "batch.input.range"
        case .batchOutputSettings: "batch.output.settings"
        case .batchPin: "batch.pin"
        case .windowJobProgress: "window.job.progress"
        case .batchAddColorReplace: "batch.add.color.replace"
        case .batchAddContinuousFill: "batch.add.continuous.fill"
        case .batchAddSeparation: "batch.add.separation"
        case .batchAddVisibility: "batch.add.visibility"
        case .batchAddLineWidth: "batch.add.line.width"
        case .batchAddBoundaryAirbrush: "batch.add.boundary.airbrush"
        case .batchAddDust: "batch.add.dust"
        case .batchAddMirror: "batch.add.mirror"
        case .batchAddRotate: "batch.add.rotate"
        case .batchAddResize: "batch.add.resize"
        case .batchAddConvert: "batch.add.convert"
        case .batchExtractPairs: "batch.extract.pairs"
        case .batchAddFilterSharpenWeak: "batch.add.filter.sharpen.weak"
        case .batchAddFilterSharpenStrong: "batch.add.filter.sharpen.strong"
        case .batchAddFilterBlurWeak: "batch.add.filter.blur.weak"
        case .batchAddFilterBlurStrong: "batch.add.filter.blur.strong"
        case .batchAddFilterGaussian: "batch.add.filter.gaussian"
        case .batchAddFilterInvert: "batch.add.filter.invert"
        case .batchAddFilterAutoContrast: "batch.add.filter.auto.contrast"
        case .batchAddFilterBrightness: "batch.add.filter.brightness"
        case .batchAddFilterToneCurve: "batch.add.filter.tone.curve"
        case .batchAddFilterLevels: "batch.add.filter.levels"
        case .batchAddFilterHSV: "batch.add.filter.hsv"
        case .batchAddFilterColorBalance: "batch.add.filter.color.balance"
        case .batchAddFilterUnsharp: "batch.add.filter.unsharp"
        }
    }
}
