import Foundation

public enum CoreBatchInputKind: UInt32, CaseIterable, Equatable, Sendable {
    case file = 1
    case folder = 2
    case currentSequence = 3
}

public struct CoreBatchInputSelector: Equatable, Sendable {
    public var kind: CoreBatchInputKind
    public var path: String
    public var firstCell: UInt32
    public var lastCell: UInt32

    public init(
        kind: CoreBatchInputKind,
        path: String = "",
        firstCell: UInt32 = 0,
        lastCell: UInt32 = 0
    ) {
        self.kind = kind
        self.path = path
        self.firstCell = firstCell
        self.lastCell = lastCell
    }

    public static func file(_ path: String) -> Self { Self(kind: .file, path: path) }
    public static func folder(_ path: String) -> Self { Self(kind: .folder, path: path) }
    public static func currentSequence() -> Self { Self(kind: .currentSequence) }

    var isValid: Bool {
        path.utf8.count <= 32_768 && !path.utf8.contains(0)
            && (firstCell == 0 || lastCell == 0 || firstCell <= lastCell)
            && (kind == .currentSequence ? path.isEmpty : !path.isEmpty)
    }
}

public enum CoreBatchMissingPolicy: UInt32, CaseIterable, Equatable, Sendable {
    case skip = 1
    case error = 2
}

public struct CoreBatchTargetSelector: Equatable, Sendable {
    public var layerID: UInt64?
    public var planeID: UInt64?
    public var layerKind: CoreLayerKind?
    public var planeKind: CorePlaneKind?
    public var missingPolicy: CoreBatchMissingPolicy

    public init(
        layerID: UInt64? = nil,
        planeID: UInt64? = nil,
        layerKind: CoreLayerKind? = .binaryColoring,
        planeKind: CorePlaneKind? = .color,
        missingPolicy: CoreBatchMissingPolicy = .error
    ) {
        self.layerID = layerID
        self.planeID = planeID
        self.layerKind = layerKind
        self.planeKind = planeKind
        self.missingPolicy = missingPolicy
    }
}

public enum CoreBatchOperationKind: UInt32, CaseIterable, Equatable, Hashable, Sendable {
    case colorReplace = 1
    case continuousFill = 2
    case separation = 3
    case visibility = 4
    case lineWidth = 5
    case filter = 6
    case boundaryAirbrush = 7
    case dustRemoval = 8
    case mirror = 9
    case rotate90 = 10
    case resize = 11
    case convertPlane = 12
}

public enum CoreBatchSeparationDestination: Int64, CaseIterable, Equatable, Sendable {
    case replaceSource = 1
    case selectionMask = 2
    case mainLinePlane = 3
    case colorPlane = 4
    case nativeFile = 5
}

public struct CoreBatchColorPair: Equatable, Sendable {
    public var enabled: Bool
    public var oldColor: CoreColorValue
    public var newColor: CoreColorValue

    public init(enabled: Bool = true, oldColor: CoreColorValue, newColor: CoreColorValue) {
        self.enabled = enabled
        self.oldColor = oldColor
        self.newColor = newColor
    }
}

public struct CoreBatchSeed: Equatable, Sendable {
    public var enabled: Bool
    public var x: UInt32
    public var y: UInt32
    public var fillColor: CoreColorValue
    public var tolerance: UInt16
    public var gapClose: UInt8
    public var expectedColor: CoreColorValue?

    public init(
        enabled: Bool = true,
        x: UInt32,
        y: UInt32,
        fillColor: CoreColorValue,
        tolerance: UInt16 = 0,
        gapClose: UInt8 = 0,
        expectedColor: CoreColorValue? = nil
    ) {
        self.enabled = enabled
        self.x = x
        self.y = y
        self.fillColor = fillColor
        self.tolerance = tolerance
        self.gapClose = gapClose
        self.expectedColor = expectedColor
    }
}

public struct CoreBatchOperation: Equatable, Identifiable, Sendable {
    public let id: UUID
    public var kind: CoreBatchOperationKind
    public var enabled: Bool
    public var configureEachRun: Bool
    public var target: CoreBatchTargetSelector?
    public var parameters: [Int64]
    public var colors: [CoreColorValue]
    public var colorPairs: [CoreBatchColorPair]
    public var seeds: [CoreBatchSeed]
    public var filter: CoreFilterRequest?

    public init(
        id: UUID = UUID(),
        kind: CoreBatchOperationKind,
        enabled: Bool = true,
        configureEachRun: Bool = false,
        target: CoreBatchTargetSelector? = CoreBatchTargetSelector(),
        parameters: [Int64] = [],
        colors: [CoreColorValue] = [],
        colorPairs: [CoreBatchColorPair] = [],
        seeds: [CoreBatchSeed] = [],
        filter: CoreFilterRequest? = nil
    ) {
        self.id = id
        self.kind = kind
        self.enabled = enabled
        self.configureEachRun = configureEachRun
        self.target = target
        self.parameters = Array(parameters.prefix(8))
        self.colors = colors
        self.colorPairs = colorPairs
        self.seeds = seeds
        self.filter = filter
    }

    public static func invertColorPlane() -> Self {
        example(.filter)
    }

    public static func example(_ kind: CoreBatchOperationKind) -> Self {
        let transparent = CoreColorValue.rgba8(red: 0, green: 0, blue: 0, alpha: 0)
        let black = CoreColorValue.rgba8(red: 0, green: 0, blue: 0)
        let white = CoreColorValue.rgba8(red: 255, green: 255, blue: 255)
        switch kind {
        case .colorReplace:
            return Self(kind: kind, colorPairs: [
                CoreBatchColorPair(oldColor: black, newColor: white),
            ])
        case .continuousFill:
            return Self(kind: kind, seeds: [
                CoreBatchSeed(x: 0, y: 0, fillColor: white),
            ])
        case .separation:
            return Self(kind: kind, parameters: [0, 4], colors: [black], colorPairs: [
                CoreBatchColorPair(oldColor: transparent, newColor: white),
            ])
        case .visibility:
            return Self(
                kind: kind,
                target: CoreBatchTargetSelector(planeKind: nil),
                parameters: [1]
            )
        case .lineWidth:
            return Self(kind: kind, parameters: [4, 1_000])
        case .filter:
            return Self(
                kind: kind,
                filter: CoreFilterRequest(kind: .invert, planeID: 1)
            )
        case .boundaryAirbrush:
            return Self(kind: kind, parameters: [1, 1_000], colors: [black, white])
        case .dustRemoval:
            return Self(kind: kind, parameters: [1, 1])
        case .mirror:
            return Self(kind: kind, target: nil, parameters: [1])
        case .rotate90:
            return Self(kind: kind, target: nil, parameters: [1])
        case .resize:
            return Self(kind: kind, target: nil, parameters: [1, 1, 72_000, 72_000, 1, 3])
        case .convertPlane:
            return Self(kind: kind, parameters: [4, 1])
        }
    }

    var isValid: Bool {
        guard parameters.count <= 8,
              colors.count <= 4_096,
              colorPairs.count <= 4_096,
              seeds.count <= 4_096,
              colors.allSatisfy(\.hasValidNativeComponents),
              colorPairs.allSatisfy({
                  $0.oldColor.hasValidNativeComponents && $0.newColor.hasValidNativeComponents
              }),
              seeds.allSatisfy({
                  $0.fillColor.hasValidNativeComponents
                      && ($0.expectedColor?.hasValidNativeComponents ?? true)
              })
        else { return false }
        let noTarget = kind == .mirror || kind == .rotate90 || kind == .resize
        guard noTarget || target != nil else { return false }
        switch kind {
        case .colorReplace: return !colorPairs.isEmpty
        case .continuousFill: return !seeds.isEmpty
        case .separation: return !colors.isEmpty && colorPairs.first != nil
        case .filter: return filter?.isValid == true
        case .boundaryAirbrush: return colors.count >= 2
        default: return true
        }
    }
}

public enum CoreBatchOutputPolicy: UInt32, CaseIterable, Equatable, Sendable {
    case duplicate = 1
    case newSave = 2
    case explicitOverwrite = 3
}

public enum CoreBatchFailurePolicy: UInt32, CaseIterable, Equatable, Sendable {
    case `continue` = 1
    case stop = 2
}

public struct CoreBatchOutputSettings: Equatable, Sendable {
    public var policy: CoreBatchOutputPolicy
    public var folder: String
    public var cellFolder: Bool
    public var basename: String
    public var startNumber: UInt32
    public var descending: Bool
    public var failurePolicy: CoreBatchFailurePolicy
    public var waitMilliseconds: UInt32
    public var previewBeforeSave: Bool

    public init(
        policy: CoreBatchOutputPolicy = .duplicate,
        folder: String = "",
        cellFolder: Bool = false,
        basename: String = "",
        startNumber: UInt32 = 1,
        descending: Bool = false,
        failurePolicy: CoreBatchFailurePolicy = .continue,
        waitMilliseconds: UInt32 = 0,
        previewBeforeSave: Bool = false
    ) {
        self.policy = policy
        self.folder = folder
        self.cellFolder = cellFolder
        self.basename = basename
        self.startNumber = startNumber
        self.descending = descending
        self.failurePolicy = failurePolicy
        self.waitMilliseconds = waitMilliseconds
        self.previewBeforeSave = previewBeforeSave
    }
}

public struct CoreBatchGraphDraft: Equatable, Sendable {
    public var name: String
    public var inputs: [CoreBatchInputSelector]
    public var operations: [CoreBatchOperation]
    public var output: CoreBatchOutputSettings

    public init(
        name: String,
        inputs: [CoreBatchInputSelector],
        operations: [CoreBatchOperation],
        output: CoreBatchOutputSettings = CoreBatchOutputSettings()
    ) {
        self.name = name
        self.inputs = inputs
        self.operations = operations
        self.output = output
    }

    var isValid: Bool {
        !name.isEmpty && name.utf8.count <= 4_096 && !name.utf8.contains(0)
            && !name.contains("/") && !name.contains("\\")
            && !inputs.isEmpty && inputs.count <= 1_024 && inputs.allSatisfy(\.isValid)
            && !operations.isEmpty && operations.count <= 1_024
            && operations.allSatisfy(\.isValid)
            && output.folder.utf8.count <= 32_768 && !output.folder.utf8.contains(0)
            && output.basename.utf8.count <= 4_096 && !output.basename.utf8.contains(0)
            && output.waitMilliseconds <= 3_600_000
    }

    var isRunReady: Bool {
        isValid && operations.allSatisfy { !$0.configureEachRun }
    }
}

public enum CoreBatchRunScope: UInt32, CaseIterable, Equatable, Sendable {
    case current = 1
    case all = 2
}

public struct CoreBatchRunOptions: Equatable, Sendable {
    public var scope: CoreBatchRunScope
    public var dryRun: Bool
    public var previewConfirmed: Bool

    public init(
        scope: CoreBatchRunScope,
        dryRun: Bool = false,
        previewConfirmed: Bool = false
    ) {
        self.scope = scope
        self.dryRun = dryRun
        self.previewConfirmed = previewConfirmed
    }
}

public struct CoreBatchPreviewItem: Equatable, Sendable {
    public let inputName: String
    public let outputPath: String
    public let warning: String
}

public struct CoreBatchPreviewProjection: Equatable, Sendable {
    public let items: [CoreBatchPreviewItem]
}

public enum CoreBatchItemOutcome: UInt32, Equatable, Sendable {
    case succeeded = 1
    case skipped = 2
    case failed = 3
    case cancelled = 4
    case dryRun = 5
}

public struct CoreBatchReportItem: Equatable, Sendable {
    public let outcome: CoreBatchItemOutcome
    public let inputName: String
    public let outputPath: String
    public let message: String
}

public struct CoreBatchReportProjection: Equatable, Sendable {
    public let cancelled: Bool
    public let failureCount: UInt64
    public let items: [CoreBatchReportItem]
}

public struct CoreBatchGraphSummary: Equatable, Sendable {
    public let version: UInt32
    public let inputCount: UInt64
    public let operationCount: UInt64
    public let operationKinds: [CoreBatchOperationKind]
    public let operations: [CoreBatchOperation]
    public let outputPolicy: CoreBatchOutputPolicy
    public let failurePolicy: CoreBatchFailurePolicy
}

public enum CoreBatchTaskState: UInt32, Equatable, Sendable {
    case ready = 0
    case running = 1
    case completed = 2
    case cancelled = 3
    case failed = 4
}

public struct CoreBatchProgressProjection: Equatable, Sendable {
    public let state: CoreBatchTaskState
    public let completedWork: UInt64
    public let totalWork: UInt64
}

public struct CoreBatchPairCandidateProjection: Equatable, Sendable {
    public let oldColor: CoreColorValue
    public let newColor: CoreColorValue
    public let pixelCount: UInt64
    public let affectedBounds: CoreFrameRect
    public let ambiguous: Bool
}

public struct CoreBatchPairPreviewProjection: Equatable, Sendable {
    public let pixelFormat: CorePixelStorageFormat
    public let width: UInt32
    public let height: UInt32
    public let unchangedPixelCount: UInt64
    public let ambiguityCount: UInt32
    public let candidates: [CoreBatchPairCandidateProjection]
}
