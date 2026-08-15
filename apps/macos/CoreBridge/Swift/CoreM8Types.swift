import Foundation

public enum CoreFilterKind: UInt32, CaseIterable, Equatable, Sendable {
    case sharpenWeak = 1
    case sharpenStrong = 2
    case blurWeak = 3
    case blurStrong = 4
    case gaussianBlur = 5
    case invert = 6
    case autoContrast = 7
    case brightnessContrast = 8
    case toneCurve = 9
    case levels = 10
    case hsv = 11
    case colorBalance = 12
    case unsharpMask = 13
}

public enum CoreFilterChannel: UInt32, CaseIterable, Equatable, Sendable {
    case rgb = 1
    case red = 2
    case green = 3
    case blue = 4
}

public enum CoreCurveInterpolation: UInt32, CaseIterable, Equatable, Sendable {
    case bezier = 1
    case bSpline = 2
}

public struct CoreCurvePoint: Equatable, Sendable {
    public var input: UInt32
    public var output: UInt32

    public init(input: UInt32, output: UInt32) {
        self.input = input
        self.output = output
    }
}

public struct CoreFilterRequest: Equatable, Sendable {
    public var kind: CoreFilterKind
    public var planeID: UInt64
    public var channel: CoreFilterChannel
    public var interpolation: CoreCurveInterpolation
    public var parameters: [Int32]
    public var curvePoints: [CoreCurvePoint]

    public init(
        kind: CoreFilterKind,
        planeID: UInt64,
        channel: CoreFilterChannel = .rgb,
        interpolation: CoreCurveInterpolation = .bezier,
        parameters: [Int32] = [],
        curvePoints: [CoreCurvePoint] = []
    ) {
        self.kind = kind
        self.planeID = planeID
        self.channel = channel
        self.interpolation = interpolation
        self.parameters = parameters
        self.curvePoints = curvePoints
    }

    var isValid: Bool {
        planeID != 0 && parameters.count <= 5 && curvePoints.count <= 4_096
            && curvePoints.allSatisfy { $0.input <= 65_535 && $0.output <= 65_535 }
    }
}

public struct CoreFilterPreviewProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let planeID: UInt64
    public let baseChecksum: UInt64
    public let previewChecksum: UInt64
    public let previewRevision: UInt64
}

public enum CoreGeometryPrimitive: UInt32, CaseIterable, Equatable, Sendable {
    case line = 1
    case curve = 2
    case rectangle = 3
    case ellipse = 4
    case polygon = 5
    case polyline = 6
}

public struct CoreGeometryPoint: Equatable, Sendable {
    public var x: Float
    public var y: Float

    public init(x: Float, y: Float) {
        self.x = x
        self.y = y
    }

    var isValid: Bool { x.isFinite && y.isFinite }
}

public struct CoreGeometryOptions: Equatable, Sendable {
    public var outline: Bool
    public var fill: Bool
    public var closePath: Bool
    public var bezierSegments: Bool
    public var constrainTo45Degrees: Bool
    public var fromCenter: Bool
    public var taperStart: Bool
    public var taperEnd: Bool
    public var squareCrossSection: Bool

    public init(
        outline: Bool = true,
        fill: Bool = false,
        closePath: Bool = false,
        bezierSegments: Bool = false,
        constrainTo45Degrees: Bool = false,
        fromCenter: Bool = false,
        taperStart: Bool = false,
        taperEnd: Bool = false,
        squareCrossSection: Bool = false
    ) {
        self.outline = outline
        self.fill = fill
        self.closePath = closePath
        self.bezierSegments = bezierSegments
        self.constrainTo45Degrees = constrainTo45Degrees
        self.fromCenter = fromCenter
        self.taperStart = taperStart
        self.taperEnd = taperEnd
        self.squareCrossSection = squareCrossSection
    }

    var featureFlags: UInt64 {
        (outline ? 1 << 0 : 0) | (fill ? 1 << 1 : 0)
            | (closePath ? 1 << 2 : 0) | (bezierSegments ? 1 << 3 : 0)
            | (constrainTo45Degrees ? 1 << 4 : 0) | (fromCenter ? 1 << 5 : 0)
            | (taperStart ? 1 << 6 : 0) | (taperEnd ? 1 << 7 : 0)
            | (squareCrossSection ? 1 << 8 : 0)
    }
}

public struct CoreGeometryRequest: Equatable, Sendable {
    public var primitive: CoreGeometryPrimitive
    public var planeID: UInt64
    public var baseRevision: UInt64
    public var outlineColor: CoreColorValue
    public var fillColor: CoreColorValue
    public var outlineWidth: Float
    public var aspectRatioQ16: UInt32
    public var polygonSides: UInt32
    public var rotationTurns: UInt32
    public var options: CoreGeometryOptions
    public var points: [CoreGeometryPoint]

    public init(
        primitive: CoreGeometryPrimitive,
        planeID: UInt64,
        baseRevision: UInt64 = 0,
        outlineColor: CoreColorValue = .rgba8(red: 0, green: 0, blue: 0),
        fillColor: CoreColorValue = .rgba8(red: 0, green: 0, blue: 0, alpha: 0),
        outlineWidth: Float = 1,
        aspectRatioQ16: UInt32 = 65_536,
        polygonSides: UInt32 = 5,
        rotationTurns: UInt32 = 0,
        options: CoreGeometryOptions = .init(),
        points: [CoreGeometryPoint]
    ) {
        self.primitive = primitive
        self.planeID = planeID
        self.baseRevision = baseRevision
        self.outlineColor = outlineColor
        self.fillColor = fillColor
        self.outlineWidth = outlineWidth
        self.aspectRatioQ16 = aspectRatioQ16
        self.polygonSides = polygonSides
        self.rotationTurns = rotationTurns
        self.options = options
        self.points = points
    }

    var isValid: Bool {
        planeID != 0 && outlineWidth.isFinite && outlineWidth > 0
            && aspectRatioQ16 > 0 && (3 ... 256).contains(polygonSides)
            && !points.isEmpty && points.count <= 4_096 && points.allSatisfy(\.isValid)
            && outlineColor.hasValidNativeComponents && fillColor.hasValidNativeComponents
            && (options.outline || options.fill)
    }
}

public struct CoreGeometryPreviewProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let planeID: UInt64
    public let baseRevision: UInt64
    public let previewRevision: UInt64
}

public enum CoreVectorEraseMode: UInt32, CaseIterable, Equatable, Sendable {
    case partial = 1
    case toIntersection = 2
    case wholePath = 3
}

public enum CoreVectorWidthMode: UInt32, CaseIterable, Equatable, Sendable {
    case add = 1
    case subtract = 2
    case scale = 3
    case constant = 4
}

public enum CoreVectorSelectionMode: UInt32, CaseIterable, Equatable, Sendable {
    case cutBySelection = 1
    case touching = 2
    case fullyContained = 3
    case line = 4
    case wholeLine = 5
    case toIntersection = 6
    case fillBoundary = 7
    case fill = 8
}

public struct CoreVectorSelectionRange: Equatable, Sendable {
    public let pathID: UInt64
    public let startMillion: UInt32
    public let endMillion: UInt32
}

public struct CoreVectorSelectionProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let ranges: [CoreVectorSelectionRange]
    public let fillIDs: [UInt64]
}

public enum CoreVectorCommand: Equatable, Sendable {
    case erase(planeID: UInt64, x: Float, y: Float, radius: Float, mode: CoreVectorEraseMode)
    case connect(planeID: UInt64, maximumGap: Float)
    case correctWidth(pathIDs: [UInt64], mode: CoreVectorWidthMode, parameter: Float)
    case select(mode: CoreVectorSelectionMode, bounds: CoreFrameRect)
    case rasterizeToLayer(layerID: UInt64, scale: UInt32, antialias: Bool, name: String)
    case vectorize(sourcePlaneID: UInt64, targetLayerID: UInt64, alphaThreshold: UInt32)

    var isValid: Bool {
        switch self {
        case let .erase(planeID, x, y, radius, _):
            planeID != 0 && x.isFinite && y.isFinite && radius.isFinite && radius > 0
        case let .connect(planeID, gap):
            planeID != 0 && gap.isFinite && gap >= 0
        case let .correctWidth(ids, _, parameter):
            !ids.isEmpty && ids.count <= 4_096 && ids.allSatisfy { $0 != 0 }
                && parameter.isFinite
        case let .select(_, bounds):
            bounds.width > 0 && bounds.height > 0
        case let .rasterizeToLayer(layerID, scale, _, name):
            layerID != 0 && (1 ... 16).contains(scale) && !name.isEmpty
                && name.utf8.count <= 4_096
        case let .vectorize(source, _, threshold):
            source != 0 && threshold <= 65_535
        }
    }
}

public struct CoreGradientStop: Equatable, Sendable {
    public var positionMilli: UInt32
    public var color: CoreColorValue

    public init(positionMilli: UInt32, color: CoreColorValue) {
        self.positionMilli = positionMilli
        self.color = color
    }
}

public enum CoreGradientKind: UInt32, CaseIterable, Equatable, Sendable {
    case linear = 1
    case radial = 2
}

public enum CoreGradientMode: UInt32, CaseIterable, Equatable, Sendable {
    case composite = 1
    case overwrite = 2
}

public struct CoreGradientRequest: Equatable, Sendable {
    public var planeID: UInt64
    public var kind: CoreGradientKind
    public var mode: CoreGradientMode
    public var dither: Bool
    public var constrainTo45Degrees: Bool
    public var startX: Double
    public var startY: Double
    public var endX: Double
    public var endY: Double
    public var stops: [CoreGradientStop]

    public init(
        planeID: UInt64,
        kind: CoreGradientKind = .linear,
        mode: CoreGradientMode = .composite,
        dither: Bool = false,
        constrainTo45Degrees: Bool = false,
        startX: Double,
        startY: Double,
        endX: Double,
        endY: Double,
        stops: [CoreGradientStop]
    ) {
        self.planeID = planeID
        self.kind = kind
        self.mode = mode
        self.dither = dither
        self.constrainTo45Degrees = constrainTo45Degrees
        self.startX = startX
        self.startY = startY
        self.endX = endX
        self.endY = endY
        self.stops = stops
    }

    var isValid: Bool {
        planeID != 0 && [startX, startY, endX, endY].allSatisfy(\.isFinite)
            && (2 ... 64).contains(stops.count)
            && stops.allSatisfy { $0.positionMilli <= 1_000 && $0.color.hasValidNativeComponents }
            && stops.map(\.positionMilli) == stops.map(\.positionMilli).sorted()
    }
}

public enum CoreDustMode: UInt32, CaseIterable, Equatable, Sendable {
    case removeForeground = 1
    case fillTransparentHoles = 2
    case replaceColorOutliers = 3
}

public enum CoreEffectCommand: Equatable, Sendable {
    case gradient(CoreGradientRequest, alphaOnly: Bool)
    case airbrush(planeID: UInt64, x: Double, y: Double, radius: Double, hardnessMilli: UInt32, opacityMilli: UInt32, color: CoreColorValue)
    case boundaryAirbrush(planeID: UInt64, width: UInt32, strengthMilli: UInt32, colors: [CoreColorValue])
    case blur(planeID: UInt64, radius: UInt32, strengthMilli: UInt32)
    case stamp(planeID: UInt64, sourceX: Int32, sourceY: Int32, destinationX: Int32, destinationY: Int32, width: UInt32, height: UInt32, opacityMilli: UInt32)
    case dust(planeID: UInt64, mode: CoreDustMode, maximumPixels: UInt32)

    var isValid: Bool {
        switch self {
        case let .gradient(input, _): input.isValid
        case let .airbrush(planeID, x, y, radius, hardness, opacity, color):
            planeID != 0 && x.isFinite && y.isFinite && radius.isFinite && radius > 0
                && hardness <= 1_000 && opacity <= 1_000 && color.hasValidNativeComponents
        case let .boundaryAirbrush(planeID, width, strength, colors):
            planeID != 0 && width > 0 && strength <= 1_000 && !colors.isEmpty
                && colors.count <= 256 && colors.allSatisfy(\.hasValidNativeComponents)
        case let .blur(planeID, radius, strength):
            planeID != 0 && radius > 0 && strength <= 1_000
        case let .stamp(planeID, _, _, _, _, width, height, opacity):
            planeID != 0 && width > 0 && height > 0 && opacity <= 1_000
        case let .dust(planeID, _, maximum):
            planeID != 0 && maximum > 0
        }
    }
}

public enum CoreAnnotationKind: UInt32, CaseIterable, Equatable, Sendable {
    case text = 1
    case stroke = 2
    case leader = 3
    case value = 4
}

public enum CoreAnnotationOutput: UInt32, CaseIterable, Equatable, Sendable {
    case normal = 1
    case instruction = 2
}

public struct CoreAnnotationPoint: Equatable, Sendable {
    public var xMilli: Int32
    public var yMilli: Int32

    public init(xMilli: Int32, yMilli: Int32) {
        self.xMilli = xMilli
        self.yMilli = yMilli
    }
}

public struct CoreAnnotationObject: Equatable, Sendable {
    public var kind: CoreAnnotationKind
    public var layerID: UInt64
    public var output: CoreAnnotationOutput
    public var bounds: CoreFrameRect
    public var fontFamily: String
    public var fontSizeMilli: UInt32
    public var strokeWidthMilli: UInt32
    public var color: CoreColorValue
    public var text: String
    public var points: [CoreAnnotationPoint]
    public var bold: Bool
    public var italic: Bool
    public var underline: Bool

    public init(
        kind: CoreAnnotationKind,
        layerID: UInt64,
        output: CoreAnnotationOutput,
        bounds: CoreFrameRect,
        fontFamily: String = "",
        fontSizeMilli: UInt32? = nil,
        strokeWidthMilli: UInt32? = nil,
        color: CoreColorValue = .rgba8(red: 0, green: 0, blue: 0),
        text: String = "",
        points: [CoreAnnotationPoint] = [],
        bold: Bool = false,
        italic: Bool = false,
        underline: Bool = false
    ) {
        self.kind = kind
        self.layerID = layerID
        self.output = output
        self.bounds = bounds
        self.fontFamily = fontFamily
        self.fontSizeMilli = fontSizeMilli ?? (kind == .text || kind == .value ? 12_000 : 0)
        self.strokeWidthMilli = strokeWidthMilli ?? (kind == .text ? 0 : 1_000)
        self.color = color
        self.text = text
        self.points = points
        self.bold = bold
        self.italic = italic
        self.underline = underline
    }

    var isValid: Bool {
        guard layerID != 0, bounds.width >= 0, bounds.height >= 0,
              fontFamily.utf8.count <= 1_024, text.utf8.count <= 1_048_576,
              points.count <= 65_536, color.hasValidNativeComponents
        else { return false }
        let hasStyle = bold || italic || underline
        return switch kind {
        case .text:
            !text.isEmpty && points.isEmpty && fontSizeMilli > 0
                && fontSizeMilli <= 1_000_000 && strokeWidthMilli == 0
        case .stroke:
            text.isEmpty && fontFamily.isEmpty && fontSizeMilli == 0 && !hasStyle
                && points.count >= 2 && strokeWidthMilli > 0
                && strokeWidthMilli <= 1_000_000
        case .leader:
            text.isEmpty && fontFamily.isEmpty && fontSizeMilli == 0 && !hasStyle
                && points.count == 2 && strokeWidthMilli > 0
                && strokeWidthMilli <= 1_000_000
        case .value:
            !text.isEmpty && fontSizeMilli > 0 && fontSizeMilli <= 1_000_000
                && points.count == 2 && strokeWidthMilli > 0
                && strokeWidthMilli <= 1_000_000
        }
    }
}

public enum CoreAnnotationEdit: Equatable, Sendable {
    case create(CoreAnnotationObject)
    case update(id: UInt64, object: CoreAnnotationObject)
    case move(id: UInt64, deltaX: Int32, deltaY: Int32)
    case delete(id: UInt64)

    var isValid: Bool {
        switch self {
        case let .create(object): object.isValid
        case let .update(id, object): id != 0 && object.isValid
        case let .move(id, _, _), let .delete(id): id != 0
        }
    }
}

public enum CoreShootingFrameAnchor: UInt32, CaseIterable, Equatable, Sendable {
    case topLeft = 1
    case topRight = 2
    case center = 3
    case bottomLeft = 4
    case bottomRight = 5
}

public struct CoreShootingFrame: Equatable, Sendable {
    public var id: UInt64
    public var anchor: CoreShootingFrameAnchor
    public var centerX: Double
    public var centerY: Double
    public var width: Double
    public var height: Double
    public var rotationDegrees: Double
    public var visible: Bool
    public var includeInInstructionExport: Bool

    public init(
        id: UInt64 = 0,
        anchor: CoreShootingFrameAnchor = .center,
        centerX: Double,
        centerY: Double,
        width: Double,
        height: Double,
        rotationDegrees: Double = 0,
        visible: Bool = true,
        includeInInstructionExport: Bool = true
    ) {
        self.id = id
        self.anchor = anchor
        self.centerX = centerX
        self.centerY = centerY
        self.width = width
        self.height = height
        self.rotationDegrees = rotationDegrees
        self.visible = visible
        self.includeInInstructionExport = includeInInstructionExport
    }

    var isValid: Bool {
        [centerX, centerY, width, height, rotationDegrees].allSatisfy(\.isFinite)
            && width > 0 && height > 0
    }
}

public struct CoreShootingFrameProjection: Equatable, Sendable {
    public let id: UInt64
    public let anchor: CoreShootingFrameAnchor
    public let centerXMilli: Int64
    public let centerYMilli: Int64
    public let widthMilli: UInt64
    public let heightMilli: UInt64
    public let rotationTurns: UInt32
    public let visible: Bool
    public let includeInInstructionExport: Bool
    public let corners: [(Int64, Int64)]

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.id == rhs.id && lhs.anchor == rhs.anchor
            && lhs.centerXMilli == rhs.centerXMilli && lhs.centerYMilli == rhs.centerYMilli
            && lhs.widthMilli == rhs.widthMilli && lhs.heightMilli == rhs.heightMilli
            && lhs.rotationTurns == rhs.rotationTurns && lhs.visible == rhs.visible
            && lhs.includeInInstructionExport == rhs.includeInInstructionExport
            && lhs.corners.elementsEqual(rhs.corners, by: ==)
    }
}

public struct CoreVanishingPoint: Equatable, Sendable {
    public var id: UInt64
    public var layerID: UInt64
    public var xMilli: Int64
    public var yMilli: Int64
    public var intervalMilliDegrees: UInt32
    public var angleMilliDegrees: UInt32
    public var opacityMilli: UInt32
    public var visible: Bool
    public var color: CoreColorValue

    public init(
        id: UInt64 = 0,
        layerID: UInt64,
        xMilli: Int64,
        yMilli: Int64,
        intervalMilliDegrees: UInt32 = 15_000,
        angleMilliDegrees: UInt32 = 0,
        opacityMilli: UInt32 = 500,
        visible: Bool = true,
        color: CoreColorValue = .rgba8(red: 0, green: 128, blue: 255)
    ) {
        self.id = id
        self.layerID = layerID
        self.xMilli = xMilli
        self.yMilli = yMilli
        self.intervalMilliDegrees = intervalMilliDegrees
        self.angleMilliDegrees = angleMilliDegrees
        self.opacityMilli = opacityMilli
        self.visible = visible
        self.color = color
    }

    var isValid: Bool {
        layerID != 0 && intervalMilliDegrees > 0 && intervalMilliDegrees <= 360_000
            && angleMilliDegrees < 360_000 && opacityMilli <= 1_000
            && color.hasValidNativeComponents
    }
}

public struct CoreM8Projection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let shootingFrame: CoreShootingFrameProjection?
    public let vanishingPoints: [CoreVanishingPoint]
}

public struct CoreM8MutationProjection: Equatable, Sendable {
    public let state: CoreM8Projection
    public let createdIDs: [UInt64]
}

public enum CoreM8Command: Equatable, Sendable {
    case beginFilterPreview(CoreFilterRequest)
    case updateFilterPreview(CoreFilterRequest)
    case cancelFilterPreview
    case applyFilterPreview
    case applyLastFilter(planeID: UInt64)
    case createAdjustment(CoreFilterRequest, name: String)
    case updateAdjustment(layerID: UInt64, filter: CoreFilterRequest)
    case beginGeometryPreview(CoreGeometryRequest)
    case updateGeometryPreview(CoreGeometryRequest)
    case cancelGeometryPreview
    case commitGeometryPreview
    case applyGeometry(CoreGeometryRequest)
    case vector(CoreVectorCommand)
    case effect(CoreEffectCommand)
    case annotation([CoreAnnotationEdit])
    case shootingFrameCreate(CoreShootingFrame, preview: Bool)
    case shootingFrameUpdate(CoreShootingFrame, preview: Bool)
    case shootingFrameDelete(id: UInt64)
    case shootingFramePreviewUpdate(CoreShootingFrame)
    case shootingFramePreviewApply
    case shootingFramePreviewCancel
    case vanishingPointCreate(CoreVanishingPoint, preview: Bool)
    case vanishingPointUpdate(CoreVanishingPoint, preview: Bool)
    case vanishingPointDelete(id: UInt64)
    case vanishingPointDeleteAll
    case vanishingPointPreviewUpdate(CoreVanishingPoint)
    case vanishingPointPreviewApply
    case vanishingPointPreviewCancel

    var isValid: Bool {
        switch self {
        case let .beginFilterPreview(input), let .updateFilterPreview(input),
             let .createAdjustment(input, _), let .updateAdjustment(_, input):
            input.isValid
        case let .applyLastFilter(planeID): planeID != 0
        case let .beginGeometryPreview(input), let .updateGeometryPreview(input),
             let .applyGeometry(input): input.isValid
        case let .vector(command): command.isValid
        case let .effect(command): command.isValid
        case let .annotation(edits):
            !edits.isEmpty && edits.count <= 4_096 && edits.allSatisfy(\.isValid)
        case let .shootingFrameCreate(frame, _), let .shootingFrameUpdate(frame, _),
             let .shootingFramePreviewUpdate(frame): frame.isValid
        case let .shootingFrameDelete(id): id != 0
        case let .vanishingPointCreate(point, _), let .vanishingPointUpdate(point, _),
             let .vanishingPointPreviewUpdate(point): point.isValid
        case let .vanishingPointDelete(id): id != 0
        case .cancelFilterPreview, .applyFilterPreview, .cancelGeometryPreview,
             .commitGeometryPreview, .shootingFramePreviewApply,
             .shootingFramePreviewCancel, .vanishingPointDeleteAll,
             .vanishingPointPreviewApply, .vanishingPointPreviewCancel:
            true
        }
    }
}
