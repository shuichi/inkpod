import Foundation

public enum CoreEditorTool: UInt32, CaseIterable, Equatable, Sendable {
    case pencil = 1
    case brush = 2
    case eraser = 3
    case fill = 1_001
    case eyedropper = 1_002
    case selection = 1_005
    case floatingTransform = 1_006
    case colorReplace = 1_008

    public var consumesColor: Bool {
        switch self {
        case .pencil, .brush, .fill, .selection, .colorReplace:
            true
        case .eraser, .eyedropper, .floatingTransform:
            false
        }
    }
}

public enum CoreColorDepth: UInt32, CaseIterable, Equatable, Sendable {
    case binary = 1
    case grayscale8 = 2
    case grayscale16 = 3
    case rgba8 = 8
    case rgba16 = 16
}

public struct CoreColorValue: Equatable, Hashable, Sendable {
    public let depth: CoreColorDepth
    public let red: UInt16
    public let green: UInt16
    public let blue: UInt16
    public let alpha: UInt16

    public init(
        depth: CoreColorDepth,
        red: UInt16,
        green: UInt16,
        blue: UInt16,
        alpha: UInt16
    ) {
        self.depth = depth
        self.red = red
        self.green = green
        self.blue = blue
        self.alpha = alpha
    }

    public static func rgba8(
        red: UInt8,
        green: UInt8,
        blue: UInt8,
        alpha: UInt8 = .max
    ) -> Self {
        Self(
            depth: .rgba8,
            red: UInt16(red),
            green: UInt16(green),
            blue: UInt16(blue),
            alpha: UInt16(alpha)
        )
    }

    public static func rgba16(
        red: UInt16,
        green: UInt16,
        blue: UInt16,
        alpha: UInt16 = .max
    ) -> Self {
        Self(depth: .rgba16, red: red, green: green, blue: blue, alpha: alpha)
    }

    var hasValidNativeComponents: Bool {
        switch depth {
        case .binary:
            return [red, green, blue, alpha].allSatisfy { $0 <= 1 }
        case .grayscale8, .rgba8:
            return [red, green, blue, alpha].allSatisfy { $0 <= UInt16(UInt8.max) }
        case .grayscale16, .rgba16:
            return true
        }
    }
}

public enum CoreBrushShape: UInt32, CaseIterable, Equatable, Sendable {
    case round = 1
    case square = 2
}

public enum CoreStartColorPredicate: UInt32, CaseIterable, Equatable, Sendable {
    case any = 0
    case exactNative = 1
}

public struct CoreBrushOptions: Equatable, Sendable {
    public let shape: CoreBrushShape
    public let smoothing: UInt16
    public let startColor: CoreStartColorPredicate

    public init(
        shape: CoreBrushShape = .round,
        smoothing: UInt16 = 0,
        startColor: CoreStartColorPredicate = .any
    ) {
        self.shape = shape
        self.smoothing = smoothing
        self.startColor = startColor
    }

    var isValid: Bool { smoothing <= 1_000 }
}

public enum CoreSelectionShape: UInt32, CaseIterable, Equatable, Sendable {
    case rectangle = 1
    case ellipse = 2
    case lasso = 3
    case polyline = 4
    case trace = 5
    case wand = 6
}

public enum CoreRangeInterpretation: UInt32, CaseIterable, Equatable, Sendable {
    case normal = 1
    case tight = 2
    case enclosedInterior = 3
    case drawing = 4
    case boundary = 5
}

public enum CoreTraceBrushShape: UInt32, CaseIterable, Equatable, Sendable {
    case round = 1
    case square = 2
}

public struct CoreSelectionOptions: Equatable, Sendable {
    public let shape: CoreSelectionShape
    public let operation: CoreSelectionOperation
    public let tolerance: UInt16
    public let gapClose: UInt16
    public let diameter: Double
    public let interpretation: CoreRangeInterpretation
    public let aspectRatio: Double
    public let fromCenter: Bool
    public let constrainRotationTo45Degrees: Bool
    public let pressureControlsSize: Bool
    public let screenSizedTrace: Bool
    public let rotationTurns: UInt32
    public let traceShape: CoreTraceBrushShape

    public init(
        shape: CoreSelectionShape = .rectangle,
        operation: CoreSelectionOperation = .replace,
        tolerance: UInt16 = 0,
        gapClose: UInt16 = 0,
        diameter: Double = 1,
        interpretation: CoreRangeInterpretation = .normal,
        aspectRatio: Double = 1,
        fromCenter: Bool = false,
        constrainRotationTo45Degrees: Bool = false,
        pressureControlsSize: Bool = false,
        screenSizedTrace: Bool = false,
        rotationTurns: UInt32 = 0,
        traceShape: CoreTraceBrushShape = .round
    ) {
        self.shape = shape
        self.operation = operation
        self.tolerance = tolerance
        self.gapClose = gapClose
        self.diameter = diameter
        self.interpretation = interpretation
        self.aspectRatio = aspectRatio
        self.fromCenter = fromCenter
        self.constrainRotationTo45Degrees = constrainRotationTo45Degrees
        self.pressureControlsSize = pressureControlsSize
        self.screenSizedTrace = screenSizedTrace
        self.rotationTurns = rotationTurns
        self.traceShape = traceShape
    }

    var isValid: Bool {
        gapClose <= UInt16(UInt8.max)
            && diameter.isFinite && (1 ... 4_096).contains(diameter)
            && aspectRatio.isFinite
            && (aspectRatio == 0 || (1.0 / 65_536 ... 65_535).contains(aspectRatio))
            && !(pressureControlsSize && screenSizedTrace)
    }
}

public enum CoreFillOperation: UInt32, CaseIterable, Equatable, Sendable {
    case seed = 1
    case closedRegion = 2
    case extensionRegion = 3
}

public enum CoreInclusionMode: UInt32, CaseIterable, Equatable, Sendable {
    case none = 0
    case specified = 1
    case exceptSpecified = 2
}

public struct CoreFillOptions: Equatable, Sendable {
    public let operation: CoreFillOperation
    public let detachedRegions: Bool
    public let overflowAbort: Bool
    public let transparentOnly: Bool
    public let useDocumentSelection: Bool
    public let useLightTableBoundary: Bool
    public let useLightTableColor: Bool
    public let tolerance: UInt16
    public let gapClose: UInt16
    public let inclusionMode: CoreInclusionMode
    public let extensionDistance: UInt32
    public let inclusionColors: [CoreColorValue]

    public init(
        operation: CoreFillOperation = .seed,
        detachedRegions: Bool = false,
        overflowAbort: Bool = true,
        transparentOnly: Bool = false,
        useDocumentSelection: Bool = true,
        useLightTableBoundary: Bool = false,
        useLightTableColor: Bool = false,
        tolerance: UInt16 = 0,
        gapClose: UInt16 = 0,
        inclusionMode: CoreInclusionMode = .none,
        extensionDistance: UInt32 = 0,
        inclusionColors: [CoreColorValue] = []
    ) {
        self.operation = operation
        self.detachedRegions = detachedRegions
        self.overflowAbort = overflowAbort
        self.transparentOnly = transparentOnly
        self.useDocumentSelection = useDocumentSelection
        self.useLightTableBoundary = useLightTableBoundary
        self.useLightTableColor = useLightTableColor
        self.tolerance = tolerance
        self.gapClose = gapClose
        self.inclusionMode = inclusionMode
        self.extensionDistance = extensionDistance
        self.inclusionColors = inclusionColors
    }

    var isValid: Bool {
        gapClose <= UInt16(UInt8.max) && extensionDistance <= 4_096
            && inclusionColors.count <= 6
            && inclusionColors.allSatisfy(\.hasValidNativeComponents)
            && (inclusionMode == .none ? inclusionColors.isEmpty : !inclusionColors.isEmpty)
    }
}

public enum CoreEditorUpdate: Equatable, Sendable {
    case activeTool(CoreEditorTool)
    case toolColor(CoreColorValue)
    case diameter(Double)
    case fillOptions(CoreFillOptions)
    case brushOptions(CoreBrushOptions)
    case selectionOptions(CoreSelectionOptions)

    var isValid: Bool {
        switch self {
        case .activeTool:
            true
        case let .toolColor(color):
            color.hasValidNativeComponents
        case let .diameter(value):
            value.isFinite && (1 ... 4_096).contains(value)
        case let .fillOptions(options):
            options.isValid
        case let .brushOptions(options):
            options.isValid
        case let .selectionOptions(options):
            options.isValid
        }
    }
}

public struct CorePaintExpectation: Equatable, Sendable {
    public let documentRevision: UInt64
    public let viewRevision: UInt64
    public let editorRevision: UInt64
    public let layerID: UInt64
    public let planeID: UInt64

    public init(
        documentRevision: UInt64,
        viewRevision: UInt64,
        editorRevision: UInt64,
        layerID: UInt64,
        planeID: UInt64
    ) {
        self.documentRevision = documentRevision
        self.viewRevision = viewRevision
        self.editorRevision = editorRevision
        self.layerID = layerID
        self.planeID = planeID
    }

    var isValid: Bool { layerID != 0 && planeID != 0 }
}

public struct CoreEditorProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let editorRevision: UInt64
    public let activeTool: CoreEditorTool
    public let lastColorConsumingTool: CoreEditorTool?
    public let currentColor: CoreColorValue
    public let diameter: Double
    public let activeLayerID: UInt64
    public let activePlaneID: UInt64
    public let fillOptions: CoreFillOptions
    public let brushOptions: CoreBrushOptions
    public let selectionOptions: CoreSelectionOptions

    public var expectation: CorePaintExpectation {
        CorePaintExpectation(
            documentRevision: session.documentRevision,
            viewRevision: session.viewRevision,
            editorRevision: editorRevision,
            layerID: activeLayerID,
            planeID: activePlaneID
        )
    }
}

public struct CorePaletteProjection: Equatable, Sendable {
    public let colors: [CoreColorValue]
}

public struct CoreColorChartEntry: Equatable, Identifiable, Sendable {
    public let index: UInt64
    public let color: CoreColorValue
    public let name: String
    public let frequency: UInt64?

    public var id: UInt64 { index }

    public init(index: UInt64, color: CoreColorValue, name: String, frequency: UInt64? = nil) {
        self.index = index
        self.color = color
        self.name = name
        self.frequency = frequency
    }
}

public struct CoreColorChartProjection: Equatable, Sendable {
    public let entries: [CoreColorChartEntry]
    public let isLocked: Bool
    public let selectedIndex: UInt64?
    public let page: UInt32
}

public enum CoreColorCheckMode: UInt32, CaseIterable, Equatable, Sendable {
    case off = 0
    case legacyWhite = 1
    case nativeAlpha = 2
}

public struct CorePaintProjection: Equatable, Sendable {
    public let editor: CoreEditorProjection
    public let palette: CorePaletteProjection
    public let chart: CoreColorChartProjection
    public let colorCheckMode: CoreColorCheckMode
}

public struct CoreFillGesture: Equatable, Sendable {
    public let start: CorePointerSample
    public let end: CorePointerSample

    public init(start: CorePointerSample, end: CorePointerSample) {
        self.start = start
        self.end = end
    }

    var isValid: Bool { start.isValid && end.isValid }
}

public struct CoreFillProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let changedPixelCount: UInt64
    public let leakCandidate: (x: UInt32, y: UInt32)?

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.session == rhs.session
            && lhs.changedPixelCount == rhs.changedPixelCount
            && lhs.leakCandidate?.x == rhs.leakCandidate?.x
            && lhs.leakCandidate?.y == rhs.leakCandidate?.y
    }
}

public enum CoreEyedropperSource: UInt32, CaseIterable, Equatable, Sendable {
    case topmostNontransparent = 1
    case selectedPlane = 2
    case composite = 3
    case lightTableTopmost = 4
}

public struct CoreDocumentPoint: Equatable, Sendable {
    public let x: Float
    public let y: Float
}

public enum CoreColorReplaceMode: UInt32, CaseIterable, Equatable, Sendable {
    case rasterColor = 1
    case rasterMainLine = 2
}

public enum CoreColorReplaceRegion: Equatable, Sendable {
    case entireSelectionOrDocument
    case rectangle(CoreFillGesture)
    case pen(samples: [CorePointerSample], diameter: Float)
    case polyline([CorePointerSample])
    case lasso([CorePointerSample])

    var isValid: Bool {
        switch self {
        case .entireSelectionOrDocument:
            true
        case let .rectangle(gesture):
            gesture.isValid
        case let .pen(samples, diameter):
            !samples.isEmpty && samples.count <= 1_048_576
                && samples.allSatisfy(\.isValid) && diameter.isFinite && diameter > 0
        case let .polyline(samples), let .lasso(samples):
            samples.count >= 2 && samples.count <= 1_048_576
                && samples.allSatisfy(\.isValid)
        }
    }
}

public struct CoreColorReplaceRequest: Equatable, Sendable {
    public let mode: CoreColorReplaceMode
    public let targetColor: CoreColorValue
    public let replacementColor: CoreColorValue
    public let region: CoreColorReplaceRegion

    public init(
        mode: CoreColorReplaceMode,
        targetColor: CoreColorValue,
        replacementColor: CoreColorValue,
        region: CoreColorReplaceRegion
    ) {
        self.mode = mode
        self.targetColor = targetColor
        self.replacementColor = replacementColor
        self.region = region
    }

    var isValid: Bool {
        targetColor.hasValidNativeComponents && replacementColor.hasValidNativeComponents
            && region.isValid
    }
}

public struct CoreColorReplacePreviewProjection: Equatable, Sendable {
    public let baseDocumentRevision: UInt64
    public let matchedPixels: UInt64
    public let matchedObjects: UInt64
    public let affectedBounds: CoreFrameRect?
}

public struct CoreLocatorProjection: Equatable, Sendable {
    public let documentX: Int32
    public let documentY: Int32
    public let selection: CoreFrameRect?
    public let color: CoreColorValue?
    public let neighborhoodOriginX: Int32
    public let neighborhoodOriginY: Int32
    public let neighborhoodWidth: UInt32
    public let neighborhoodHeight: UInt32
    public let neighborhoodRGBA8: [UInt8]
}

public struct CoreColorChartPreviewID: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) { self.rawValue = rawValue }
}

public struct CoreColorChartPreviewProjection: Equatable, Sendable {
    public let id: CoreColorChartPreviewID
    public let session: CoreSessionTarget
    public let baseDocumentRevision: UInt64
    public let entries: [CoreColorChartEntry]
    public let sourceUniqueColorCount: UInt64
    public let retainedColorCount: UInt32
    public let addedColorCount: UInt32
    public let removedColorCount: UInt32
    public let exceedsMaximum: Bool
}

public enum CoreSelectionOperation: UInt32, CaseIterable, Equatable, Sendable {
    case replace = 1
    case add = 2
    case subtract = 3
    case intersect = 4
}

public enum CoreSelectionAdjustOperation: UInt32, CaseIterable, Equatable, Sendable {
    case invert = 1
    case expand = 2
    case shrink = 3
}

public enum CoreSelectionLayerOperation: UInt32, CaseIterable, Equatable, Sendable {
    case replace = 1
    case add = 2
    case subtract = 3
}

public enum CoreFloatingAnchor: UInt32, CaseIterable, Equatable, Sendable {
    case topLeft = 1
    case topRight = 2
    case center = 3
    case bottomLeft = 4
    case bottomRight = 5
}

public struct CoreFloatingTransform: Equatable, Sendable {
    public let anchor: CoreFloatingAnchor
    public let targetX: Double
    public let targetY: Double
    public let scaleX: Double
    public let scaleY: Double
    public let rotationDegrees: Double

    public init(
        anchor: CoreFloatingAnchor,
        targetX: Double,
        targetY: Double,
        scaleX: Double,
        scaleY: Double,
        rotationDegrees: Double
    ) {
        self.anchor = anchor
        self.targetX = targetX
        self.targetY = targetY
        self.scaleX = scaleX
        self.scaleY = scaleY
        self.rotationDegrees = rotationDegrees
    }

    public static let identity = Self(
        anchor: .topLeft,
        targetX: 0,
        targetY: 0,
        scaleX: 1,
        scaleY: 1,
        rotationDegrees: 0
    )

    var isValid: Bool {
        targetX.isFinite && targetY.isFinite
            && scaleX.isFinite && scaleY.isFinite
            && scaleX != 0 && scaleY != 0
            && rotationDegrees.isFinite
            && abs(targetX) <= 16_777_216 && abs(targetY) <= 16_777_216
            && abs(scaleX) <= 4_096 && abs(scaleY) <= 4_096
            && abs(rotationDegrees) <= 1_000_000
    }
}

public enum CoreHistoryEntryKind: UInt32, Equatable, Sendable {
    case raster = 1
    case palette = 2
    case colorChart = 3
    case mainLineColor = 4
    case document = 5
}

public struct CoreHistoryItemProjection: Equatable, Identifiable, Sendable {
    public let index: UInt64
    public let kind: CoreHistoryEntryKind
    public let isApplied: Bool

    public var id: UInt64 { index }
}

public struct CoreHistoryProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let cursor: UInt64
    public let items: [CoreHistoryItemProjection]
}

public struct CoreHistoryVisualizationID: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) { self.rawValue = rawValue }
}

public struct CoreHistoryVisualizationProgressProjection: Equatable, Sendable {
    public let id: CoreHistoryVisualizationID
    public let completedEvents: UInt64
    public let totalEvents: UInt64
    public let completedRows: UInt64
    public let rowCount: UInt64
    public let isComplete: Bool
}

public struct CoreHistoryVisualizationRow: Equatable, Identifiable, Sendable {
    public let journalEventID: UInt64
    public let procedureID: UInt64
    public let committedStateID: UInt64
    public let branchID: UInt64
    public let primitiveID: UInt32
    public let primitiveName: String
    public let arguments: String
    public let thumbnailWidth: UInt32
    public let thumbnailHeight: UInt32
    public let thumbnailStrideBytes: UInt32
    public let thumbnailChecksum: UInt64
    public let thumbnailRGBA8: [UInt8]

    public var id: UInt64 { journalEventID }
}

public struct CoreOutputColorGuardProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let scannedPixelCount: UInt64
    public let selectedPixelCount: UInt64
    public let transparentPixelCount: UInt64
}
