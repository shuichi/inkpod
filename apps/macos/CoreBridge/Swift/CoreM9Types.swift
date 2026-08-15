import Foundation

public struct CoreCutID: Hashable, Comparable, Sendable {
    public let rawValue: UInt64
    public init(rawValue: UInt64) { self.rawValue = rawValue }
    public static func < (lhs: Self, rhs: Self) -> Bool { lhs.rawValue < rhs.rawValue }
}

public struct CoreCutGeneration: Hashable, Sendable {
    public let rawValue: UInt64
    public init(rawValue: UInt64) { self.rawValue = rawValue }
}

public struct CoreCutTarget: Hashable, Sendable {
    public let id: CoreCutID
    public let generation: CoreCutGeneration

    public init(id: CoreCutID, generation: CoreCutGeneration) {
        self.id = id
        self.generation = generation
    }
}

public struct CoreCutUUID: Hashable, Sendable {
    public let high: UInt64
    public let low: UInt64

    public init(high: UInt64, low: UInt64) {
        self.high = high
        self.low = low
    }

    var isValid: Bool { high != 0 || low != 0 }
}

public struct CoreCutMetadata: Equatable, Sendable {
    public var workTitle: String
    public var episode: String
    public var scene: String
    public var cutName: String
    public var instruction: String
    public var durationFrames: UInt32

    public init(
        workTitle: String = "",
        episode: String = "",
        scene: String = "",
        cutName: String,
        instruction: String = "",
        durationFrames: UInt32 = 1
    ) {
        self.workTitle = workTitle
        self.episode = episode
        self.scene = scene
        self.cutName = cutName
        self.instruction = instruction
        self.durationFrames = durationFrames
    }

    var isValid: Bool {
        durationFrames > 0
            && [workTitle, episode, scene, cutName, instruction].allSatisfy {
                $0.utf8.count <= 4_096 && !$0.utf8.contains(0)
            }
            && !cutName.isEmpty
    }
}

public struct CoreCutDefaults: Equatable, Sendable {
    public var sizingMode: UInt32
    public var width: UInt32
    public var height: UInt32
    public var dpiXMilli: UInt32
    public var dpiYMilli: UInt32
    public var marginMilli: UInt32
    public var safeFrameRatioMilli: UInt32
    public var maximumCloseRatioMilli: UInt32
    public var anchor: UInt32
    public var initialLayerKind: CoreLayerKind
    public var pixelFormat: CorePixelStorageFormat

    public init(
        sizingMode: UInt32 = 1,
        width: UInt32 = 1_920,
        height: UInt32 = 1_080,
        dpiXMilli: UInt32 = 72_000,
        dpiYMilli: UInt32 = 72_000,
        marginMilli: UInt32 = 0,
        safeFrameRatioMilli: UInt32 = 900,
        maximumCloseRatioMilli: UInt32 = 950,
        anchor: UInt32 = 3,
        initialLayerKind: CoreLayerKind = .raster,
        pixelFormat: CorePixelStorageFormat = .rgba8
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
    }

    var isValid: Bool {
        CoreCellSizingMode(rawValue: sizingMode) != nil
            && CoreFrameAnchor(rawValue: anchor) != nil
            && (pixelFormat == .rgba8 || pixelFormat == .rgba16)
            && width > 0 && height > 0 && dpiXMilli > 0 && dpiYMilli > 0
            && safeFrameRatioMilli <= 1_000 && maximumCloseRatioMilli <= 1_000
    }

    func cellCreationOptions(count: UInt32) -> CoreCellCreationOptions? {
        guard isValid,
              let sizingMode = CoreCellSizingMode(rawValue: sizingMode),
              let anchor = CoreFrameAnchor(rawValue: anchor),
              (1 ... 64).contains(count)
        else { return nil }
        return CoreCellCreationOptions(
            sizingMode: sizingMode,
            width: width,
            height: height,
            dpiXMilli: dpiXMilli,
            dpiYMilli: dpiYMilli,
            marginMilli: marginMilli,
            safeFrameRatioMilli: safeFrameRatioMilli,
            maximumCloseRatioMilli: maximumCloseRatioMilli,
            anchor: anchor,
            initialLayerKind: initialLayerKind,
            pixelFormat: pixelFormat,
            count: count
        )
    }
}

public struct CoreCutMember: Equatable, Sendable {
    public let displayNumber: UInt32
    public let cellID: UInt64
    public let documentUUID: CoreDocumentUUID
    public let relativePath: String

    public init(
        displayNumber: UInt32,
        cellID: UInt64,
        documentUUID: CoreDocumentUUID,
        relativePath: String
    ) {
        self.displayNumber = displayNumber
        self.cellID = cellID
        self.documentUUID = documentUUID
        self.relativePath = relativePath
    }

    var isValid: Bool {
        displayNumber > 0 && cellID > 0 && documentUUID.isValid
            && !relativePath.isEmpty && relativePath.utf8.count <= 4_096
            && !relativePath.utf8.contains(0)
            && !relativePath.contains("/") && !relativePath.contains("\\")
            && relativePath != "." && relativePath != ".."
    }
}

public enum CoreCutSequenceOperation: Equatable, Sendable {
    case insert(CoreCutMember, position: UInt32)
    case remove(cellID: UInt64, documentUUID: CoreDocumentUUID)
    case moveBefore(cellID: UInt64, documentUUID: CoreDocumentUUID, anchorCellID: UInt64, anchorDocumentUUID: CoreDocumentUUID)
    case moveAfter(cellID: UInt64, documentUUID: CoreDocumentUUID, anchorCellID: UInt64, anchorDocumentUUID: CoreDocumentUUID)
    case renumber(position: UInt32, count: UInt32, first: UInt32, step: UInt32)
}

public struct CoreCutProjection: Equatable, Sendable {
    public let target: CoreCutTarget
    public let cutID: UInt64
    public let cutUUID: CoreCutUUID
    public let revision: UInt64
    public let stateID: UInt64
    public let metadata: CoreCutMetadata
    public let defaults: CoreCutDefaults
    public let members: [CoreCutMember]
    public let isDirty: Bool
    public let canUndo: Bool
    public let canRedo: Bool
    public let isRecovered: Bool
    public let ownerThreadID: UInt64
}

public struct CoreCutMutationProjection: Equatable, Sendable {
    public let cut: CoreCutProjection
    public let applied: Bool
    public let failedOperationIndex: UInt32?
}

public struct CoreRGBA8Source: Equatable, Sendable {
    public let name: String
    public let documentUUID: CoreDocumentUUID
    public let sourceGeneration: UInt64
    public let width: UInt32
    public let height: UInt32
    public let dpiXMilli: UInt32
    public let dpiYMilli: UInt32
    public let rgba8: [UInt8]

    public init(
        name: String,
        documentUUID: CoreDocumentUUID,
        sourceGeneration: UInt64,
        width: UInt32,
        height: UInt32,
        dpiXMilli: UInt32 = 72_000,
        dpiYMilli: UInt32 = 72_000,
        rgba8: [UInt8]
    ) {
        self.name = name
        self.documentUUID = documentUUID
        self.sourceGeneration = sourceGeneration
        self.width = width
        self.height = height
        self.dpiXMilli = dpiXMilli
        self.dpiYMilli = dpiYMilli
        self.rgba8 = rgba8
    }

    var isValid: Bool {
        let pixels = UInt64(width).multipliedReportingOverflow(by: UInt64(height))
        let bytes = pixels.partialValue.multipliedReportingOverflow(by: 4)
        return !name.isEmpty && name.utf8.count <= 4_096 && !name.utf8.contains(0)
            && documentUUID.isValid && sourceGeneration > 0 && width > 0 && height > 0
            && width <= UInt32(Int32.max) && height <= UInt32(Int32.max)
            && dpiXMilli > 0 && dpiYMilli > 0 && !pixels.overflow && !bytes.overflow
            && bytes.partialValue <= UInt64(Int.max) && rgba8.count == Int(bytes.partialValue)
    }
}

public struct CoreNamedRaster: Equatable, Sendable {
    public let name: String
    public let format: CoreCommonRasterFormat
    public let bytes: [UInt8]

    public init(name: String, format: CoreCommonRasterFormat, bytes: [UInt8]) {
        self.name = name
        self.format = format
        self.bytes = bytes
    }
}

public struct CoreIdentifiedNamedRaster: Equatable, Sendable {
    public let raster: CoreNamedRaster
    public let documentUUID: CoreDocumentUUID
    public let sourceGeneration: UInt64

    public init(
        raster: CoreNamedRaster,
        documentUUID: CoreDocumentUUID,
        sourceGeneration: UInt64
    ) {
        self.raster = raster
        self.documentUUID = documentUUID
        self.sourceGeneration = sourceGeneration
    }

    var isValid: Bool {
        !raster.name.isEmpty && raster.name.utf8.count <= 4_096
            && !raster.bytes.isEmpty && raster.bytes.count <= 512 * 1_024 * 1_024
            && documentUUID.isValid && sourceGeneration > 0
    }
}

public struct CoreSequenceCellProjection: Equatable, Sendable {
    public let index: UInt32
    public let documentUUID: CoreDocumentUUID
    public let sourceGeneration: UInt64
    public let cellNumber: UInt32
    public let name: String
    public let width: UInt32
    public let height: UInt32
    public let thumbnailWidth: UInt32
    public let thumbnailHeight: UInt32
    public let thumbnailChecksum: UInt64
    public let thumbnailRGBA8: [UInt8]
}

public enum CoreSequenceDirection: UInt32, Equatable, Sendable {
    case previous = 1
    case next = 2
}

public enum CoreSequenceEndpointPolicy: UInt32, Equatable, Sendable, Codable {
    case stop = 1
    case wrap = 2
}

public enum CoreSequenceStepResult: UInt32, Equatable, Sendable {
    case empty = 1
    case singleCell = 2
    case stopped = 3
    case advanced = 4
    case wrapped = 5
}

public struct CoreSequenceStepPlan: Equatable, Sendable {
    public let direction: CoreSequenceDirection
    public let endpointPolicy: CoreSequenceEndpointPolicy
    public let result: CoreSequenceStepResult
    public let sequenceRevision: UInt64
    public let sourceDocumentUUID: CoreDocumentUUID?
    public let sourceGeneration: UInt64
    public let targetDocumentUUID: CoreDocumentUUID?
    public let targetGeneration: UInt64
    public let sourceIndex: UInt32?
    public let targetIndex: UInt32?
    public let sourceCellNumber: UInt32
    public let targetCellNumber: UInt32
}

public enum CoreLightTableDisplayMode: UInt32, Equatable, Sendable {
    case color = 1
    case monotone = 2
    case halftone = 3
}

public struct CoreLightTableSetProjection: Equatable, Sendable {
    public let id: UInt64
    public let name: String
    public let opacityMilli: UInt32
    public let isActive: Bool
    public let items: [CoreLightTableItemProjection]
}

public struct CoreLightTableItemProjection: Equatable, Sendable {
    public let id: UInt64
    public let name: String
    public let sourcePlaneID: UInt64
    public let sourceDocumentUUID: CoreDocumentUUID
    public let sourceRevision: UInt64
    public let opacityMilli: UInt32
    public let effectiveOpacityMilli: UInt32
    public let displayMode: CoreLightTableDisplayMode
    public let displayColor: CoreColorValue
    public let translateXMilli: Int32
    public let translateYMilli: Int32
    public let scaleXMilli: UInt32
    public let scaleYMilli: UInt32
    public let rotationMilliDegrees: Int32
    public let isVisible: Bool
}

public struct CoreLightTableItemSource: Equatable, Sendable {
    public let source: CoreRGBA8Source
    public let opacityMilli: UInt32
    public let displayMode: CoreLightTableDisplayMode
    public let displayColor: CoreColorValue
    public let isVisible: Bool

    public init(
        source: CoreRGBA8Source,
        opacityMilli: UInt32 = 500,
        displayMode: CoreLightTableDisplayMode = .color,
        displayColor: CoreColorValue = .rgba8(red: 255, green: 128, blue: 128),
        isVisible: Bool = true
    ) {
        self.source = source
        self.opacityMilli = opacityMilli
        self.displayMode = displayMode
        self.displayColor = displayColor
        self.isVisible = isVisible
    }
}

public enum CoreLightTableEditCommand: Equatable, Sendable {
    case createSet(name: String)
    case duplicateSet(id: UInt64, name: String)
    case deleteSet(id: UInt64)
    case renameSet(id: UInt64, name: String)
    case reorderSet(id: UInt64, destinationIndex: UInt32)
    case activateSet(id: UInt64)
    case removeItem(id: UInt64)
    case reorderItem(id: UInt64, destinationIndex: UInt32)
    case updateItem(id: UInt64, name: String, opacityMilli: UInt32, displayMode: CoreLightTableDisplayMode, displayColor: CoreColorValue, isVisible: Bool, translateXMilli: Int32, translateYMilli: Int32, scaleXMilli: UInt32, scaleYMilli: UInt32, rotationMilliDegrees: Int32)
    case setGlobalOpacity(UInt32)
}

public enum CoreLightTableBulkDirection: UInt32, Equatable, Sendable {
    case previous = 1
    case next = 2
    case both = 3
}

public struct CoreLightTableBulkRequest: Equatable, Sendable {
    public let targetSetID: UInt64
    public let direction: CoreLightTableBulkDirection
    public let neighborCount: UInt32
    public let baseOpacityMilli: UInt32
    public let distanceStepMilli: UInt32
    public let baseDocumentRevision: UInt64
    public let sequenceRevision: UInt64
    public let activeDocumentUUID: CoreDocumentUUID
    public let activeSourceGeneration: UInt64
}

public enum CoreLightTableBulkAction: UInt32, Equatable, Sendable {
    case add = 1
    case skipExisting = 2
}

public struct CoreLightTableBulkEntry: Equatable, Sendable {
    public let action: CoreLightTableBulkAction
    public let sequenceIndex: UInt32
    public let cellNumber: UInt32
    public let distance: UInt32
    public let opacityMilli: UInt32
    public let documentUUID: CoreDocumentUUID
    public let sourceGeneration: UInt64
    public let existingSourceRevision: UInt64?
}

public struct CoreLightTableBulkPreview: Equatable, Sendable {
    public let request: CoreLightTableBulkRequest
    public let entries: [CoreLightTableBulkEntry]
    public let addCount: UInt32
    public let skipCount: UInt32
}

public struct CoreMotionProjection: Equatable, Sendable {
    public let sequenceIndex: UInt64
    public let cellNumber: UInt32
    public let thumbnailWidth: UInt32
    public let thumbnailHeight: UInt32
    public let thumbnailChecksum: UInt64
    public let isPaused: Bool
    public let includesSelection: Bool
    public let includesLightTable: Bool
}

public struct CoreAnimationProjection: Equatable, Sendable {
    public let session: CoreSessionProjection
    public let sequence: [CoreSequenceCellProjection]
    public let activeSequenceIndex: UInt32?
    public let lightTableSets: [CoreLightTableSetProjection]
    public let motion: CoreMotionProjection?
}

public struct CoreAnimationMutationProjection: Equatable, Sendable {
    public let state: CoreAnimationProjection
    public let createdIDs: [UInt64]
    public let applied: Bool
}

public struct CoreSequenceExportItem: Equatable, Sendable {
    public let name: String
    public let bytes: [UInt8]
}

public enum CoreAnimationCommand: Equatable, Sendable {
    case replaceSequence([CoreRGBA8Source])
    case importSequence([CoreNamedRaster])
    case importIdentifiedSequence([CoreIdentifiedNamedRaster])
    case exportSequence(CoreCommonRasterFormat, compositeWhite: Bool)
    case activateSequence(UInt32)
    case resolveStep(CoreSequenceDirection, CoreSequenceEndpointPolicy)
    case commitStep(CoreSequenceStepPlan)
    case addLightTableItem(CoreLightTableItemSource)
    case addLightTableRaster(CoreNamedRaster, documentUUID: CoreDocumentUUID, sourceRevision: UInt64)
    case reloadLightTableRaster(itemID: UInt64, raster: CoreNamedRaster, documentUUID: CoreDocumentUUID, sourceRevision: UInt64)
    case editLightTable(CoreLightTableEditCommand)
    case previewLightTableBulk(setID: UInt64, direction: CoreLightTableBulkDirection, neighborCount: UInt32, baseOpacityMilli: UInt32, distanceStepMilli: UInt32)
    case registerLightTableBulk(CoreLightTableBulkRequest)
    case sampleLightTable(x: UInt32, y: UInt32)
    case swapLightTable(itemID: UInt64)
    case setSubpalette(UInt32)
    case sampleSubpalette(x: UInt32, y: UInt32)
    case motionStart(fps: UInt32, loop: Bool, includeSelection: Bool, includeLightTable: Bool)
    case motionStep(CoreSequenceDirection)
    case motionTogglePause
    case motionStop
}
