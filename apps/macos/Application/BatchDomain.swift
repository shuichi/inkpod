import Foundation

enum BatchDraftEditResult: Equatable {
    case applied
    case noOp
    case invalid
}

struct BatchWindowDraft: Equatable {
    var name = "Batch Set"
    var inputs: [CoreBatchInputSelector] = [.currentSequence()]
    var operations: [CoreBatchOperation] = []
    var output = CoreBatchOutputSettings()

    @discardableResult
    mutating func add(_ operation: CoreBatchOperation) -> BatchDraftEditResult {
        guard operations.count < 1_024 else { return .invalid }
        operations.append(operation)
        return .applied
    }

    @discardableResult
    mutating func removeOperation(at index: Int) -> BatchDraftEditResult {
        guard operations.indices.contains(index) else { return .invalid }
        operations.remove(at: index)
        return .applied
    }

    @discardableResult
    mutating func moveOperation(from source: Int, to destination: Int) -> BatchDraftEditResult {
        guard operations.indices.contains(source), operations.indices.contains(destination) else {
            return .invalid
        }
        guard source != destination else { return .noOp }
        let operation = operations.remove(at: source)
        operations.insert(operation, at: destination)
        return .applied
    }

    var coreGraph: CoreBatchGraphDraft {
        CoreBatchGraphDraft(name: name, inputs: inputs, operations: operations, output: output)
    }
}

enum BatchCommandCatalog {
    static let operationCommands: [InkpodCommandID: CoreBatchOperation] = [
        .batchAddColorReplace: .example(.colorReplace),
        .batchAddContinuousFill: .example(.continuousFill),
        .batchAddSeparation: .example(.separation),
        .batchAddVisibility: .example(.visibility),
        .batchAddLineWidth: .example(.lineWidth),
        .batchAddBoundaryAirbrush: .example(.boundaryAirbrush),
        .batchAddDust: .example(.dustRemoval),
        .batchAddMirror: .example(.mirror),
        .batchAddRotate: .example(.rotate90),
        .batchAddResize: .example(.resize),
        .batchAddConvert: .example(.convertPlane),
        .batchAddFilterSharpenWeak: filter(.sharpenWeak),
        .batchAddFilterSharpenStrong: filter(.sharpenStrong),
        .batchAddFilterBlurWeak: filter(.blurWeak),
        .batchAddFilterBlurStrong: filter(.blurStrong),
        .batchAddFilterGaussian: filter(.gaussianBlur, parameters: [1, 1_000]),
        .batchAddFilterInvert: filter(.invert),
        .batchAddFilterAutoContrast: filter(.autoContrast),
        .batchAddFilterBrightness: filter(.brightnessContrast, parameters: [0, 0]),
        .batchAddFilterToneCurve: filter(
            .toneCurve,
            curve: [CoreCurvePoint(input: 0, output: 0), CoreCurvePoint(input: 65_535, output: 65_535)]
        ),
        .batchAddFilterLevels: filter(.levels, parameters: [0, 1_000, 65_535, 0, 65_535]),
        .batchAddFilterHSV: filter(.hsv, parameters: [0, 0, 0]),
        .batchAddFilterColorBalance: filter(.colorBalance, parameters: [0, 0, 0]),
        .batchAddFilterUnsharp: filter(.unsharpMask, parameters: [1, 1_000, 0]),
    ]

    static let surfaceCommands: Set<InkpodCommandID> = Set([
        .windowBatch, .batchInputFile, .batchInputFolder, .batchInputCurrent,
        .batchOperationRemove, .batchOperationUp, .batchOperationDown,
        .batchOperationEdit, .batchReplaceSwap, .batchOutputDuplicate,
        .batchOutputNew, .batchOutputOverwrite, .batchFailureContinue,
        .batchFailureStop, .batchPreview, .batchDryRun, .batchRunCurrent,
        .batchRunAll, .batchSaveSet, .batchLoadSet, .batchCancel,
        .batchInputRange, .batchOutputSettings, .batchPin, .windowJobProgress,
        .batchExtractPairs,
    ]).union(operationCommands.keys)

    private static func filter(
        _ kind: CoreFilterKind,
        parameters: [Int32] = [],
        curve: [CoreCurvePoint] = []
    ) -> CoreBatchOperation {
        CoreBatchOperation(
            kind: .filter,
            filter: CoreFilterRequest(
                kind: kind,
                planeID: 1,
                parameters: parameters,
                curvePoints: curve
            )
        )
    }
}

final class BatchFolderBroker {
    private let startAccess: (URL) -> Bool
    private let stopAccess: (URL) -> Void

    init(
        startAccess: @escaping (URL) -> Bool = { $0.startAccessingSecurityScopedResource() },
        stopAccess: @escaping (URL) -> Void = { $0.stopAccessingSecurityScopedResource() }
    ) {
        self.startAccess = startAccess
        self.stopAccess = stopAccess
    }

    func acquire(_ url: URL) -> SecurityScopedResourceLease? {
        let lease = SecurityScopedResourceLease(
            url: url,
            start: startAccess,
            stop: stopAccess
        )
        return lease.isAccessing ? lease : nil
    }
}

@MainActor
final class BatchJobRegistry {
    private(set) var activeTask: CoreTask?
    private var leases: [SecurityScopedResourceLease] = []
    private var closing = false

    func start(_ task: CoreTask) -> Bool {
        guard !closing, activeTask == nil else { return false }
        activeTask = task
        return true
    }

    func retain(_ lease: SecurityScopedResourceLease) {
        if closing { lease.close() }
        else { leases.append(lease) }
    }

    func retain(contentsOf newLeases: [SecurityScopedResourceLease]) {
        newLeases.forEach(retain)
    }

    func replaceLeases(with newLeases: [SecurityScopedResourceLease]) {
        releaseLeases()
        retain(contentsOf: newLeases)
    }

    func complete(_ task: CoreTask) {
        guard activeTask === task else { return }
        activeTask = nil
        if closing { releaseLeases() }
    }

    func cancel(using host: CoreHost) {
        guard let activeTask else { return }
        _ = host.cancel(request: activeTask.requestID)
    }

    func close(using host: CoreHost) {
        closing = true
        cancel(using: host)
        if activeTask == nil { releaseLeases() }
    }

    func waitUntilStopped() async {
        while activeTask != nil {
            try? await Task.sleep(for: .milliseconds(20))
        }
    }

    func releaseLeases() {
        leases.forEach { $0.close() }
        leases.removeAll(keepingCapacity: false)
    }
}
