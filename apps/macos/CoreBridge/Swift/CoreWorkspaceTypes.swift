import Foundation

public struct CoreCellCreationPlanID: RawRepresentable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public enum CoreCellSizingMode: UInt32, CaseIterable, Codable, Sendable {
    case imagePixels = 1
    case frameMicrometres = 2
}

public enum CoreFrameAnchor: UInt32, CaseIterable, Codable, Sendable {
    case topLeft = 1
    case topRight = 2
    case center = 3
    case bottomLeft = 4
    case bottomRight = 5
}

public enum CoreLayerKind: UInt32, CaseIterable, Codable, Sendable {
    case binaryColoring = 1
    case grayscaleColoring = 2
    case raster = 3
    case selection = 4
    case frame = 5
    case vanishingPoint = 6
    case adjustment = 7
    case text = 8
    case annotation = 9
    case vectorColoring = 10
}

public enum CorePlaneKind: UInt32, CaseIterable, Codable, Sendable {
    case mainLine = 1
    case color = 2
    case raster = 3
    case selection = 4
    case vectorMainLine = 5
    case colorTrace = 6
    case vectorFill = 7
}

public enum CorePixelStorageFormat: UInt32, CaseIterable, Codable, Sendable {
    case none = 0
    case binary8 = 1
    case grayscale8 = 2
    case grayscale16 = 3
    case rgba8 = 4
    case rgba16 = 5
}

public struct CoreCellCreationOptions: Equatable, Sendable {
    public let sizingMode: CoreCellSizingMode
    public let width: UInt32
    public let height: UInt32
    public let dpiXMilli: UInt32
    public let dpiYMilli: UInt32
    public let marginMilli: UInt32
    public let safeFrameRatioMilli: UInt32
    public let maximumCloseRatioMilli: UInt32
    public let anchor: CoreFrameAnchor
    public let initialLayerKind: CoreLayerKind
    public let pixelFormat: CorePixelStorageFormat
    public let count: UInt32

    public init(
        sizingMode: CoreCellSizingMode,
        width: UInt32,
        height: UInt32,
        dpiXMilli: UInt32,
        dpiYMilli: UInt32,
        marginMilli: UInt32,
        safeFrameRatioMilli: UInt32,
        maximumCloseRatioMilli: UInt32,
        anchor: CoreFrameAnchor,
        initialLayerKind: CoreLayerKind,
        pixelFormat: CorePixelStorageFormat,
        count: UInt32
    ) {
        self.sizingMode = sizingMode
        self.width = width
        self.height = height
        self.dpiXMilli = dpiXMilli
        self.dpiYMilli = dpiYMilli
        self.marginMilli = marginMilli
        self.safeFrameRatioMilli = safeFrameRatioMilli
        self.maximumCloseRatioMilli = maximumCloseRatioMilli
        self.anchor = anchor
        self.initialLayerKind = initialLayerKind
        self.pixelFormat = pixelFormat
        self.count = count
    }

    public static let defaultSingleCell = CoreCellCreationOptions(
        sizingMode: .imagePixels,
        width: 1_920,
        height: 1_080,
        dpiXMilli: 72_000,
        dpiYMilli: 72_000,
        marginMilli: 0,
        safeFrameRatioMilli: 900,
        maximumCloseRatioMilli: 1_000,
        anchor: .center,
        initialLayerKind: .binaryColoring,
        pixelFormat: .rgba8,
        count: 1
    )

    var hasValidFrontendBounds: Bool {
        width > 0 && height > 0 && dpiXMilli > 0 && dpiYMilli > 0
            && (1 ... 64).contains(count)
    }
}

public struct CoreFrameRect: Equatable, Codable, Sendable {
    public var x: Int32
    public var y: Int32
    public var width: Int32
    public var height: Int32

    public init(x: Int32, y: Int32, width: Int32, height: Int32) {
        self.x = x
        self.y = y
        self.width = width
        self.height = height
    }

    var hasPositiveSize: Bool { width > 0 && height > 0 }
}

public struct CoreCellCreationPlanItem: Equatable, Sendable {
    public let sizingMode: CoreCellSizingMode
    public let width: UInt32
    public let height: UInt32
    public let dpiXMilli: UInt32
    public let dpiYMilli: UInt32
    public let initialLayerKind: CoreLayerKind
    public let pixelFormat: CorePixelStorageFormat
    public let hundredFrame: CoreFrameRect
    public let referenceFrame: CoreFrameRect
    public let drawingFrame: CoreFrameRect
    public let safeFrame: CoreFrameRect
    public let shootingFrame: CoreFrameRect
    public let maximumCloseFrame: CoreFrameRect
    public let margins: CoreMargins
}

public struct CoreCellCreationPlanProjection: Equatable, Sendable {
    public let id: CoreCellCreationPlanID
    public let options: CoreCellCreationOptions
    public let items: [CoreCellCreationPlanItem]
}

public struct CoreMargins: Equatable, Codable, Sendable {
    public var left: UInt32
    public var top: UInt32
    public var right: UInt32
    public var bottom: UInt32

    public init(left: UInt32, top: UInt32, right: UInt32, bottom: UInt32) {
        self.left = left
        self.top = top
        self.right = right
        self.bottom = bottom
    }
}

public struct CorePaperFrames: Equatable, Sendable {
    public var hundred: CoreFrameRect
    public var reference: CoreFrameRect
    public var drawing: CoreFrameRect
    public var safe: CoreFrameRect
    public var shooting: CoreFrameRect
    public var maximumClose: CoreFrameRect
    public var margins: CoreMargins

    public init(
        hundred: CoreFrameRect,
        reference: CoreFrameRect,
        drawing: CoreFrameRect,
        safe: CoreFrameRect,
        shooting: CoreFrameRect,
        maximumClose: CoreFrameRect,
        margins: CoreMargins
    ) {
        self.hundred = hundred
        self.reference = reference
        self.drawing = drawing
        self.safe = safe
        self.shooting = shooting
        self.maximumClose = maximumClose
        self.margins = margins
    }
}

public enum CoreMirrorAxis: UInt32, Sendable {
    case horizontal = 1
    case vertical = 2
}

public enum CoreQuarterTurn: UInt32, Sendable {
    case left = 1
    case right = 2
}

public struct CoreDocumentResize: Equatable, Sendable {
    public let width: UInt32
    public let height: UInt32
    public let dpiXMilli: UInt32
    public let dpiYMilli: UInt32
    public let anchor: CoreFrameAnchor
    public let resample: Bool

    public init(
        width: UInt32,
        height: UInt32,
        dpiXMilli: UInt32,
        dpiYMilli: UInt32,
        anchor: CoreFrameAnchor,
        resample: Bool
    ) {
        self.width = width
        self.height = height
        self.dpiXMilli = dpiXMilli
        self.dpiYMilli = dpiYMilli
        self.anchor = anchor
        self.resample = resample
    }
}

public enum CoreCellEditCommand: Equatable, Sendable {
    case updatePaperFrames(CorePaperFrames)
    case mirror(CoreMirrorAxis)
    case rotate(CoreQuarterTurn)
    case resize(CoreDocumentResize)
    case fitPaperToFrames
}

public struct CoreLogicalViewProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let target: CoreViewTarget
    public let viewRevision: UInt64
}

public struct CoreNodeProjection: Equatable, Identifiable, Sendable {
    public let id: UInt64
    public let parentID: UInt64
    public let planeKind: CorePlaneKind
    public let pixelFormat: CorePixelStorageFormat
    public let opacityMilli: UInt32
    public let index: UInt32
    public let isVisible: Bool
    public let isEditable: Bool
    public let name: String

    public var kind: CorePlaneKind { planeKind }
}

public struct CoreLayerProjection: Equatable, Identifiable, Sendable {
    public let id: UInt64
    public let kind: CoreLayerKind
    public let pixelFormat: CorePixelStorageFormat
    public let opacityMilli: UInt32
    public let index: UInt32
    public let isVisible: Bool
    public let isEditable: Bool
    public let name: String
    public let planes: [CoreNodeProjection]
}

public struct CoreTreeProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let editorRevision: UInt64
    public let activeLayerID: UInt64
    public let activePlaneID: UInt64
    public let layers: [CoreLayerProjection]
}

public struct CoreTreeMutationProjection: Equatable, Sendable {
    public let tree: CoreTreeProjection
    public let affectedObjectID: UInt64?
}

public enum CoreTreeEditCommand: Equatable, Sendable {
    case createLayer(kind: CoreLayerKind, pixelFormat: CorePixelStorageFormat, name: String)
    case duplicateLayer(id: UInt64)
    case deleteLayer(id: UInt64)
    case reorderLayer(id: UInt64, destinationIndex: UInt32)
    case setLayerProperties(
        id: UInt64,
        visible: Bool,
        editable: Bool,
        opacityMilli: UInt32,
        name: String
    )
    case convertLayer(id: UInt64, kind: CoreLayerKind, pixelFormat: CorePixelStorageFormat)
    case mergeLayer(id: UInt64)
    case deleteHiddenLayers
    case createPlane(
        parentLayerID: UInt64,
        kind: CorePlaneKind,
        pixelFormat: CorePixelStorageFormat,
        name: String
    )
    case duplicatePlane(id: UInt64, parentLayerID: UInt64)
    case deletePlane(id: UInt64, parentLayerID: UInt64)
    case reorderPlane(id: UInt64, parentLayerID: UInt64, destinationIndex: UInt32)
    case setPlaneProperties(
        id: UInt64,
        parentLayerID: UInt64,
        visible: Bool,
        editable: Bool,
        opacityMilli: UInt32,
        name: String
    )
    case convertPlane(
        id: UInt64,
        parentLayerID: UInt64,
        kind: CorePlaneKind,
        pixelFormat: CorePixelStorageFormat
    )
    case mergePlane(id: UInt64, parentLayerID: UInt64)

    var hasValidFrontendBounds: Bool {
        func validName(_ name: String) -> Bool {
            !name.utf8.isEmpty && name.utf8.count <= 4_096
        }
        switch self {
        case let .createLayer(_, _, name), let .createPlane(_, _, _, name):
            return validName(name)
        case let .setLayerProperties(id, _, _, opacity, name):
            return id != 0 && opacity <= 1_000 && validName(name)
        case let .setPlaneProperties(id, parent, _, _, opacity, name):
            return id != 0 && parent != 0 && opacity <= 1_000 && validName(name)
        case let .duplicateLayer(id), let .deleteLayer(id), let .mergeLayer(id):
            return id != 0
        case let .reorderLayer(id, _):
            return id != 0
        case let .convertLayer(id, _, _):
            return id != 0
        case .deleteHiddenLayers:
            return true
        case let .duplicatePlane(id, parent), let .deletePlane(id, parent),
             let .mergePlane(id, parent):
            return id != 0 && parent != 0
        case let .reorderPlane(id, parent, _):
            return id != 0 && parent != 0
        case let .convertPlane(id, parent, _, _):
            return id != 0 && parent != 0
        }
    }
}
