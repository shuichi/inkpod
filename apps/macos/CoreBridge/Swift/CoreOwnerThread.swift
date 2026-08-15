import Darwin
import Foundation
import InkpodCoreC

private struct BoundedRingBuffer<Element> {
    private var storage: [Element?]
    private var head = 0
    private var tail = 0
    private(set) var count = 0

    init(capacity: Int) {
        precondition(capacity > 0)
        storage = Array(repeating: nil, count: capacity)
    }

    var isEmpty: Bool { count == 0 }
    var isFull: Bool { count == storage.count }

    mutating func append(_ element: Element) -> Bool {
        guard !isFull else { return false }
        storage[tail] = element
        tail = (tail + 1) % storage.count
        count += 1
        return true
    }

    mutating func popFirst() -> Element? {
        guard !isEmpty else { return nil }
        let element = storage[head]
        storage[head] = nil
        head = (head + 1) % storage.count
        count -= 1
        return element
    }

    mutating func drain() -> [Element] {
        var elements: [Element] = []
        elements.reserveCapacity(count)
        while let element = popFirst() {
            elements.append(element)
        }
        return elements
    }
}

enum CoreMailboxAdmission: Equatable {
    case accepted
    case queueFull
    case allocationFailed
    case hostStopped
    case shutdownAlreadyEnqueued
}

final class CoreMailbox: @unchecked Sendable {
    static let normalCapacity = 4_096
    static let inputSampleCapacity = 4_096
    static let inputBoundaryReserve = 64
    static let controlCapacity = 64

    private let condition = NSCondition()
    private var normal = BoundedRingBuffer<CoreRequestEnvelope>(capacity: normalCapacity)
    private var input = BoundedRingBuffer<CoreRequestEnvelope>(
        capacity: inputSampleCapacity + inputBoundaryReserve
    )
    private var control = BoundedRingBuffer<CoreRequestEnvelope>(capacity: controlCapacity)
    private var normalProcessingEnabled = true
    private var accepting = true
    private var shutdownEnqueued = false
    private var stopped = false
    private var normalAdmissionFailureCount: Int

    init(normalAdmissionFailureCount: Int) {
        self.normalAdmissionFailureCount = normalAdmissionFailureCount
    }

    func enqueue(
        _ envelope: CoreRequestEnvelope,
        lane: CoreRequestLane,
        isShutdown: Bool = false
    ) -> CoreMailboxAdmission {
        condition.lock()
        defer { condition.unlock() }

        if isShutdown, shutdownEnqueued {
            return .shutdownAlreadyEnqueued
        }
        guard accepting, !stopped else {
            return .hostStopped
        }
        if lane == .normal, normalAdmissionFailureCount > 0 {
            normalAdmissionFailureCount -= 1
            return .allocationFailed
        }

        let appended: Bool
        switch lane {
        case .normal:
            appended = normal.append(envelope)
        case .inputSample:
            appended = input.count < Self.inputSampleCapacity && input.append(envelope)
        case .inputBoundary:
            appended = input.append(envelope)
        case .control:
            appended = control.append(envelope)
        }
        guard appended else {
            return .queueFull
        }
        if isShutdown {
            accepting = false
            shutdownEnqueued = true
        }
        condition.signal()
        return .accepted
    }

    func take() -> CoreRequestEnvelope? {
        condition.lock()
        defer { condition.unlock() }
        while true {
            if let envelope = control.popFirst() {
                return envelope
            }
            if stopped {
                return nil
            }
            if let envelope = input.popFirst() {
                return envelope
            }
            if normalProcessingEnabled, let envelope = normal.popFirst() {
                return envelope
            }
            condition.wait()
        }
    }

    func setNormalProcessingEnabled(_ enabled: Bool) {
        condition.lock()
        normalProcessingEnabled = enabled
        condition.broadcast()
        condition.unlock()
    }

    func drainAndStop() -> [CoreRequestEnvelope] {
        condition.lock()
        accepting = false
        stopped = true
        var drained = control.drain()
        drained.append(contentsOf: input.drain())
        drained.append(contentsOf: normal.drain())
        condition.broadcast()
        condition.unlock()
        return drained
    }
}

private final class CoreOwnerFinishSignal: @unchecked Sendable {
    private let condition = NSCondition()
    private var finished = false

    func markFinished() {
        condition.lock()
        finished = true
        condition.broadcast()
        condition.unlock()
    }

    func wait(timeout: TimeInterval) -> Bool {
        let deadline = Date(timeIntervalSinceNow: timeout)
        condition.lock()
        while !finished, condition.wait(until: deadline) {}
        let result = finished
        condition.unlock()
        return result
    }
}

private final class CoreCancellationRegistry: @unchecked Sendable {
    private let lock = NSLock()
    private var active: [CoreRequestID: OpaquePointer] = [:]
    private var requested: Set<CoreRequestID> = []

    func request(_ requestID: CoreRequestID) {
        let task = lock.withLock { () -> OpaquePointer? in
            if let task = active[requestID] { return task }
            requested.insert(requestID)
            return nil
        }
        if let task { _ = inkpod_task_cancel(task) }
    }

    func begin(_ requestID: CoreRequestID, task: OpaquePointer) {
        let cancelImmediately = lock.withLock { () -> Bool in
            precondition(active[requestID] == nil)
            active[requestID] = task
            return requested.remove(requestID) != nil
        }
        if cancelImmediately { _ = inkpod_task_cancel(task) }
    }

    func finish(_ requestID: CoreRequestID) {
        lock.withLock {
            active.removeValue(forKey: requestID)
            requested.remove(requestID)
        }
    }
}

private final class CoreBatchCancellationRegistry: @unchecked Sendable {
    private let lock = NSLock()
    private var active: [CoreRequestID: OpaquePointer] = [:]
    private var requested: Set<CoreRequestID> = []

    func request(_ requestID: CoreRequestID) {
        lock.withLock {
            if let task = active[requestID] {
                _ = inkpod_batch_task_cancel(task)
            } else {
                requested.insert(requestID)
            }
        }
    }

    func begin(_ requestID: CoreRequestID, task: OpaquePointer) {
        lock.withLock {
            precondition(active[requestID] == nil)
            active[requestID] = task
            if requested.remove(requestID) != nil {
                _ = inkpod_batch_task_cancel(task)
            }
        }
    }

    func finish(_ requestID: CoreRequestID) {
        lock.withLock {
            active.removeValue(forKey: requestID)
            requested.remove(requestID)
        }
    }

    func query(_ requestID: CoreRequestID) -> CoreBatchProgressProjection? {
        lock.withLock {
            guard let task = active[requestID] else { return nil }
            var info = InkpodTaskInfo()
            info.struct_size = UInt32(MemoryLayout<InkpodTaskInfo>.size)
            guard CoreStatus(cValue: inkpod_batch_task_query(task, &info)) == .ok,
                  let state = CoreBatchTaskState(rawValue: info.state)
            else { return nil }
            return CoreBatchProgressProjection(
                state: state,
                completedWork: info.completed_work,
                totalWork: info.total_work
            )
        }
    }
}

final class CoreOwnerThread: @unchecked Sendable {
    private let finishSignal = CoreOwnerFinishSignal()
    private let cancellations: CoreCancellationRegistry
    private let batchCancellations: CoreBatchCancellationRegistry
    private let thread: Thread

    init(
        mailbox: CoreMailbox,
        completions: CoreCompletionRegistry,
        testConfiguration: CoreHostTestConfiguration
    ) {
        let finishSignal = self.finishSignal
        let cancellations = CoreCancellationRegistry()
        let batchCancellations = CoreBatchCancellationRegistry()
        self.cancellations = cancellations
        self.batchCancellations = batchCancellations
        thread = Thread {
            let loop = CoreOwnerLoop(
                mailbox: mailbox,
                completions: completions,
                cancellations: cancellations,
                batchCancellations: batchCancellations,
                createABIMismatchCount: testConfiguration.createABIMismatchCount
            )
            loop.run()
            finishSignal.markFinished()
        }
        thread.name = "inkpod.macos.core-owner"
        thread.qualityOfService = .userInitiated
    }

    func start() {
        thread.start()
    }

    func waitUntilFinished(timeout: TimeInterval) -> Bool {
        finishSignal.wait(timeout: timeout)
    }

    func requestCancellation(_ requestID: CoreRequestID) {
        cancellations.request(requestID)
        batchCancellations.request(requestID)
    }

    func batchProgress(_ requestID: CoreRequestID) -> CoreBatchProgressProjection? {
        batchCancellations.query(requestID)
    }
}

private enum CoreTransientKind {
    case stroke
    case floatingPaste
    case filterPreview
    case geometryPreview
    case shootingFramePreview
    case vanishingPointPreview
    case annotationStroke
    case motion
}

private struct CoreSessionEntry {
    var core: OpaquePointer
    let target: CoreSessionTarget
    let primaryView: CoreViewTarget
    var documentUUID: CoreDocumentUUID
    var views: [CoreViewID: CoreViewEntry]
    var activeTransient: CoreTransientKind? = nil
    var colorCheckMode: CoreColorCheckMode = .off
    var motion: CoreMotionProjection? = nil
}

private struct CoreViewEntry {
    let target: CoreViewTarget
    /// Zero denotes the Core primary logical view; nonzero values are Rust-owned
    /// secondary view IDs and never cross the SwiftUI boundary.
    let coreViewID: UInt64
}

private struct CoreCellPlanEntry {
    var raw: OpaquePointer
    let projection: CoreCellCreationPlanProjection
}

private struct CoreColorChartPreviewEntry {
    var raw: OpaquePointer
    let session: CoreSessionTarget
    let baseDocumentRevision: UInt64
}

private struct CoreHistoryVisualizationEntry {
    let session: CoreSessionTarget
    var task: OpaquePointer?
    var builder: OpaquePointer?
    var visualization: OpaquePointer?
    var progress: CoreHistoryVisualizationProgressProjection
}

private struct CoreCutEntry {
    var cut: OpaquePointer
    let target: CoreCutTarget
}

private enum CoreCutTargetResolution {
    case live(CoreCutEntry)
    case retired
    case invalid
    case stale
}

private enum CoreTargetResolution {
    case live(CoreSessionEntry)
    case retired
    case invalid
    case stale
}

private enum CoreViewTargetResolution {
    case live(CoreSessionEntry, CoreViewEntry)
    case retired
    case invalid
    case stale
}

private enum BareCoreResult {
    case success(OpaquePointer)
    case failure(CoreStatus)
}

private final class CoreOwnerLoop {
    private let mailbox: CoreMailbox
    private let completions: CoreCompletionRegistry
    private let cancellations: CoreCancellationRegistry
    private let batchCancellations: CoreBatchCancellationRegistry
    private let ownerThreadID: UInt64
    private var sessions: [CoreSessionID: CoreSessionEntry] = [:]
    private var sessionByDocumentUUID: [CoreDocumentUUID: CoreSessionID] = [:]
    private var retiredGenerations: [CoreSessionID: CoreSessionGeneration] = [:]
    private var retiredViewGenerations: [CoreViewID: CoreViewGeneration] = [:]
    private var nextSessionID: UInt64 = 1
    private var nextGeneration: UInt64 = 1
    private var nextViewID: UInt64 = 1
    private var nextViewGeneration: UInt64 = 1
    private var cuts: [CoreCutID: CoreCutEntry] = [:]
    private var retiredCutGenerations: [CoreCutID: CoreCutGeneration] = [:]
    private var nextCutID: UInt64 = 1
    private var nextCutGeneration: UInt64 = 1
    private var clipboards: [CoreClipboardID: OpaquePointer] = [:]
    private var nextClipboardID: UInt64 = 1
    private var cellPlans: [CoreCellCreationPlanID: CoreCellPlanEntry] = [:]
    private var nextCellPlanID: UInt64 = 1
    private var colorChartPreviews: [CoreColorChartPreviewID: CoreColorChartPreviewEntry] = [:]
    private var nextColorChartPreviewID: UInt64 = 1
    private var historyVisualizations: [
        CoreHistoryVisualizationID: CoreHistoryVisualizationEntry
    ] = [:]
    private var nextHistoryVisualizationID: UInt64 = 1
    private var createABIMismatchCount: Int

    init(
        mailbox: CoreMailbox,
        completions: CoreCompletionRegistry,
        cancellations: CoreCancellationRegistry,
        batchCancellations: CoreBatchCancellationRegistry,
        createABIMismatchCount: Int
    ) {
        self.mailbox = mailbox
        self.completions = completions
        self.cancellations = cancellations
        self.batchCancellations = batchCancellations
        self.createABIMismatchCount = createABIMismatchCount
        ownerThreadID = UInt64(pthread_mach_thread_np(pthread_self()))
    }

    func run() {
        while let envelope = mailbox.take() {
            guard completions.isPending(envelope.requestID) else {
                continue
            }
            switch envelope.request {
            case let .cancel(targetRequestID):
                let cancelled = completions.complete(
                    targetRequestID,
                    with: .failed(.cancelled)
                )
                if cancelled {
                    cancellations.finish(targetRequestID)
                    batchCancellations.finish(targetRequestID)
                }
                completions.complete(
                    envelope.requestID,
                    with: cancelled ? .acknowledged : .noOp(nil)
                )
            case .shutdown:
                executeShutdown(requestID: envelope.requestID)
                return
            case let .setNormalProcessingEnabledForTesting(enabled):
                mailbox.setNormalProcessingEnabled(enabled)
                completions.complete(envelope.requestID, with: .acknowledged)
            default:
                completions.complete(
                    envelope.requestID,
                    with: execute(envelope.request, requestID: envelope.requestID)
                )
            }
        }
    }

    private func execute(
        _ request: CoreRequest,
        requestID: CoreRequestID
    ) -> CoreRequestOutcome {
        switch request {
        case let .createSession(documentUUID):
            return createSession(documentUUID: documentUUID)
        case let .prepareCellCreation(options):
            return prepareCellCreation(options: options)
        case let .commitCellCreation(planID, documentUUIDs):
            return commitCellCreation(planID: planID, documentUUIDs: documentUUIDs)
        case let .cancelCellCreation(planID):
            return cancelCellCreation(planID: planID)
        case let .inspectSession(target):
            return inspectSession(target: target)
        case let .closeSession(target):
            return closeSession(target: target)
        case let .createCut(cutUUID, metadata, defaults, members):
            return createCut(
                cutUUID: cutUUID,
                metadata: metadata,
                defaults: defaults,
                members: members
            )
        case let .inspectCut(target):
            return inspectCut(target)
        case let .closeCut(target):
            return closeCut(target)
        case let .openCut(path, recovery):
            return openCut(pathUTF8: path, recovery: recovery)
        case let .updateCut(target, expectedRevision, metadata, defaults):
            return updateCut(
                target,
                expectedRevision: expectedRevision,
                metadata: metadata,
                defaults: defaults
            )
        case let .cancelCutUpdate(target):
            return cancelCutUpdate(target)
        case let .editCutSequence(target, expectedRevision, operations):
            return editCutSequence(
                target,
                expectedRevision: expectedRevision,
                operations: operations
            )
        case let .cancelCutSequence(target):
            return cancelCutSequence(target)
        case let .undoCut(target, expectedRevision):
            return cutHistory(target, expectedRevision: expectedRevision, redo: false)
        case let .redoCut(target, expectedRevision):
            return cutHistory(target, expectedRevision: expectedRevision, redo: true)
        case let .saveCut(target, expectedRevision, path, recovery):
            return saveCut(
                target,
                expectedRevision: expectedRevision,
                pathUTF8: path,
                recovery: recovery
            )
        case let .createView(target, expectedRevision):
            return createView(target: target, expectedDocumentRevision: expectedRevision)
        case let .closeView(target):
            return closeView(target: target)
        case let .applyView(target, command, expectation):
            return applyView(target: target, command: command, expectation: expectation)
        case let .resolveDocumentPoints(target, documentRevision, viewRevision, samples):
            return resolveDocumentPoints(
                target: target,
                expectedDocumentRevision: documentRevision,
                expectedViewRevision: viewRevision,
                samples: samples
            )
        case let .applyDocument(target, command, expectedRevision):
            return applyDocument(
                target: target,
                command: command,
                expectedDocumentRevision: expectedRevision
            )
        case let .editCell(target, expectedRevision, command):
            return editCell(
                target: target,
                expectedDocumentRevision: expectedRevision,
                command: command
            )
        case let .inspectTree(target, expectedRevision):
            return inspectTree(target: target, expectedDocumentRevision: expectedRevision)
        case let .setActiveNode(target, expectedRevision, layerID, planeID):
            return setActiveNode(
                target: target,
                expectedDocumentRevision: expectedRevision,
                layerID: layerID,
                planeID: planeID
            )
        case let .editTree(target, expectedRevision, command):
            return editTree(
                target: target,
                expectedDocumentRevision: expectedRevision,
                command: command
            )
        case let .inspectPaint(target, expectedRevision):
            return inspectPaint(target: target, expectedDocumentRevision: expectedRevision)
        case let .updateEditor(target, expectation, update):
            return updateEditor(target: target, expectation: expectation, update: update)
        case let .beginRasterStroke(target, expectation, samples):
            return beginRasterStroke(
                target: target,
                expectation: expectation,
                samples: samples
            )
        case let .appendRasterStroke(target, samples):
            return appendRasterStroke(target: target, samples: samples)
        case let .endStroke(target):
            return endStroke(target: target)
        case let .cancelStroke(target):
            return cancelStroke(target: target)
        case let .applyFill(target, expectation, gesture):
            return applyFill(target: target, expectation: expectation, gesture: gesture)
        case let .eyedropper(target, expectation, source, point):
            return eyedropper(
                target: target,
                expectation: expectation,
                source: source,
                devicePoint: point
            )
        case let .replacePalette(target, expectedRevision, colors):
            return replacePalette(
                target: target,
                expectedDocumentRevision: expectedRevision,
                colors: colors
            )
        case let .generatePalette(target, expectedRevision, maximum, quantization):
            return generatePalette(
                target: target,
                expectedDocumentRevision: expectedRevision,
                maximumColors: maximum,
                quantizationBits: quantization
            )
        case let .savePaletteFile(target, expectedRevision, path):
            return savePaletteFile(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: path
            )
        case let .loadPaletteFile(target, expectedRevision, path):
            return loadPaletteFile(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: path
            )
        case let .replaceColorChart(target, expectedRevision, entries, locked):
            return replaceColorChart(
                target: target,
                expectedDocumentRevision: expectedRevision,
                entries: entries,
                locked: locked
            )
        case let .saveColorChartFile(target, expectedRevision, path):
            return saveColorChartFile(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: path
            )
        case let .loadColorChartFile(target, expectedRevision, path):
            return loadColorChartFile(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: path
            )
        case let .createColorChartPreview(target, expectedRevision, maximum, quantization):
            return createColorChartPreview(
                target: target,
                expectedDocumentRevision: expectedRevision,
                maximumColors: maximum,
                quantizationBits: quantization,
                requestID: requestID
            )
        case let .applyColorChartPreview(target, expectedRevision, preview):
            return applyColorChartPreview(
                target: target,
                expectedDocumentRevision: expectedRevision,
                previewID: preview
            )
        case let .cancelColorChartPreview(preview):
            return cancelColorChartPreview(preview)
        case let .setColorCheck(target, expectedRevision, mode):
            return setColorCheck(
                target: target,
                expectedViewRevision: expectedRevision,
                mode: mode
            )
        case let .inspectLocator(target, expectedRevision, point, radius):
            return inspectLocator(
                target: target,
                expectedViewRevision: expectedRevision,
                devicePoint: point,
                radius: radius
            )
        case let .paintLocatorPixel(target, expectation, x, y):
            return paintLocatorPixel(
                target: target,
                expectation: expectation,
                documentX: x,
                documentY: y
            )
        case let .previewColorReplace(target, expectation, input):
            return colorReplace(
                target: target,
                expectation: expectation,
                request: input,
                commit: false
            )
        case let .applyColorReplace(target, expectation, input):
            return colorReplace(
                target: target,
                expectation: expectation,
                request: input,
                commit: true
            )
        case let .selectOutputColorGuard(target, expectedRevision, operation):
            return selectOutputColorGuard(
                target: target,
                expectedDocumentRevision: expectedRevision,
                operation: operation,
                requestID: requestID
            )
        case let .applySelection(target, expectation, samples):
            return applySelection(
                target: target,
                expectation: expectation,
                samples: samples
            )
        case let .selectionAdjust(target, expectedRevision, operation, pixels):
            return selectionAdjust(
                target: target,
                expectedDocumentRevision: expectedRevision,
                operation: operation,
                pixels: pixels
            )
        case let .clearSelection(target, expectedRevision):
            return clearSelection(
                target: target,
                expectedDocumentRevision: expectedRevision
            )
        case let .selectColor(target, expectation, different, operation):
            return selectColor(
                target: target,
                expectation: expectation,
                different: different,
                operation: operation
            )
        case let .selectionToLayer(target, expectedRevision, name):
            return selectionToLayer(
                target: target,
                expectedDocumentRevision: expectedRevision,
                nameUTF8: name
            )
        case let .selectionFromLayer(target, expectedRevision, layerID, operation):
            return selectionFromLayer(
                target: target,
                expectedDocumentRevision: expectedRevision,
                layerID: layerID,
                operation: operation
            )
        case let .undo(target, expectedRevision):
            return undo(target: target, expectedDocumentRevision: expectedRevision)
        case let .redo(target, expectedRevision):
            return redo(target: target, expectedDocumentRevision: expectedRevision)
        case let .inspectHistory(target, expectedRevision):
            return inspectHistory(
                target: target,
                expectedDocumentRevision: expectedRevision
            )
        case let .jumpHistory(target, expectedRevision, cursor):
            return jumpHistory(
                target: target,
                expectedDocumentRevision: expectedRevision,
                cursor: cursor
            )
        case let .buildSnapshot(route):
            return buildSnapshot(route: route)
        case let .save(target, expectedRevision, pathUTF8, allowCleanSave):
            return save(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: pathUTF8,
                allowCleanSave: allowCleanSave
            )
        case let .open(target, expectedRevision, pathUTF8):
            return openFile(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: pathUTF8,
                recovery: false
            )
        case let .autosave(target, expectedRevision, pathUTF8):
            return autosave(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: pathUTF8
            )
        case let .openRecovery(target, expectedRevision, pathUTF8):
            return openFile(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: pathUTF8,
                recovery: true
            )
        case let .revert(target, expectedRevision):
            return revert(
                target: target,
                expectedDocumentRevision: expectedRevision,
                partial: false
            )
        case let .revertPartial(target, expectedRevision):
            return revert(
                target: target,
                expectedDocumentRevision: expectedRevision,
                partial: true
            )
        case let .importCommonRaster(target, expectedRevision, format, bytes, documentUUID):
            return importCommonRaster(
                target: target,
                expectedDocumentRevision: expectedRevision,
                format: format,
                bytes: bytes,
                documentUUID: documentUUID
            )
        case let .exportCommonRaster(target, expectedRevision, format, compositeWhite):
            return exportCommonRaster(
                target: target,
                expectedDocumentRevision: expectedRevision,
                format: format,
                compositeWhite: compositeWhite
            )
        case let .compactionPlan(target, expectedRevision):
            return compactionPlan(
                target: target,
                expectedDocumentRevision: expectedRevision
            )
        case let .writeCompactedCopy(target, expectedRevision, pathUTF8, token):
            return writeCompactedCopy(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: pathUTF8,
                token: token
            )
        case let .copyClipboard(target, expectedRevision, cut):
            return copyClipboard(
                target: target,
                expectedDocumentRevision: expectedRevision,
                cut: cut
            )
        case let .createClipboard(raster):
            return createClipboard(from: raster)
        case let .releaseClipboard(clipboard):
            return releaseClipboard(clipboard)
        case let .beginPaste(target, expectedRevision, clipboard, mode):
            return beginPaste(
                target: target,
                expectedDocumentRevision: expectedRevision,
                clipboard: clipboard,
                mode: mode
            )
        case let .transformFloatingPaste(target, expectedRevision, transform):
            return transformFloatingPaste(
                target: target,
                expectedDocumentRevision: expectedRevision,
                transform: transform
            )
        case let .commitPaste(target, expectedRevision):
            return finishPaste(
                target: target,
                expectedDocumentRevision: expectedRevision,
                commit: true
            )
        case let .cancelPaste(target, expectedRevision):
            return finishPaste(
                target: target,
                expectedDocumentRevision: expectedRevision,
                commit: false
            )
        case let .beginHistoryVisualization(target, expectedRevision):
            return beginHistoryVisualization(
                target: target,
                expectedDocumentRevision: expectedRevision
            )
        case let .stepHistoryVisualization(visualization, maximumEvents):
            return stepHistoryVisualization(
                visualization,
                maximumEvents: maximumEvents
            )
        case let .historyVisualizationRows(visualization, range):
            return historyVisualizationRows(visualization, range: range)
        case let .releaseHistoryVisualization(visualization):
            return releaseHistoryVisualization(visualization)
        case let .inspectM8(target, expectedRevision):
            return inspectM8(
                target: target,
                expectedDocumentRevision: expectedRevision
            )
        case let .performM8(target, expectedRevision, command):
            return performM8(
                target: target,
                expectedDocumentRevision: expectedRevision,
                command: command,
                requestID: requestID
            )
        case let .inspectAnimation(target, expectedRevision):
            return inspectAnimation(
                target: target,
                expectedDocumentRevision: expectedRevision
            )
        case let .performAnimation(target, expectedRevision, command):
            return performAnimation(
                target: target,
                expectedDocumentRevision: expectedRevision,
                command: command
            )
        case let .exportInstructionRaster(target, expectedRevision, format, compositeWhite):
            return exportInstructionRaster(
                target: target,
                expectedDocumentRevision: expectedRevision,
                format: format,
                compositeWhite: compositeWhite
            )
        case let .previewBatch(target, expectedRevision, graph, scope):
            return previewBatch(
                target: target,
                expectedDocumentRevision: expectedRevision,
                graph: graph,
                scope: scope
            )
        case let .executeBatch(target, expectedRevision, graph, options):
            return executeBatch(
                target: target,
                expectedDocumentRevision: expectedRevision,
                graph: graph,
                options: options,
                requestID: requestID
            )
        case let .saveBatchGraph(graph, path):
            return saveBatchGraph(graph, pathUTF8: path)
        case let .inspectBatchGraph(path):
            return inspectBatchGraph(pathUTF8: path)
        case let .previewSavedBatch(target, expectedRevision, path, operations, scope):
            return previewSavedBatch(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: path,
                operations: operations,
                scope: scope
            )
        case let .executeSavedBatch(target, expectedRevision, path, operations, options):
            return executeSavedBatch(
                target: target,
                expectedDocumentRevision: expectedRevision,
                pathUTF8: path,
                operations: operations,
                options: options,
                requestID: requestID
            )
        case let .extractBatchPairs(target, expectedRevision, oldIndex, newIndex):
            return extractBatchPairs(
                target: target,
                expectedDocumentRevision: expectedRevision,
                oldSequenceIndex: oldIndex,
                newSequenceIndex: newIndex
            )
        case let .selectAll(target, expectedRevision):
            return selectAll(
                target: target,
                expectedDocumentRevision: expectedRevision
            )
        case let .beginTransientForTesting(target):
            return beginTransientForTesting(target: target)
        case .cancel, .shutdown, .setNormalProcessingEnabledForTesting:
            preconditionFailure("control requests are handled by the owner loop")
        }
    }

    private func createSession(documentUUID: CoreDocumentUUID) -> CoreRequestOutcome {
        guard documentUUID.isValid else {
            return .failed(.invalidRequest)
        }
        if let existingID = sessionByDocumentUUID[documentUUID],
           let existing = sessions[existingID]
        {
            return projection(for: existing).map(CoreRequestOutcome.noOp)
                ?? .failed(.coreOperation(.panic))
        }
        guard sessions.count < 64 else {
            return .failed(.sessionLimit)
        }

        var config = InkpodCoreConfig()
        config.struct_size = UInt32(MemoryLayout<InkpodCoreConfig>.size)
        config.abi_version = inkpod_bridge_abi_version()
        config.feature_flags = inkpod_bridge_feature_none()
        if createABIMismatchCount > 0 {
            createABIMismatchCount -= 1
            config.abi_version &+= 1
        }

        var core: OpaquePointer?
        let createStatus = CoreStatus(cValue: inkpod_core_create(&config, &core))
        guard createStatus == .ok, let rawCore = core else {
            if core != nil {
                _ = inkpod_core_destroy(&core)
            }
            return .failed(.coreCreate(createStatus))
        }

        var defaults = InkpodEditorDefaults()
        defaults.struct_size = UInt32(MemoryLayout<InkpodEditorDefaults>.size)
        let defaultsStatus = CoreStatus(
            cValue: inkpod_core_get_editor_defaults(rawCore, &defaults)
        )
        guard defaultsStatus == .ok else {
            _ = inkpod_core_destroy(&core)
            return .failed(.coreOperation(defaultsStatus))
        }

        var options = InkpodCellCreateOptions()
        options.struct_size = UInt32(MemoryLayout<InkpodCellCreateOptions>.size)
        options.document_uuid_high = documentUUID.high
        options.document_uuid_low = documentUUID.low
        options.width = defaults.width
        options.height = defaults.height
        options.dpi_x_milli = defaults.dpi_x_milli
        options.dpi_y_milli = defaults.dpi_y_milli
        var info = InkpodDocumentInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodDocumentInfo>.size)
        let documentStatus = CoreStatus(
            cValue: inkpod_core_new_cell(rawCore, &options, &info)
        )
        guard documentStatus == .ok else {
            _ = inkpod_core_destroy(&core)
            return .failed(.coreOperation(documentStatus))
        }

        var replay = InkpodReplayContract()
        replay.struct_size = UInt32(MemoryLayout<InkpodReplayContract>.size)
        let replayStatus = CoreStatus(
            cValue: inkpod_core_get_replay_contract(rawCore, &replay)
        )
        guard replayStatus == .ok else {
            _ = inkpod_core_destroy(&core)
            return .failed(.coreOperation(replayStatus))
        }

        guard nextSessionID != 0,
              nextGeneration != 0,
              nextViewID != 0,
              nextViewGeneration != 0
        else {
            _ = inkpod_core_destroy(&core)
            return .failed(.identityOverflow)
        }
        let target = CoreSessionTarget(
            id: CoreSessionID(rawValue: nextSessionID),
            generation: CoreSessionGeneration(rawValue: nextGeneration)
        )
        let primaryView = CoreViewTarget(
            session: target,
            id: CoreViewID(rawValue: nextViewID),
            generation: CoreViewGeneration(rawValue: nextViewGeneration)
        )
        guard nextSessionID < UInt64.max,
              nextGeneration < UInt64.max,
              nextViewID < UInt64.max,
              nextViewGeneration < UInt64.max
        else {
            _ = inkpod_core_destroy(&core)
            return .failed(.identityOverflow)
        }
        nextSessionID += 1
        nextGeneration += 1
        nextViewID += 1
        nextViewGeneration += 1

        let primaryViewEntry = CoreViewEntry(target: primaryView, coreViewID: 0)
        let entry = CoreSessionEntry(
            core: rawCore,
            target: target,
            primaryView: primaryView,
            documentUUID: documentUUID,
            views: [primaryView.id: primaryViewEntry]
        )
        sessions[target.id] = entry
        sessionByDocumentUUID[documentUUID] = target.id
        return .created(
            CoreSessionProjection(
                target: target,
                primaryView: primaryView,
                documentUUID: documentUUID,
                cellID: info.cell_id,
                documentRevision: info.document_revision,
                viewRevision: info.view_revision,
                abiVersion: inkpod_abi_version(),
                replayEpoch: replay.replay_epoch,
                procedureFormatVersion: replay.procedure_format_version,
                ownerThreadID: ownerThreadID,
                hasActiveTransient: false,
                canUndo: info.flags & inkpod_bridge_document_can_undo() != 0,
                canRedo: info.flags & inkpod_bridge_document_can_redo() != 0,
                isDirty: info.flags & inkpod_bridge_document_dirty() != 0,
                isRecovered: info.flags & inkpod_bridge_document_recovered() != 0,
                documentWidth: info.width,
                documentHeight: info.height,
                dpiXMilli: info.dpi_x_milli,
                dpiYMilli: info.dpi_y_milli,
                paperFrames: paperFrames(from: info)
            )
        )
    }

    private func prepareCellCreation(
        options: CoreCellCreationOptions
    ) -> CoreRequestOutcome {
        guard options.hasValidFrontendBounds,
              nextCellPlanID != 0,
              nextCellPlanID < UInt64.max
        else {
            return .failed(.invalidRequest)
        }
        var input = InkpodCellCreationOptions()
        input.struct_size = UInt32(MemoryLayout<InkpodCellCreationOptions>.size)
        input.sizing_mode = options.sizingMode.rawValue
        input.feature_flags = inkpod_bridge_feature_none()
        input.width = options.width
        input.height = options.height
        input.dpi_x_milli = options.dpiXMilli
        input.dpi_y_milli = options.dpiYMilli
        input.margin_milli = options.marginMilli
        input.safe_frame_ratio_milli = options.safeFrameRatioMilli
        input.maximum_close_ratio_milli = options.maximumCloseRatioMilli
        input.anchor = options.anchor.rawValue
        input.initial_layer_kind = options.initialLayerKind.rawValue
        input.pixel_format = options.pixelFormat.rawValue
        input.count = options.count

        var rawPlan: OpaquePointer?
        let createStatus = CoreStatus(
            cValue: inkpod_cell_creation_plan_create(&input, &rawPlan)
        )
        guard createStatus == .ok, let rawPlan else {
            if rawPlan != nil { _ = inkpod_cell_creation_plan_release(&rawPlan) }
            return .failed(.coreOperation(createStatus == .ok ? .panic : createStatus))
        }
        var ownedPlan: OpaquePointer? = rawPlan
        var count: UInt32 = 0
        let countStatus = CoreStatus(cValue: inkpod_cell_creation_plan_count(rawPlan, &count))
        guard countStatus == .ok, count > 0, count <= 64 else {
            _ = inkpod_cell_creation_plan_release(&ownedPlan)
            return .failed(.coreOperation(countStatus == .ok ? .panic : countStatus))
        }
        var copied = [InkpodCellCreationPlanItem](repeating: InkpodCellCreationPlanItem(), count: Int(count))
        for index in copied.indices {
            copied[index].struct_size = UInt32(MemoryLayout<InkpodCellCreationPlanItem>.size)
        }
        var written: UInt32 = 0
        let copyStatus = copied.withUnsafeMutableBufferPointer { buffer in
            CoreStatus(cValue: inkpod_cell_creation_plan_copy(
                rawPlan,
                buffer.baseAddress,
                count,
                UInt64(MemoryLayout<InkpodCellCreationPlanItem>.stride),
                &written
            ))
        }
        guard copyStatus == .ok, written == count else {
            _ = inkpod_cell_creation_plan_release(&ownedPlan)
            return .failed(.coreOperation(copyStatus == .ok ? .panic : copyStatus))
        }
        let items: [CoreCellCreationPlanItem] = copied.compactMap(cellPlanItem(from:))
        guard items.count == copied.count else {
            _ = inkpod_cell_creation_plan_release(&ownedPlan)
            return .failed(.coreOperation(.panic))
        }
        let id = CoreCellCreationPlanID(rawValue: nextCellPlanID)
        nextCellPlanID += 1
        let projection = CoreCellCreationPlanProjection(id: id, options: options, items: items)
        cellPlans[id] = CoreCellPlanEntry(raw: rawPlan, projection: projection)
        ownedPlan = nil
        return .cellPlan(projection)
    }

    private func commitCellCreation(
        planID: CoreCellCreationPlanID,
        documentUUIDs: [CoreDocumentUUID]
    ) -> CoreRequestOutcome {
        guard planID.rawValue != 0 else { return .failed(.invalidRequest) }
        guard let plan = cellPlans[planID] else {
            return planID.rawValue < nextCellPlanID
                ? .failed(.staleTarget)
                : .failed(.invalidTarget)
        }
        let count = plan.projection.items.count
        guard documentUUIDs.count == count,
              !documentUUIDs.isEmpty,
              documentUUIDs.allSatisfy(\.isValid),
              Set(documentUUIDs).count == documentUUIDs.count,
              documentUUIDs.allSatisfy({ sessionByDocumentUUID[$0] == nil }),
              sessions.count + count <= 64
        else {
            return .failed(.invalidRequest)
        }
        guard let lastOffset = UInt64(exactly: count - 1),
              nextSessionID.addingReportingOverflow(lastOffset).overflow == false,
              nextGeneration.addingReportingOverflow(lastOffset).overflow == false,
              nextViewID.addingReportingOverflow(lastOffset).overflow == false,
              nextViewGeneration.addingReportingOverflow(lastOffset).overflow == false,
              nextSessionID + lastOffset < UInt64.max,
              nextGeneration + lastOffset < UInt64.max,
              nextViewID + lastOffset < UInt64.max,
              nextViewGeneration + lastOffset < UInt64.max
        else {
            return .failed(.identityOverflow)
        }

        var stagedCores: [OpaquePointer] = []
        stagedCores.reserveCapacity(count)
        for index in 0 ..< count {
            let rawCore: OpaquePointer
            switch createBareCore() {
            case let .success(core): rawCore = core
            case let .failure(status):
                destroyCores(&stagedCores)
                return .failed(.coreCreate(status))
            }
            var info = documentInfo()
            let uuid = documentUUIDs[index]
            let status = CoreStatus(cValue: inkpod_core_new_cell_from_plan(
                rawCore,
                plan.raw,
                UInt32(index),
                uuid.high,
                uuid.low,
                &info
            ))
            guard status == .ok else {
                var failedCore: OpaquePointer? = rawCore
                _ = inkpod_core_destroy(&failedCore)
                destroyCores(&stagedCores)
                return .failed(.coreOperation(status))
            }
            stagedCores.append(rawCore)
        }

        var stagedEntries: [CoreSessionEntry] = []
        var projections: [CoreSessionProjection] = []
        for index in 0 ..< count {
            let offset = UInt64(index)
            let target = CoreSessionTarget(
                id: CoreSessionID(rawValue: nextSessionID + offset),
                generation: CoreSessionGeneration(rawValue: nextGeneration + offset)
            )
            let primaryView = CoreViewTarget(
                session: target,
                id: CoreViewID(rawValue: nextViewID + offset),
                generation: CoreViewGeneration(rawValue: nextViewGeneration + offset)
            )
            let entry = CoreSessionEntry(
                core: stagedCores[index],
                target: target,
                primaryView: primaryView,
                documentUUID: documentUUIDs[index],
                views: [primaryView.id: CoreViewEntry(target: primaryView, coreViewID: 0)]
            )
            guard let projection = projection(for: entry) else {
                destroyCores(&stagedCores)
                return .failed(.coreOperation(.panic))
            }
            stagedEntries.append(entry)
            projections.append(projection)
        }

        nextSessionID += UInt64(count)
        nextGeneration += UInt64(count)
        nextViewID += UInt64(count)
        nextViewGeneration += UInt64(count)
        for entry in stagedEntries {
            sessions[entry.target.id] = entry
            sessionByDocumentUUID[entry.documentUUID] = entry.target.id
        }
        if let released = cellPlans.removeValue(forKey: planID)?.raw {
            var owner: OpaquePointer? = released
            _ = inkpod_cell_creation_plan_release(&owner)
        }
        return .cellsCreated(projections)
    }

    private func cancelCellCreation(
        planID: CoreCellCreationPlanID
    ) -> CoreRequestOutcome {
        guard planID.rawValue != 0 else { return .failed(.invalidRequest) }
        guard let plan = cellPlans.removeValue(forKey: planID) else {
            return planID.rawValue < nextCellPlanID ? .noOp(nil) : .failed(.invalidTarget)
        }
        var raw: OpaquePointer? = plan.raw
        let status = CoreStatus(cValue: inkpod_cell_creation_plan_release(&raw))
        return status == .ok && raw == nil ? .acknowledged : .failed(.coreOperation(status))
    }

    private func createView(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case var .live(entry):
            guard let session = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard session.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var coreViewID: UInt64 = 0
            let status = CoreStatus(cValue: inkpod_core_view_create(entry.core, &coreViewID))
            guard status == .ok, coreViewID != 0 else {
                return .failed(.coreOperation(status == .ok ? .panic : status))
            }
            guard nextViewID != 0, nextViewGeneration != 0,
                  nextViewID < UInt64.max, nextViewGeneration < UInt64.max
            else {
                _ = inkpod_core_view_close(entry.core, coreViewID)
                return .failed(.identityOverflow)
            }
            let viewTarget = CoreViewTarget(
                session: target,
                id: CoreViewID(rawValue: nextViewID),
                generation: CoreViewGeneration(rawValue: nextViewGeneration)
            )
            guard let viewRevision = viewRevision(core: entry.core, coreViewID: coreViewID) else {
                _ = inkpod_core_view_close(entry.core, coreViewID)
                return .failed(.coreOperation(.panic))
            }
            nextViewID += 1
            nextViewGeneration += 1
            entry.views[viewTarget.id] = CoreViewEntry(
                target: viewTarget,
                coreViewID: coreViewID
            )
            sessions[target.id] = entry
            return .viewCreated(
                CoreLogicalViewProjection(
                    session: session,
                    target: viewTarget,
                    viewRevision: viewRevision
                )
            )
        }
    }

    private func closeView(target: CoreViewTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired:
            return .noOp(nil)
        case .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(resolvedEntry, view):
            guard view.coreViewID != 0 else { return .failed(.invalidRequest) }
            var entry = resolvedEntry
            let status = CoreStatus(
                cValue: inkpod_core_view_close(entry.core, view.coreViewID)
            )
            guard status == .ok else { return .failed(.coreOperation(status)) }
            entry.views.removeValue(forKey: target.id)
            retiredViewGenerations[target.id] = target.generation
            sessions[target.session.id] = entry
            return .viewClosed(target)
        }
    }

    private func applyView(
        target: CoreViewTarget,
        command: CoreViewCommand,
        expectation: CoreCommandExpectation?
    ) -> CoreRequestOutcome {
        guard command.isValid else {
            return .failed(.coreOperation(.invalidArgument))
        }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard let beforeViewRevision = viewRevision(
                core: entry.core,
                coreViewID: view.coreViewID
            ) else {
                return .failed(.coreOperation(.panic))
            }
            if let expectation,
               (expectation.documentRevision != before.documentRevision
                   || expectation.viewRevision != beforeViewRevision)
            {
                return .failed(.staleTarget)
            }
            var input = InkpodViewInput()
            input.struct_size = UInt32(MemoryLayout<InkpodViewInput>.size)
            switch command {
            case let .viewportResized(width, height):
                input.kind = inkpod_bridge_view_viewport_resized()
                input.value1 = width
                input.value2 = height
            case let .panBy(deviceDX, deviceDY):
                input.kind = inkpod_bridge_view_pan_by()
                input.value1 = deviceDX
                input.value2 = deviceDY
            case let .zoomAt(factor, deviceX, deviceY):
                input.kind = inkpod_bridge_view_zoom_at()
                input.value1 = factor
                input.value2 = deviceX
                input.value3 = deviceY
            case let .fit(width, height):
                input.kind = inkpod_bridge_view_fit()
                input.value1 = width
                input.value2 = height
            case let .oneToOne(width, height):
                input.kind = inkpod_bridge_view_one_to_one()
                input.value1 = width
                input.value2 = height
            case let .boxZoom(documentX, documentY, width, height):
                input.kind = inkpod_bridge_view_box_zoom()
                input.value1 = Double(documentX)
                input.value2 = Double(documentY)
                input.value3 = Double(width)
                input.value4 = Double(height)
            case .flipHorizontal:
                input.kind = inkpod_bridge_view_flip_horizontal()
            case .flipVertical:
                input.kind = inkpod_bridge_view_flip_vertical()
            case let .setRulerVisible(value):
                input.kind = inkpod_bridge_view_set_ruler_visible()
                input.value1 = value ? 1 : 0
            case let .setGuidesVisible(value):
                input.kind = inkpod_bridge_view_set_guides_visible()
                input.value1 = value ? 1 : 0
            case let .setGridVisible(value):
                input.kind = inkpod_bridge_view_set_grid_visible()
                input.value1 = value ? 1 : 0
            case let .setGuideSnapEnabled(value):
                input.kind = inkpod_bridge_view_set_guide_snap_enabled()
                input.value1 = value ? 1 : 0
            case let .setGridSnapEnabled(value):
                input.kind = inkpod_bridge_view_set_grid_snap_enabled()
                input.value1 = value ? 1 : 0
            case let .setTransparentVisible(value):
                input.kind = inkpod_bridge_view_set_transparent_visible()
                input.value1 = value ? 1 : 0
            case let .setAlphaVisible(value):
                input.kind = inkpod_bridge_view_set_alpha_visible()
                input.value1 = value ? 1 : 0
            case let .setVectorAntialias(value):
                input.kind = inkpod_bridge_view_set_vector_antialias()
                input.value1 = value ? 1 : 0
            case let .setVectorCenterlineMode(mode):
                input.kind = inkpod_bridge_view_set_vector_centerline_mode()
                input.value1 = Double(mode)
            case let .setVectorEndpointsVisible(value):
                input.kind = inkpod_bridge_view_set_vector_endpoints_visible()
                input.value1 = value ? 1 : 0
            }
            let status: CoreStatus
            if view.coreViewID == 0 {
                var info = InkpodDocumentInfo()
                info.struct_size = UInt32(MemoryLayout<InkpodDocumentInfo>.size)
                status = CoreStatus(cValue: inkpod_core_apply_view(entry.core, &input, &info))
            } else {
                status = CoreStatus(
                    cValue: inkpod_core_view_apply(entry.core, view.coreViewID, &input)
                )
            }
            guard status == .ok else {
                return .failed(.coreOperation(status))
            }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard let updatedViewRevision = viewRevision(
                core: entry.core,
                coreViewID: view.coreViewID
            ) else {
                return .failed(.coreOperation(.panic))
            }
            if updatedViewRevision == beforeViewRevision {
                return .noOp(updated)
            }
            return view.coreViewID == 0
                ? .viewUpdated(updated)
                : .logicalViewUpdated(
                    CoreLogicalViewProjection(
                        session: updated,
                        target: target,
                        viewRevision: updatedViewRevision
                    )
                )
        }
    }

    private func applyDocument(
        target: CoreSessionTarget,
        command: CoreDocumentCommand,
        expectedDocumentRevision: UInt64
    ) -> CoreRequestOutcome {
        guard command.isValid else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var result = InkpodDispatchResult()
            result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
            let status: CoreStatus
            var affectedGuideID: UInt64?
            switch command {
            case let .addGuide(axis, position):
                var guideID: UInt64 = 0
                status = CoreStatus(cValue: inkpod_core_guide_add(
                    entry.core,
                    axis.rawValue,
                    position,
                    &result,
                    &guideID
                ))
                affectedGuideID = guideID == 0 ? nil : guideID
            case let .moveGuide(id, position):
                status = CoreStatus(cValue: inkpod_core_guide_move(
                    entry.core,
                    id,
                    position,
                    &result
                ))
                affectedGuideID = id
            case .deleteAllGuides:
                status = CoreStatus(cValue: inkpod_core_guide_delete_all(entry.core, &result))
            case let .setGrid(grid):
                var input = InkpodGridInput()
                input.struct_size = UInt32(MemoryLayout<InkpodGridInput>.size)
                input.origin_x = grid.originX
                input.origin_y = grid.originY
                input.spacing_x = grid.spacingX
                input.spacing_y = grid.spacingY
                input.subdivisions = grid.subdivisions
                status = CoreStatus(cValue: inkpod_core_grid_set(entry.core, &input, &result))
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if updated.documentRevision == before.documentRevision {
                return .noOp(updated)
            }
            return .documentCommandUpdated(
                CoreDocumentCommandProjection(
                    session: updated,
                    affectedGuideID: affectedGuideID
                )
            )
        }
    }

    private func inspectPaint(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64?
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let paint = paintProjection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if let expectedDocumentRevision,
               paint.editor.session.documentRevision != expectedDocumentRevision
            {
                return .failed(.staleTarget)
            }
            return .paint(paint)
        }
    }

    private func updateEditor(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        update: CoreEditorUpdate
    ) -> CoreRequestOutcome {
        guard update.isValid else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard validatePaintExpectation(expectation, entry: entry, view: view),
                let before = paintProjection(for: entry)
            else {
                return .failed(.staleTarget)
            }
            var input = InkpodEditorStateUpdate()
            input.struct_size = UInt32(MemoryLayout<InkpodEditorStateUpdate>.size)
            input.expected_editor_revision = expectation.editorRevision
            switch update {
            case let .activeTool(tool):
                input.kind = inkpod_bridge_editor_update_active_tool()
                input.tool = tool.rawValue
            case let .toolColor(color):
                input.kind = inkpod_bridge_editor_update_tool_color()
                input.tool = before.editor.activeTool.rawValue
                input.color = ffiColor(color)
            case let .diameter(value):
                input.kind = inkpod_bridge_editor_update_tool_diameter()
                input.tool = before.editor.activeTool.rawValue
                input.diameter_q16 = Int64((value * 65_536).rounded())
            case let .fillOptions(options):
                input.kind = inkpod_bridge_editor_update_fill_options()
                input.fill = ffiFillOptions(options)
            case let .brushOptions(options):
                input.kind = inkpod_bridge_editor_update_brush_options()
                input.brush.struct_size = UInt32(MemoryLayout<InkpodEditorBrushOptions>.size)
                input.brush.shape = options.shape.rawValue
                input.brush.smoothing = options.smoothing
                input.brush.start_color = options.startColor.rawValue
            case let .selectionOptions(options):
                input.kind = inkpod_bridge_editor_update_selection_options()
                input.selection = ffiSelectionOptions(options)
            }
            var output = InkpodEditorStateInfo()
            output.struct_size = UInt32(MemoryLayout<InkpodEditorStateInfo>.size)
            let status = CoreStatus(cValue: inkpod_core_update_editor_state(
                entry.core,
                &input,
                &output
            ))
            guard status == .ok else {
                return .failed(status == .invalidState
                    ? .staleTarget : .coreOperation(status))
            }
            guard let after = paintProjection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if after.editor.editorRevision == before.editor.editorRevision {
                return .noOp(after.editor.session)
            }
            return .paintUpdated(after)
        }
    }

    private func resolveDocumentPoints(
        target: CoreViewTarget,
        expectedDocumentRevision: UInt64,
        expectedViewRevision: UInt64,
        samples: [CorePointerSample]
    ) -> CoreRequestOutcome {
        guard !samples.isEmpty, samples.count <= 65_536,
              samples.allSatisfy(\.isValid)
        else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard projection(for: entry)?.documentRevision == expectedDocumentRevision,
                  viewRevision(core: entry.core, coreViewID: view.coreViewID)
                    == expectedViewRevision
            else { return .failed(.staleTarget) }
            let resolved = resolveDocumentPoints(
                core: entry.core,
                view: view,
                expectedViewRevision: expectedViewRevision,
                samples: samples
            )
            guard resolved.0 == .ok, resolved.1.count == samples.count else {
                return .failed(resolved.0 == .invalidArgument
                    ? .invalidRequest : .coreOperation(resolved.0))
            }
            return .documentPoints(resolved.1)
        }
    }

    private func beginRasterStroke(
        target: CoreViewTarget,
        expectation: CorePaintExpectation?,
        samples: [CorePointerSample]
    ) -> CoreRequestOutcome {
        guard validSamples(samples) else {
            return .failed(.invalidRequest)
        }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(resolvedEntry, view):
            var entry = resolvedEntry
            guard entry.activeTransient == nil else {
                return .noOp(projection(for: entry))
            }
            if let expectation {
                guard validatePaintExpectation(
                    expectation,
                    entry: entry,
                    view: view
                ) else {
                    return .failed(.staleTarget)
                }
            }
            var editor = InkpodEditorStateInfo()
            editor.struct_size = UInt32(MemoryLayout<InkpodEditorStateInfo>.size)
            guard CoreStatus(
                cValue: inkpod_core_get_editor_state(entry.core, &editor)
            ) == .ok,
                CoreEditorTool(rawValue: editor.active_tool).map({
                    $0 == .pencil || $0 == .brush || $0 == .eraser
                }) == true
            else {
                return .failed(.invalidRequest)
            }
            let ffiSamples = makeStrokeSamples(samples)
            var input = InkpodEditorStrokeInput()
            input.struct_size = UInt32(MemoryLayout<InkpodEditorStrokeInput>.size)
            input.coordinate_space = inkpod_bridge_coordinate_device()
            input.tool = editor.active_tool
            input.sample_count = UInt64(ffiSamples.count)
            input.sample_stride_bytes = UInt64(MemoryLayout<InkpodStrokeSample>.stride)
            let status = ffiSamples.withUnsafeBufferPointer { buffer in
                input.samples = buffer.baseAddress
                return CoreStatus(cValue: inkpod_core_editor_stroke_begin_for_view(
                    entry.core,
                    view.coreViewID,
                    &input
                ))
            }
            guard status == .ok else {
                return .failed(.coreOperation(status))
            }
            entry.activeTransient = .stroke
            sessions[target.session.id] = entry
            return .acknowledged
        }
    }

    private func appendRasterStroke(
        target: CoreViewTarget,
        samples: [CorePointerSample]
    ) -> CoreRequestOutcome {
        guard validSamples(samples) else {
            _ = cancelStroke(target: target)
            return .failed(.invalidRequest)
        }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case var .live(entry, _):
            guard entry.activeTransient == .stroke else {
                return .failed(.coreOperation(.invalidState))
            }
            let ffiSamples = makeStrokeSamples(samples)
            var span = InkpodStrokeSampleSpan()
            span.struct_size = UInt32(MemoryLayout<InkpodStrokeSampleSpan>.size)
            span.sample_count = UInt64(ffiSamples.count)
            span.sample_stride_bytes = UInt64(MemoryLayout<InkpodStrokeSample>.stride)
            let status = ffiSamples.withUnsafeBufferPointer { buffer in
                span.samples = buffer.baseAddress
                return CoreStatus(cValue: inkpod_core_stroke_append(entry.core, &span))
            }
            guard status == .ok else {
                _ = inkpod_core_stroke_cancel(entry.core)
                entry.activeTransient = nil
                sessions[target.session.id] = entry
                return .failed(.coreOperation(status))
            }
            return .acknowledged
        }
    }

    private func endStroke(target: CoreViewTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case var .live(entry, _):
            guard entry.activeTransient == .stroke else {
                return .failed(.coreOperation(.invalidState))
            }
            let beforeRevision = projection(for: entry)?.documentRevision
            var result = InkpodDispatchResult()
            result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
            let status = CoreStatus(cValue: inkpod_core_stroke_end(entry.core, &result))
            entry.activeTransient = nil
            sessions[target.session.id] = entry
            guard status == .ok else {
                return .failed(.coreOperation(status))
            }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            return beforeRevision == updated.documentRevision
                ? .noOp(updated) : .documentUpdated(updated)
        }
    }

    private func cancelStroke(target: CoreViewTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case var .live(entry, _):
            let status = CoreStatus(cValue: inkpod_core_stroke_cancel(entry.core))
            guard status == .ok else {
                return .failed(.coreOperation(status))
            }
            entry.activeTransient = nil
            sessions[target.session.id] = entry
            return .acknowledged
        }
    }

    private func applyFill(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        gesture: CoreFillGesture
    ) -> CoreRequestOutcome {
        guard gesture.isValid else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard validatePaintExpectation(expectation, entry: entry, view: view),
                  let editor = editorProjection(for: entry),
                  entry.activeTransient == nil
            else {
                return .failed(.staleTarget)
            }
            let resolved = resolveDocumentPoints(
                core: entry.core,
                view: view,
                expectedViewRevision: expectation.viewRevision,
                samples: [gesture.start, gesture.end]
            )
            guard resolved.0 == .ok, resolved.1.count == 2,
                  let seedX = documentPixel(
                    resolved.1[0].x,
                    upperBound: editor.session.documentWidth
                  ),
                  let seedY = documentPixel(
                    resolved.1[0].y,
                    upperBound: editor.session.documentHeight
                  )
            else {
                return .failed(resolved.0 == .ok
                    ? .invalidRequest : .coreOperation(resolved.0))
            }
            var input = InkpodFillInput()
            input.struct_size = UInt32(MemoryLayout<InkpodFillInput>.size)
            input.operation = editor.fillOptions.operation.rawValue
            input.flags = inkpod_bridge_fill_flags(
                editor.fillOptions.detachedRegions ? 1 : 0,
                editor.fillOptions.overflowAbort ? 1 : 0,
                editor.fillOptions.transparentOnly ? 1 : 0,
                editor.fillOptions.operation == .seed ? 0 : 1,
                editor.fillOptions.useDocumentSelection ? 1 : 0,
                editor.fillOptions.useLightTableBoundary ? 1 : 0,
                editor.fillOptions.useLightTableColor ? 1 : 0
            )
            input.seed_x = seedX
            input.seed_y = seedY
            input.color = ffiColor(editor.currentColor)
            input.tolerance = editor.fillOptions.tolerance
            input.gap_close = editor.fillOptions.gapClose
            input.inclusion_mode = editor.fillOptions.inclusionMode.rawValue
            input.extension_distance = editor.fillOptions.extensionDistance
            if editor.fillOptions.operation != .seed {
                guard let bounds = documentBounds(
                    from: resolved.1[0],
                    to: resolved.1[1],
                    width: editor.session.documentWidth,
                    height: editor.session.documentHeight
                ) else {
                    return .failed(.invalidRequest)
                }
                input.selection = ffiFrame(bounds)
            }
            let inclusion = editor.fillOptions.inclusionColors.map(ffiColor)
            input.inclusion_color_count = UInt64(inclusion.count)
            input.inclusion_color_stride_bytes = inclusion.isEmpty
                ? 0 : UInt64(MemoryLayout<InkpodColorValue>.stride)
            var result = InkpodFillResult()
            result.struct_size = UInt32(MemoryLayout<InkpodFillResult>.size)
            let status = inclusion.withUnsafeBufferPointer { buffer in
                input.inclusion_colors = inclusion.isEmpty ? nil : buffer.baseAddress
                return CoreStatus(cValue: inkpod_core_apply_fill_for_editor_target(
                    entry.core,
                    expectation.layerID,
                    expectation.planeID,
                    &input,
                    &result
                ))
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if result.changed_pixel_count == 0 { return .noOp(updated) }
            let leak = result.flags & inkpod_bridge_fill_result_leak_candidate() != 0
                ? (x: result.leak_x, y: result.leak_y) : nil
            return .fillApplied(CoreFillProjection(
                session: updated,
                changedPixelCount: result.changed_pixel_count,
                leakCandidate: leak
            ))
        }
    }

    private func eyedropper(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        source: CoreEyedropperSource,
        devicePoint: CorePointerSample
    ) -> CoreRequestOutcome {
        guard devicePoint.isValid else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard validatePaintExpectation(expectation, entry: entry, view: view),
                  let before = paintProjection(for: entry)
            else {
                return .failed(.staleTarget)
            }
            let resolved = resolveDocumentPoints(
                core: entry.core,
                view: view,
                expectedViewRevision: expectation.viewRevision,
                samples: [devicePoint]
            )
            guard resolved.0 == .ok, let point = resolved.1.first,
                  let x = documentPixel(point.x, upperBound: before.editor.session.documentWidth),
                  let y = documentPixel(point.y, upperBound: before.editor.session.documentHeight)
            else {
                return .failed(resolved.0 == .ok
                    ? .invalidRequest : .coreOperation(resolved.0))
            }
            var sampled = InkpodColorValue()
            sampled.struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
            let sampleStatus = CoreStatus(cValue: inkpod_core_eyedropper(
                entry.core,
                source.rawValue,
                x,
                y,
                &sampled
            ))
            guard sampleStatus == .ok, let color = coreColor(sampled) else {
                return .failed(.coreOperation(sampleStatus))
            }
            var update = InkpodEditorStateUpdate()
            update.struct_size = UInt32(MemoryLayout<InkpodEditorStateUpdate>.size)
            update.kind = inkpod_bridge_editor_update_tool_color()
            update.expected_editor_revision = expectation.editorRevision
            update.tool = before.editor.activeTool.rawValue
            update.color = ffiColor(color)
            var output = InkpodEditorStateInfo()
            output.struct_size = UInt32(MemoryLayout<InkpodEditorStateInfo>.size)
            let updateStatus = CoreStatus(cValue: inkpod_core_update_editor_state(
                entry.core,
                &update,
                &output
            ))
            guard updateStatus == .ok, let after = paintProjection(for: entry) else {
                return .failed(.coreOperation(updateStatus))
            }
            return .eyedropperSampled(after)
        }
    }

    private func replacePalette(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        colors: [CoreColorValue]
    ) -> CoreRequestOutcome {
        guard colors.count <= 4_096, colors.allSatisfy(\.hasValidNativeComponents) else {
            return .failed(.invalidRequest)
        }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            let values = colors.map(ffiColor)
            var input = InkpodColorArray()
            input.struct_size = UInt32(MemoryLayout<InkpodColorArray>.size)
            input.color_count = UInt64(values.count)
            input.color_stride_bytes = values.isEmpty
                ? 0 : UInt64(MemoryLayout<InkpodColorValue>.stride)
            var result = dispatchResult()
            let status = values.withUnsafeBufferPointer { buffer in
                input.colors = values.isEmpty ? nil : buffer.baseAddress
                return CoreStatus(cValue: inkpod_core_palette_set(
                    entry.core,
                    &input,
                    &result
                ))
            }
            return paintMutationOutcome(status: status, before: expectedDocumentRevision, entry: entry)
        }
    }

    private func generatePalette(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        maximumColors: UInt32,
        quantizationBits: UInt32
    ) -> CoreRequestOutcome {
        guard (1 ... 4_096).contains(maximumColors), quantizationBits <= 7 else {
            return .failed(.invalidRequest)
        }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var result = dispatchResult()
            let status = CoreStatus(cValue: inkpod_core_palette_generate(
                entry.core,
                maximumColors,
                quantizationBits,
                &result
            ))
            return paintMutationOutcome(status: status, before: expectedDocumentRevision, entry: entry)
        }
    }

    private func savePaletteFile(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreRequestOutcome {
        guard validPath(pathUTF8) else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            guard let palette = paletteProjection(core: entry.core) else {
                return .failed(.coreOperation(.panic))
            }
            let colors = palette.colors.map(ffiColor)
            var input = InkpodColorArray()
            input.struct_size = UInt32(MemoryLayout<InkpodColorArray>.size)
            input.color_count = UInt64(colors.count)
            input.color_stride_bytes = colors.isEmpty
                ? 0 : UInt64(MemoryLayout<InkpodColorValue>.stride)
            let status = colors.withUnsafeBufferPointer { buffer in
                input.colors = colors.isEmpty ? nil : buffer.baseAddress
                return withPath(pathUTF8) { path, count in
                    CoreStatus(cValue: inkpod_palette_file_save(path, count, &input))
                }
            }
            return status == .ok ? .acknowledged : .failed(.coreOperation(status))
        }
    }

    private func loadPaletteFile(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreRequestOutcome {
        guard validPath(pathUTF8) else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var buffer = InkpodColorBuffer()
            buffer.struct_size = UInt32(MemoryLayout<InkpodColorBuffer>.size)
            var status = withPath(pathUTF8) { path, count in
                CoreStatus(cValue: inkpod_palette_file_load(path, count, &buffer))
            }
            guard status == .ok || status == .bufferTooSmall,
                  buffer.color_count <= 4_096,
                  buffer.color_count <= UInt64(Int.max)
            else {
                return .failed(.coreOperation(status))
            }
            var colors = [InkpodColorValue](
                repeating: InkpodColorValue(),
                count: Int(buffer.color_count)
            )
            for index in colors.indices {
                colors[index].struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
            }
            if !colors.isEmpty {
                status = colors.withUnsafeMutableBufferPointer { storage in
                    buffer.colors = storage.baseAddress
                    buffer.color_capacity = UInt64(storage.count)
                    buffer.color_stride_bytes = UInt64(MemoryLayout<InkpodColorValue>.stride)
                    return withPath(pathUTF8) { path, count in
                        CoreStatus(cValue: inkpod_palette_file_load(path, count, &buffer))
                    }
                }
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            var input = InkpodColorArray()
            input.struct_size = UInt32(MemoryLayout<InkpodColorArray>.size)
            input.color_count = UInt64(colors.count)
            input.color_stride_bytes = colors.isEmpty
                ? 0 : UInt64(MemoryLayout<InkpodColorValue>.stride)
            var result = dispatchResult()
            status = colors.withUnsafeBufferPointer { storage in
                input.colors = colors.isEmpty ? nil : storage.baseAddress
                return CoreStatus(cValue: inkpod_core_palette_set(
                    entry.core,
                    &input,
                    &result
                ))
            }
            return paintMutationOutcome(status: status, before: expectedDocumentRevision, entry: entry)
        }
    }

    private func replaceColorChart(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        entries: [CoreColorChartEntry],
        locked: Bool
    ) -> CoreRequestOutcome {
        guard entries.count <= 4_096,
              entries.allSatisfy({
                  $0.color.hasValidNativeComponents && $0.name.utf8.count <= 4_096
              })
        else {
            return .failed(.invalidRequest)
        }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var result = dispatchResult()
            let status = withFFIColorChartEntries(entries) { ffiEntries in
                CoreStatus(cValue: inkpod_core_color_chart_set(
                    entry.core,
                    ffiEntries.baseAddress,
                    UInt64(ffiEntries.count),
                    ffiEntries.isEmpty
                        ? 0 : UInt64(MemoryLayout<InkpodColorChartEntry>.stride),
                    locked ? 1 : 0,
                    &result
                ))
            }
            return paintMutationOutcome(status: status, before: expectedDocumentRevision, entry: entry)
        }
    }

    private func saveColorChartFile(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreRequestOutcome {
        guard validPath(pathUTF8) else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            guard let chart = colorChartProjection(core: entry.core) else {
                return .failed(.coreOperation(.panic))
            }
            let status = withFFIColorChartEntries(chart.entries) { entries in
                withPath(pathUTF8) { path, count in
                    CoreStatus(cValue: inkpod_color_chart_file_save(
                        path,
                        count,
                        entries.baseAddress,
                        UInt64(entries.count),
                        entries.isEmpty
                            ? 0 : UInt64(MemoryLayout<InkpodColorChartEntry>.stride)
                    ))
                }
            }
            return status == .ok ? .acknowledged : .failed(.coreOperation(status))
        }
    }

    private func loadColorChartFile(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreRequestOutcome {
        guard validPath(pathUTF8) else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var chart: OpaquePointer?
            let loadStatus = withPath(pathUTF8) { path, count in
                CoreStatus(cValue: inkpod_color_chart_file_load(path, count, &chart))
            }
            guard loadStatus == .ok, let rawChart = chart else {
                if chart != nil { _ = inkpod_color_chart_file_release(&chart) }
                return .failed(.coreOperation(loadStatus))
            }
            defer { _ = inkpod_color_chart_file_release(&chart) }
            var count: UInt64 = 0
            guard CoreStatus(cValue: inkpod_color_chart_file_count(rawChart, &count)) == .ok,
                  count <= 4_096
            else {
                return .failed(.coreOperation(.invalidArgument))
            }
            var entries: [CoreColorChartEntry] = []
            entries.reserveCapacity(Int(count))
            for index in 0 ..< count {
                var color = InkpodColorValue()
                color.struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
                var nameCount: UInt64 = 0
                var status = CoreStatus(cValue: inkpod_color_chart_file_get(
                    rawChart,
                    index,
                    &color,
                    nil,
                    0,
                    &nameCount
                ))
                guard status == .ok || status == .bufferTooSmall,
                      nameCount <= 4_096,
                      nameCount <= UInt64(Int.max)
                else {
                    return .failed(.coreOperation(status))
                }
                var bytes = [UInt8](repeating: 0, count: Int(nameCount))
                status = CoreStatus(cValue: bytes.withUnsafeMutableBufferPointer { storage in
                    inkpod_color_chart_file_get(
                        rawChart,
                        index,
                        &color,
                        storage.baseAddress,
                        UInt64(storage.count),
                        &nameCount
                    )
                })
                guard status == .ok,
                      let value = coreColor(color),
                      let name = String(bytes: bytes, encoding: .utf8)
                else {
                    return .failed(.coreOperation(status))
                }
                entries.append(CoreColorChartEntry(index: index, color: value, name: name))
            }
            var result = dispatchResult()
            let setStatus = withFFIColorChartEntries(entries) { ffiEntries in
                CoreStatus(cValue: inkpod_core_color_chart_set(
                    entry.core,
                    ffiEntries.baseAddress,
                    UInt64(ffiEntries.count),
                    ffiEntries.isEmpty
                        ? 0 : UInt64(MemoryLayout<InkpodColorChartEntry>.stride),
                    0,
                    &result
                ))
            }
            return paintMutationOutcome(
                status: setStatus,
                before: expectedDocumentRevision,
                entry: entry
            )
        }
    }

    private func createColorChartPreview(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        maximumColors: UInt32,
        quantizationBits: UInt32,
        requestID: CoreRequestID
    ) -> CoreRequestOutcome {
        guard (1 ... 4_096).contains(maximumColors), quantizationBits <= 7 else {
            return .failed(.invalidRequest)
        }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard projection(for: entry)?.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var task: OpaquePointer?
            let taskStatus = CoreStatus(cValue: inkpod_task_create(&task))
            guard taskStatus == .ok, let rawTask = task else {
                return .failed(.coreOperation(taskStatus))
            }
            cancellations.begin(requestID, task: rawTask)
            defer {
                cancellations.finish(requestID)
                _ = inkpod_task_release(&task)
            }
            var summary = InkpodColorChartPreviewSummary()
            summary.struct_size = UInt32(MemoryLayout<InkpodColorChartPreviewSummary>.size)
            var preview: OpaquePointer?
            let status = CoreStatus(cValue: inkpod_core_color_chart_preview_create_task(
                entry.core,
                maximumColors,
                quantizationBits,
                rawTask,
                &summary,
                &preview
            ))
            guard status == .ok, let rawPreview = preview else {
                if preview != nil { _ = inkpod_color_chart_preview_release(&preview) }
                return .failed(status == .cancelled ? .cancelled : .coreOperation(status))
            }
            guard summary.base_document_revision == expectedDocumentRevision,
                  summary.entry_count <= 4_096,
                  let entries = colorChartPreviewEntries(
                    preview: rawPreview,
                    count: summary.entry_count
                  ),
                  nextColorChartPreviewID != 0,
                  nextColorChartPreviewID < UInt64.max
            else {
                _ = inkpod_color_chart_preview_release(&preview)
                return .failed(.coreOperation(.panic))
            }
            let previewID = CoreColorChartPreviewID(rawValue: nextColorChartPreviewID)
            nextColorChartPreviewID += 1
            colorChartPreviews[previewID] = CoreColorChartPreviewEntry(
                raw: rawPreview,
                session: target,
                baseDocumentRevision: expectedDocumentRevision
            )
            preview = nil
            return .colorChartPreview(CoreColorChartPreviewProjection(
                id: previewID,
                session: target,
                baseDocumentRevision: expectedDocumentRevision,
                entries: entries,
                sourceUniqueColorCount: summary.source_unique_color_count,
                retainedColorCount: summary.retained_color_count,
                addedColorCount: summary.added_color_count,
                removedColorCount: summary.removed_color_count,
                exceedsMaximum: summary.flags
                    & inkpod_bridge_color_chart_preview_exceeds_maximum() != 0
            ))
        }
    }

    private func applyColorChartPreview(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        previewID: CoreColorChartPreviewID
    ) -> CoreRequestOutcome {
        guard let owned = colorChartPreviews[previewID] else {
            return .failed(.staleTarget)
        }
        guard owned.session == target,
              owned.baseDocumentRevision == expectedDocumentRevision
        else {
            return .failed(.staleTarget)
        }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var result = dispatchResult()
            let status = CoreStatus(cValue: inkpod_core_color_chart_preview_apply(
                entry.core,
                owned.raw,
                &result
            ))
            guard status == .ok else {
                return .failed(status == .cancelled ? .cancelled : .coreOperation(status))
            }
            releaseColorChartPreview(previewID)
            return paintMutationOutcome(status: status, before: expectedDocumentRevision, entry: entry)
        }
    }

    private func cancelColorChartPreview(
        _ previewID: CoreColorChartPreviewID
    ) -> CoreRequestOutcome {
        guard colorChartPreviews[previewID] != nil else { return .noOp(nil) }
        return releaseColorChartPreview(previewID)
            ? .acknowledged : .failed(.coreOperation(.panic))
    }

    private func setColorCheck(
        target: CoreViewTarget,
        expectedViewRevision: UInt64,
        mode: CoreColorCheckMode
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case .live(var entry, let view):
            guard viewRevision(core: entry.core, coreViewID: view.coreViewID)
                == expectedViewRevision
            else {
                return .failed(.staleTarget)
            }
            if entry.colorCheckMode == mode { return .noOp(projection(for: entry)) }
            let status = CoreStatus(cValue: inkpod_core_set_color_check(
                entry.core,
                mode.rawValue
            ))
            guard status == .ok else { return .failed(.coreOperation(status)) }
            entry.colorCheckMode = mode
            sessions[target.session.id] = entry
            guard let paint = paintProjection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            return .paintUpdated(paint)
        }
    }

    private func inspectLocator(
        target: CoreViewTarget,
        expectedViewRevision: UInt64,
        devicePoint: CorePointerSample,
        radius: UInt32
    ) -> CoreRequestOutcome {
        guard devicePoint.isValid, radius <= 16 else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard viewRevision(core: entry.core, coreViewID: view.coreViewID)
                == expectedViewRevision
            else {
                return .failed(.staleTarget)
            }
            var sample = InkpodLocatorOutput()
            sample.struct_size = UInt32(MemoryLayout<InkpodLocatorOutput>.size)
            let sampleStatus = CoreStatus(cValue: inkpod_core_locator_sample(
                entry.core,
                view.coreViewID,
                Double(devicePoint.deviceX),
                Double(devicePoint.deviceY),
                &sample
            ))
            guard sampleStatus == .ok else {
                return .failed(.coreOperation(sampleStatus))
            }
            var neighborhood = InkpodLocatorNeighborhoodBuffer()
            neighborhood.struct_size = UInt32(
                MemoryLayout<InkpodLocatorNeighborhoodBuffer>.size
            )
            neighborhood.radius = radius
            var status = CoreStatus(cValue: inkpod_core_locator_neighborhood(
                entry.core,
                view.coreViewID,
                Double(devicePoint.deviceX),
                Double(devicePoint.deviceY),
                &neighborhood
            ))
            guard status == .bufferTooSmall || status == .ok,
                  neighborhood.required_bytes <= UInt64(Int.max),
                  neighborhood.required_bytes <= 4_356
            else {
                return .failed(.coreOperation(status))
            }
            var pixels = [UInt8](
                repeating: 0,
                count: Int(neighborhood.required_bytes)
            )
            status = CoreStatus(cValue: pixels.withUnsafeMutableBufferPointer { buffer in
                neighborhood.pixels_rgba8 = buffer.baseAddress
                neighborhood.pixel_capacity = UInt64(buffer.count)
                return inkpod_core_locator_neighborhood(
                    entry.core,
                    view.coreViewID,
                    Double(devicePoint.deviceX),
                    Double(devicePoint.deviceY),
                    &neighborhood
                )
            })
            guard status == .ok else { return .failed(.coreOperation(status)) }
            let selection = sample.flags & inkpod_bridge_locator_selection_present() != 0
                ? coreFrame(sample.selection) : nil
            let color: CoreColorValue?
            if sample.flags & inkpod_bridge_locator_color_present() != 0 {
                guard let value = coreColor(sample.color) else {
                    return .failed(.coreOperation(.panic))
                }
                color = value
            } else {
                color = nil
            }
            return .locator(CoreLocatorProjection(
                documentX: sample.document_x,
                documentY: sample.document_y,
                selection: selection,
                color: color,
                neighborhoodOriginX: neighborhood.origin_x,
                neighborhoodOriginY: neighborhood.origin_y,
                neighborhoodWidth: neighborhood.width,
                neighborhoodHeight: neighborhood.height,
                neighborhoodRGBA8: pixels
            ))
        }
    }

    private func paintLocatorPixel(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        documentX: Int32,
        documentY: Int32
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard validatePaintExpectation(expectation, entry: entry, view: view),
                  entry.activeTransient == nil,
                  documentX >= 0, documentY >= 0,
                  let session = projection(for: entry),
                  UInt64(documentX) < session.documentWidth,
                  UInt64(documentY) < session.documentHeight
            else {
                return .failed(.staleTarget)
            }
            var sample = InkpodStrokeSample()
            sample.struct_size = UInt32(MemoryLayout<InkpodStrokeSample>.size)
            sample.x = Float(documentX) + 0.5
            sample.y = Float(documentY) + 0.5
            sample.pressure = 1
            var input = InkpodEditorStrokeInput()
            input.struct_size = UInt32(MemoryLayout<InkpodEditorStrokeInput>.size)
            input.coordinate_space = inkpod_bridge_coordinate_document()
            input.tool = CoreEditorTool.pencil.rawValue
            input.flags = inkpod_bridge_stroke_auto_erase()
            input.sample_count = 1
            input.sample_stride_bytes = UInt64(MemoryLayout<InkpodStrokeSample>.stride)
            let begin = withUnsafePointer(to: &sample) { pointer in
                input.samples = pointer
                return CoreStatus(cValue: inkpod_core_editor_stroke_begin_for_view(
                    entry.core,
                    view.coreViewID,
                    &input
                ))
            }
            guard begin == .ok else { return .failed(.coreOperation(begin)) }
            var result = InkpodDispatchResult()
            result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
            let end = CoreStatus(cValue: inkpod_core_stroke_end(entry.core, &result))
            guard end == .ok else {
                _ = inkpod_core_stroke_cancel(entry.core)
                return .failed(.coreOperation(end))
            }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            return updated.documentRevision == session.documentRevision
                ? .noOp(updated) : .documentUpdated(updated)
        }
    }

    private func colorReplace(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        request: CoreColorReplaceRequest,
        commit: Bool
    ) -> CoreRequestOutcome {
        guard request.isValid else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard validatePaintExpectation(expectation, entry: entry, view: view) else {
                return .failed(.staleTarget)
            }
            let samples = colorReplaceSamples(request.region)
            let resolved: (CoreStatus, [CoreDocumentPoint])
            if samples.isEmpty {
                resolved = (.ok, [])
            } else {
                resolved = resolveDocumentPoints(
                    core: entry.core,
                    view: view,
                    expectedViewRevision: expectation.viewRevision,
                    samples: samples
                )
            }
            guard resolved.0 == .ok else { return .failed(.coreOperation(resolved.0)) }
            var input = InkpodScopedColorReplaceInput()
            input.struct_size = UInt32(MemoryLayout<InkpodScopedColorReplaceInput>.size)
            input.mode = request.mode.rawValue
            input.plane_id = expectation.planeID
            input.base_document_revision = expectation.documentRevision
            input.target_color = ffiColor(request.targetColor)
            input.replacement_color = ffiColor(request.replacementColor)
            var points: [InkpodSelectionPoint] = []
            switch request.region {
            case .entireSelectionOrDocument:
                break
            case .rectangle:
                guard resolved.1.count == 2,
                      let bounds = documentBounds(
                        from: resolved.1[0],
                        to: resolved.1[1],
                        width: editorProjection(for: entry)?.session.documentWidth ?? 0,
                        height: editorProjection(for: entry)?.session.documentHeight ?? 0
                      )
                else {
                    return .failed(.invalidRequest)
                }
                input.feature_flags = inkpod_bridge_color_replace_has_region()
                input.shape = 1
                input.bounds = ffiFrame(bounds)
            case let .pen(_, diameter):
                input.feature_flags = inkpod_bridge_color_replace_has_region()
                input.shape = 5
                input.diameter = diameter
                points = selectionPoints(resolved.1, source: samples)
            case .polyline:
                input.feature_flags = inkpod_bridge_color_replace_has_region()
                input.shape = 4
                points = selectionPoints(resolved.1, source: samples)
            case .lasso:
                input.feature_flags = inkpod_bridge_color_replace_has_region()
                input.shape = 3
                points = selectionPoints(resolved.1, source: samples)
            }
            input.point_count = UInt64(points.count)
            input.point_stride_bytes = points.isEmpty
                ? 0 : UInt64(MemoryLayout<InkpodSelectionPoint>.stride)
            if commit {
                var result = dispatchResult()
                let status = points.withUnsafeBufferPointer { buffer in
                    input.points = points.isEmpty ? nil : buffer.baseAddress
                    return CoreStatus(cValue: inkpod_core_apply_scoped_color_replace(
                        entry.core,
                        &input,
                        &result
                    ))
                }
                guard status == .ok else { return .failed(.coreOperation(status)) }
                guard let updated = projection(for: entry) else {
                    return .failed(.coreOperation(.panic))
                }
                return updated.documentRevision == expectation.documentRevision
                    ? .noOp(updated) : .documentUpdated(updated)
            }
            var preview = InkpodScopedColorReplacePreview()
            preview.struct_size = UInt32(MemoryLayout<InkpodScopedColorReplacePreview>.size)
            let status = points.withUnsafeBufferPointer { buffer in
                input.points = points.isEmpty ? nil : buffer.baseAddress
                return CoreStatus(cValue: inkpod_core_preview_scoped_color_replace(
                    entry.core,
                    &input,
                    &preview
                ))
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            return .colorReplacePreview(CoreColorReplacePreviewProjection(
                baseDocumentRevision: preview.base_document_revision,
                matchedPixels: preview.matched_pixels,
                matchedObjects: preview.matched_objects,
                affectedBounds: preview.feature_flags
                    & inkpod_bridge_color_replace_preview_has_bounds() != 0
                    ? coreFrame(preview.affected_bounds) : nil
            ))
        }
    }

    private func selectOutputColorGuard(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        operation: CoreSelectionOperation,
        requestID: CoreRequestID
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard projection(for: entry)?.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var task: OpaquePointer?
            let createStatus = CoreStatus(cValue: inkpod_task_create(&task))
            guard createStatus == .ok, let rawTask = task else {
                return .failed(.coreOperation(createStatus))
            }
            cancellations.begin(requestID, task: rawTask)
            defer {
                cancellations.finish(requestID)
                _ = inkpod_task_release(&task)
            }
            var input = InkpodOutputColorGuardRequest()
            input.struct_size = UInt32(MemoryLayout<InkpodOutputColorGuardRequest>.size)
            input.profile = inkpod_bridge_output_guard_profile()
            input.operation = operation.rawValue
            input.base_document_revision = expectedDocumentRevision
            var result = InkpodOutputColorGuardResult()
            result.struct_size = UInt32(MemoryLayout<InkpodOutputColorGuardResult>.size)
            let status = CoreStatus(cValue: inkpod_core_select_output_color_guard(
                entry.core,
                &input,
                rawTask,
                &result
            ))
            guard status == .ok else {
                return .failed(status == .cancelled ? .cancelled : .coreOperation(status))
            }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if updated.documentRevision == expectedDocumentRevision {
                return .noOp(updated)
            }
            return .outputColorGuardApplied(CoreOutputColorGuardProjection(
                session: updated,
                scannedPixelCount: result.scanned_pixel_count,
                selectedPixelCount: result.selected_pixel_count,
                transparentPixelCount: result.transparent_pixel_count
            ))
        }
    }

    private func editCell(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        command: CoreCellEditCommand
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var result = InkpodDispatchResult()
            result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
            let status: CoreStatus
            switch command {
            case let .updatePaperFrames(frames):
                var input = InkpodPaperFramesInput()
                input.struct_size = UInt32(MemoryLayout<InkpodPaperFramesInput>.size)
                input.hundred_frame = ffiFrame(frames.hundred)
                input.reference_frame = ffiFrame(frames.reference)
                input.drawing_frame = ffiFrame(frames.drawing)
                input.safe_frame = ffiFrame(frames.safe)
                input.shooting_frame = ffiFrame(frames.shooting)
                input.maximum_close_frame = ffiFrame(frames.maximumClose)
                input.margin_left = frames.margins.left
                input.margin_top = frames.margins.top
                input.margin_right = frames.margins.right
                input.margin_bottom = frames.margins.bottom
                status = CoreStatus(
                    cValue: inkpod_core_update_paper_frames(entry.core, &input, &result)
                )
            case let .mirror(axis):
                let ffiAxis = axis == .horizontal
                    ? inkpod_bridge_mirror_horizontal()
                    : inkpod_bridge_mirror_vertical()
                status = CoreStatus(
                    cValue: inkpod_core_mirror_document(entry.core, ffiAxis, &result)
                )
            case let .rotate(turn):
                let direction = turn == .left
                    ? inkpod_bridge_rotate_left()
                    : inkpod_bridge_rotate_right()
                status = CoreStatus(
                    cValue: inkpod_core_rotate_document(entry.core, direction, &result)
                )
            case let .resize(resize):
                var input = ffiResize(resize)
                status = CoreStatus(
                    cValue: inkpod_core_resize_document(entry.core, &input, &result)
                )
            case .fitPaperToFrames:
                let frames = [
                    before.paperFrames.hundred,
                    before.paperFrames.reference,
                    before.paperFrames.drawing,
                    before.paperFrames.safe,
                ]
                let maximumRight = frames.reduce(UInt64(before.documentWidth)) { partial, frame in
                    let right = max(Int64(0), Int64(frame.x) + Int64(frame.width))
                    return max(partial, UInt64(right))
                }
                let maximumBottom = frames.reduce(UInt64(before.documentHeight)) { partial, frame in
                    let bottom = max(Int64(0), Int64(frame.y) + Int64(frame.height))
                    return max(partial, UInt64(bottom))
                }
                guard maximumRight <= UInt64(UInt32.max),
                      maximumBottom <= UInt64(UInt32.max)
                else {
                    return .failed(.invalidRequest)
                }
                var input = ffiResize(CoreDocumentResize(
                    width: UInt32(maximumRight),
                    height: UInt32(maximumBottom),
                    dpiXMilli: before.dpiXMilli,
                    dpiYMilli: before.dpiYMilli,
                    anchor: .topLeft,
                    resample: false
                ))
                status = CoreStatus(
                    cValue: inkpod_core_resize_document(entry.core, &input, &result)
                )
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            return updated.documentRevision == before.documentRevision
                ? .noOp(updated)
                : .cellUpdated(updated)
        }
    }

    private func inspectTree(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64?
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let tree = treeProjection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if let expectedDocumentRevision,
               tree.session.documentRevision != expectedDocumentRevision
            {
                return .failed(.staleTarget)
            }
            return .tree(tree)
        }
    }

    private func setActiveNode(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        layerID: UInt64,
        planeID: UInt64
    ) -> CoreRequestOutcome {
        guard layerID != 0, planeID != 0 else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = treeProjection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.session.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            let status = CoreStatus(
                cValue: inkpod_core_set_active_node(entry.core, layerID, planeID)
            )
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let updated = treeProjection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if updated.editorRevision == before.editorRevision {
                return .noOp(updated.session)
            }
            return .treeUpdated(
                CoreTreeMutationProjection(tree: updated, affectedObjectID: planeID)
            )
        }
    }

    private func editTree(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        command: CoreTreeEditCommand
    ) -> CoreRequestOutcome {
        guard command.hasValidFrontendBounds else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var input = InkpodTreeEdit()
            input.struct_size = UInt32(MemoryLayout<InkpodTreeEdit>.size)
            var name: [UInt8] = []
            switch command {
            case let .createLayer(kind, pixelFormat, value):
                input.operation = inkpod_bridge_tree_create_layer()
                input.kind = kind.rawValue
                input.pixel_format = pixelFormat.rawValue
                name = Array(value.utf8)
            case let .duplicateLayer(id):
                input.operation = inkpod_bridge_tree_duplicate_layer()
                input.object_id = id
            case let .deleteLayer(id):
                input.operation = inkpod_bridge_tree_delete_layer()
                input.object_id = id
            case let .reorderLayer(id, destinationIndex):
                input.operation = inkpod_bridge_tree_reorder_layer()
                input.object_id = id
                input.destination_index = destinationIndex
            case let .setLayerProperties(id, visible, editable, opacity, value):
                input.operation = inkpod_bridge_tree_set_layer_properties()
                input.object_id = id
                input.flags = nodeFlags(visible: visible, editable: editable)
                input.opacity_milli = opacity
                name = Array(value.utf8)
            case let .convertLayer(id, kind, pixelFormat):
                input.operation = inkpod_bridge_tree_convert_layer()
                input.object_id = id
                input.kind = kind.rawValue
                input.pixel_format = pixelFormat.rawValue
            case let .mergeLayer(id):
                input.operation = inkpod_bridge_tree_merge_layer()
                input.object_id = id
            case .deleteHiddenLayers:
                input.operation = inkpod_bridge_tree_delete_hidden_layers()
            case let .createPlane(parent, kind, pixelFormat, value):
                input.operation = inkpod_bridge_tree_create_plane()
                input.parent_id = parent
                input.kind = kind.rawValue
                input.pixel_format = pixelFormat.rawValue
                name = Array(value.utf8)
            case let .duplicatePlane(id, parent):
                input.operation = inkpod_bridge_tree_duplicate_plane()
                input.object_id = id
                input.parent_id = parent
            case let .deletePlane(id, parent):
                input.operation = inkpod_bridge_tree_delete_plane()
                input.object_id = id
                input.parent_id = parent
            case let .reorderPlane(id, parent, destinationIndex):
                input.operation = inkpod_bridge_tree_reorder_plane()
                input.object_id = id
                input.parent_id = parent
                input.destination_index = destinationIndex
            case let .setPlaneProperties(id, parent, visible, editable, opacity, value):
                input.operation = inkpod_bridge_tree_set_plane_properties()
                input.object_id = id
                input.parent_id = parent
                input.flags = nodeFlags(visible: visible, editable: editable)
                input.opacity_milli = opacity
                name = Array(value.utf8)
            case let .convertPlane(id, parent, kind, pixelFormat):
                input.operation = inkpod_bridge_tree_convert_plane()
                input.object_id = id
                input.parent_id = parent
                input.kind = kind.rawValue
                input.pixel_format = pixelFormat.rawValue
            case let .mergePlane(id, parent):
                input.operation = inkpod_bridge_tree_merge_plane()
                input.object_id = id
                input.parent_id = parent
            }
            var result = InkpodDispatchResult()
            result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
            var affectedObjectID: UInt64 = 0
            let status = name.withUnsafeBufferPointer { buffer in
                input.name_utf8 = buffer.baseAddress
                input.name_bytes = UInt64(buffer.count)
                return CoreStatus(cValue: inkpod_core_tree_edit(
                    entry.core,
                    &input,
                    &result,
                    &affectedObjectID
                ))
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let tree = treeProjection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if tree.session.documentRevision == before.documentRevision {
                return .noOp(tree.session)
            }
            return .treeUpdated(CoreTreeMutationProjection(
                tree: tree,
                affectedObjectID: affectedObjectID == 0 ? nil : affectedObjectID
            ))
        }
    }

    private func applySelection(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        samples: [CorePointerSample]
    ) -> CoreRequestOutcome {
        guard !samples.isEmpty, samples.count <= 1_048_576,
              samples.allSatisfy(\.isValid)
        else {
            return .failed(.invalidRequest)
        }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard validatePaintExpectation(expectation, entry: entry, view: view),
                  let editor = editorProjection(for: entry)
            else {
                return .failed(.staleTarget)
            }
            let options = editor.selectionOptions
            let validCount = switch options.shape {
            case .rectangle, .ellipse: samples.count == 2
            case .wand: samples.count == 1
            case .lasso, .polyline: samples.count >= 3
            case .trace: true
            }
            guard validCount else { return .failed(.invalidRequest) }
            let resolved = resolveDocumentPoints(
                core: entry.core,
                view: view,
                expectedViewRevision: expectation.viewRevision,
                samples: samples
            )
            guard resolved.0 == .ok, resolved.1.count == samples.count else {
                return .failed(resolved.0 == .invalidArgument
                    ? .invalidRequest : .coreOperation(resolved.0))
            }
            let points = selectionPoints(resolved.1, source: samples)
            var input = InkpodSelectionInput()
            input.struct_size = UInt32(MemoryLayout<InkpodSelectionInput>.size)
            input.shape = options.shape.rawValue
            input.operation = options.operation.rawValue
            input.diameter = Float(options.diameter)
            input.tolerance = options.tolerance
            input.gap_close = options.gapClose
            input.interpretation = options.interpretation.rawValue
            input.aspect_ratio_q16 = UInt32(
                min(Double(UInt32.max), (options.aspectRatio * 65_536).rounded())
            )
            input.construction_flags = inkpod_bridge_selection_construction_flags(
                options.fromCenter ? 1 : 0,
                options.constrainRotationTo45Degrees ? 1 : 0,
                options.pressureControlsSize ? 1 : 0,
                options.screenSizedTrace ? 1 : 0
            )
            input.rotation_turns = options.rotationTurns
            input.trace_shape = options.traceShape.rawValue
            input.view_zoom_q16 = Int64(
                ((viewZoom(core: entry.core, view: view) ?? 1) * 65_536).rounded()
            )
            if options.shape == .wand {
                guard let point = resolved.1.first,
                      let x = documentPixel(point.x, upperBound: editor.session.documentWidth),
                      let y = documentPixel(point.y, upperBound: editor.session.documentHeight)
                else {
                    return .failed(.invalidRequest)
                }
                input.seed_x = x
                input.seed_y = y
            }
            var result = dispatchResult()
            let status = points.withUnsafeBufferPointer { buffer in
                if options.shape != .wand {
                    input.points = buffer.baseAddress
                    input.point_count = UInt64(buffer.count)
                    input.point_stride_bytes = UInt64(MemoryLayout<InkpodSelectionPoint>.stride)
                }
                return CoreStatus(cValue: inkpod_core_apply_selection_for_editor_target(
                    entry.core,
                    expectation.layerID,
                    expectation.planeID,
                    &input,
                    &result
                ))
            }
            return documentMutationOutcome(
                status: status,
                before: expectation.documentRevision,
                entry: entry
            )
        }
    }

    private func selectionAdjust(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        operation: CoreSelectionAdjustOperation,
        pixels: UInt32
    ) -> CoreRequestOutcome {
        guard operation == .invert ? pixels == 0 : (1 ... 4_096).contains(pixels) else {
            return .failed(.invalidRequest)
        }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var result = dispatchResult()
            let status = CoreStatus(cValue: inkpod_core_selection_adjust(
                entry.core,
                operation.rawValue,
                pixels,
                &result
            ))
            return documentMutationOutcome(
                status: status,
                before: expectedDocumentRevision,
                entry: entry
            )
        }
    }

    private func clearSelection(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreRequestOutcome {
        withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var result = dispatchResult()
            let status = CoreStatus(cValue: inkpod_core_selection_clear(entry.core, &result))
            return documentMutationOutcome(
                status: status,
                before: expectedDocumentRevision,
                entry: entry
            )
        }
    }

    private func selectColor(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        different: Bool,
        operation: CoreSelectionOperation
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard validatePaintExpectation(expectation, entry: entry, view: view),
                  let editor = editorProjection(for: entry)
            else {
                return .failed(.staleTarget)
            }
            var color = ffiColor(editor.currentColor)
            var result = dispatchResult()
            let status = CoreStatus(cValue: inkpod_core_select_color_for_editor_target(
                entry.core,
                expectation.layerID,
                expectation.planeID,
                &color,
                editor.selectionOptions.tolerance,
                different ? 1 : 0,
                operation.rawValue,
                &result
            ))
            return documentMutationOutcome(
                status: status,
                before: expectation.documentRevision,
                entry: entry
            )
        }
    }

    private func selectionToLayer(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        nameUTF8: [UInt8]
    ) -> CoreRequestOutcome {
        guard !nameUTF8.isEmpty, nameUTF8.count <= 4_096,
              !nameUTF8.contains(0), String(bytes: nameUTF8, encoding: .utf8) != nil
        else {
            return .failed(.invalidRequest)
        }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var result = dispatchResult()
            var layerID: UInt64 = 0
            let status = nameUTF8.withUnsafeBufferPointer { buffer in
                CoreStatus(cValue: inkpod_core_selection_to_layer(
                    entry.core,
                    buffer.baseAddress,
                    UInt64(buffer.count),
                    &result,
                    &layerID
                ))
            }
            return documentMutationOutcome(
                status: status,
                before: expectedDocumentRevision,
                entry: entry
            )
        }
    }

    private func selectionFromLayer(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        layerID: UInt64,
        operation: CoreSelectionLayerOperation
    ) -> CoreRequestOutcome {
        guard layerID != 0 else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var result = dispatchResult()
            let status = CoreStatus(cValue: inkpod_core_selection_from_layer(
                entry.core,
                layerID,
                operation.rawValue,
                &result
            ))
            return documentMutationOutcome(
                status: status,
                before: expectedDocumentRevision,
                entry: entry
            )
        }
    }

    private func undo(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64?
    ) -> CoreRequestOutcome {
        moveHistory(
            target: target,
            expectedDocumentRevision: expectedDocumentRevision,
            redo: false
        )
    }

    private func redo(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64?
    ) -> CoreRequestOutcome {
        moveHistory(
            target: target,
            expectedDocumentRevision: expectedDocumentRevision,
            redo: true
        )
    }

    private func moveHistory(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64?,
        redo: Bool
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if let expectedDocumentRevision,
               expectedDocumentRevision != before.documentRevision
            {
                return .failed(.staleTarget)
            }
            var result = InkpodDispatchResult()
            result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
            let status = CoreStatus(cValue: redo
                ? inkpod_core_redo(entry.core, &result)
                : inkpod_core_undo(entry.core, &result))
            guard status == .ok else {
                return .failed(.coreOperation(status))
            }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if updated.documentRevision == before.documentRevision {
                return .noOp(updated)
            }
            return .documentUpdated(updated)
        }
    }

    private func inspectHistory(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64?
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let session = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if let expectedDocumentRevision,
               expectedDocumentRevision != session.documentRevision
            {
                return .failed(.staleTarget)
            }
            var info = InkpodHistoryInfo()
            info.struct_size = UInt32(MemoryLayout<InkpodHistoryInfo>.size)
            let infoStatus = CoreStatus(cValue: inkpod_core_history_info(entry.core, &info))
            guard infoStatus == .ok, info.item_count <= 1_048_576,
                  info.cursor <= info.item_count
            else {
                return .failed(.coreOperation(infoStatus == .ok ? .panic : infoStatus))
            }
            let maximumMenuItems: UInt64 = 64
            let halfWindow = maximumMenuItems / 2
            let latestStart = info.item_count > maximumMenuItems
                ? info.item_count - maximumMenuItems : 0
            let cursorStart = info.cursor > halfWindow ? info.cursor - halfWindow : 0
            let firstIndex = min(cursorStart, latestStart)
            let endIndex = min(info.item_count, firstIndex + maximumMenuItems)
            var items: [CoreHistoryItemProjection] = []
            items.reserveCapacity(Int(endIndex - firstIndex))
            for index in firstIndex ..< endIndex {
                var item = InkpodHistoryItem()
                item.struct_size = UInt32(MemoryLayout<InkpodHistoryItem>.size)
                let status = CoreStatus(cValue: inkpod_core_history_item(
                    entry.core,
                    index,
                    &item
                ))
                guard status == .ok,
                      let kind = CoreHistoryEntryKind(rawValue: item.entry_kind),
                      item.index == index
                else {
                    return .failed(.coreOperation(status == .ok ? .panic : status))
                }
                items.append(CoreHistoryItemProjection(
                    index: index,
                    kind: kind,
                    isApplied: item.flags & inkpod_bridge_history_item_applied() != 0
                ))
            }
            return .history(CoreHistoryProjection(
                session: session,
                cursor: info.cursor,
                items: items
            ))
        }
    }

    private func jumpHistory(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        cursor: UInt64
    ) -> CoreRequestOutcome {
        withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var result = dispatchResult()
            let status = CoreStatus(cValue: inkpod_core_history_jump(
                entry.core,
                cursor,
                &result
            ))
            return documentMutationOutcome(
                status: status,
                before: expectedDocumentRevision,
                entry: entry
            )
        }
    }

    private func save(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8],
        allowCleanSave: Bool
    ) -> CoreRequestOutcome {
        guard validPath(pathUTF8) else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            guard allowCleanSave || before.isDirty else { return .noOp(before) }
            var info = documentInfo()
            let status = withPath(pathUTF8) { pointer, count in
                CoreStatus(cValue: inkpod_core_save(entry.core, pointer, count, &info))
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            return .fileCompleted(CoreFileProjection(operation: .save, session: updated))
        }
    }

    private func autosave(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreRequestOutcome {
        guard validPath(pathUTF8) else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var info = documentInfo()
            let status = withPath(pathUTF8) { pointer, count in
                CoreStatus(cValue: inkpod_core_autosave(entry.core, pointer, count, &info))
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            return .fileCompleted(CoreFileProjection(operation: .autosave, session: updated))
        }
    }

    private func openFile(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8],
        recovery: Bool
    ) -> CoreRequestOutcome {
        guard validPath(pathUTF8) else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            switch createBareCore() {
            case let .failure(status):
                return .failed(.coreCreate(status))
            case let .success(staged):
                var stagedOwner: OpaquePointer? = staged
                var info = documentInfo()
                let status = withPath(pathUTF8) { pointer, count in
                    CoreStatus(cValue: recovery
                        ? inkpod_core_open_recovery(staged, pointer, count, &info)
                        : inkpod_core_open(staged, pointer, count, &info))
                }
                guard status == .ok else {
                    _ = inkpod_core_destroy(&stagedOwner)
                    return .failed(.coreOperation(status))
                }
                return installStagedCore(
                    stagedOwner: &stagedOwner,
                    replacing: entry,
                    documentUUID: CoreDocumentUUID(
                        high: info.document_uuid_high,
                        low: info.document_uuid_low
                    ),
                    operation: recovery ? .openRecovery : .open
                )
            }
        }
    }

    private func revert(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        partial: Bool
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            let status: CoreStatus
            if partial {
                var result = InkpodDispatchResult()
                result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
                status = CoreStatus(
                    cValue: inkpod_core_revert_active_selection(entry.core, &result)
                )
            } else {
                var info = documentInfo()
                status = CoreStatus(cValue: inkpod_core_revert(entry.core, &info))
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if partial, updated.documentRevision == before.documentRevision {
                return .noOp(updated)
            }
            return .fileCompleted(
                CoreFileProjection(
                    operation: partial ? .revertPartial : .revert,
                    session: updated
                )
            )
        }
    }

    private func importCommonRaster(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        format: CoreCommonRasterFormat,
        bytes: [UInt8],
        documentUUID: CoreDocumentUUID
    ) -> CoreRequestOutcome {
        guard !bytes.isEmpty, documentUUID.isValid else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            if let duplicate = sessionByDocumentUUID[documentUUID], duplicate != target.id {
                return .failed(.staleTarget)
            }
            switch createBareCore() {
            case let .failure(status):
                return .failed(.coreCreate(status))
            case let .success(staged):
                var stagedOwner: OpaquePointer? = staged
                var info = documentInfo()
                let status = bytes.withUnsafeBytes { rawBytes in
                    CoreStatus(cValue: inkpod_core_import_common_raster(
                        staged,
                        ffiRasterFormat(format),
                        rawBytes.bindMemory(to: UInt8.self).baseAddress,
                        UInt64(rawBytes.count),
                        documentUUID.high,
                        documentUUID.low,
                        &info
                    ))
                }
                guard status == .ok else {
                    _ = inkpod_core_destroy(&stagedOwner)
                    return .failed(.coreOperation(status))
                }
                return installStagedCore(
                    stagedOwner: &stagedOwner,
                    replacing: entry,
                    documentUUID: documentUUID,
                    operation: .importRaster
                )
            }
        }
    }

    private func exportCommonRaster(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        format: CoreCommonRasterFormat,
        compositeWhite: Bool
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var buffer: OpaquePointer?
            let status = CoreStatus(cValue: inkpod_core_export_common_raster(
                entry.core,
                ffiRasterFormat(format),
                compositeWhite ? 1 : 0,
                &buffer
            ))
            guard status == .ok, let liveBuffer = buffer else {
                if buffer != nil { _ = inkpod_byte_buffer_release(&buffer) }
                return .failed(.coreOperation(status == .ok ? .panic : status))
            }
            var pointer: UnsafePointer<UInt8>?
            var count: UInt64 = 0
            let viewStatus = CoreStatus(
                cValue: inkpod_byte_buffer_view(liveBuffer, &pointer, &count)
            )
            guard viewStatus == .ok,
                  count <= 512 * 1_024 * 1_024,
                  count <= UInt64(Int.max),
                  let pointer
            else {
                _ = inkpod_byte_buffer_release(&buffer)
                return .failed(.coreOperation(viewStatus == .ok ? .invalidArgument : viewStatus))
            }
            let bytes = Array(UnsafeBufferPointer(start: pointer, count: Int(count)))
            let releaseStatus = CoreStatus(cValue: inkpod_byte_buffer_release(&buffer))
            guard releaseStatus == .ok, buffer == nil else {
                return .failed(.coreOperation(releaseStatus))
            }
            return .rasterExported(CoreRasterExport(format: format, bytes: bytes))
        }
    }

    private func compactionPlan(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var plan = InkpodCompactionPlan()
            plan.struct_size = UInt32(MemoryLayout<InkpodCompactionPlan>.size)
            let status = CoreStatus(cValue: inkpod_core_compaction_plan(entry.core, &plan))
            guard status == .ok else { return .failed(.coreOperation(status)) }
            return .compactionPlanned(
                CoreCompactionToken(
                    historyEventCount: plan.history_event_count,
                    historyProcedureCount: plan.history_procedure_count,
                    documentDigest: bytes(of: plan.document_digest),
                    editorDigest: bytes(of: plan.editor_digest),
                    journalDigest: bytes(of: plan.journal_digest)
                )
            )
        }
    }

    private func writeCompactedCopy(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8],
        token: CoreCompactionToken
    ) -> CoreRequestOutcome {
        guard validPath(pathUTF8), token.isValid else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var plan = InkpodCompactionPlan()
            plan.struct_size = UInt32(MemoryLayout<InkpodCompactionPlan>.size)
            plan.history_event_count = token.historyEventCount
            plan.history_procedure_count = token.historyProcedureCount
            copy(token.documentDigest, to: &plan.document_digest)
            copy(token.editorDigest, to: &plan.editor_digest)
            copy(token.journalDigest, to: &plan.journal_digest)
            let status = withPath(pathUTF8) { pointer, count in
                CoreStatus(cValue: inkpod_core_write_compacted_copy(
                    entry.core,
                    pointer,
                    count,
                    &plan
                ))
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            return .fileCompleted(
                CoreFileProjection(operation: .compactedCopy, session: updated)
            )
        }
    }

    private func copyClipboard(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        cut: Bool
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var clipboard: OpaquePointer?
            let copyStatus = CoreStatus(
                cValue: inkpod_core_clipboard_copy(entry.core, &clipboard)
            )
            guard copyStatus == .ok, let liveClipboard = clipboard else {
                if clipboard != nil { _ = inkpod_clipboard_release(&clipboard) }
                return .failed(.coreOperation(copyStatus == .ok ? .panic : copyStatus))
            }
            guard let raster = renderClipboard(liveClipboard) else {
                _ = inkpod_clipboard_release(&clipboard)
                return .failed(.coreOperation(.panic))
            }
            if cut {
                var result = InkpodDispatchResult()
                result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
                let cutStatus = CoreStatus(
                    cValue: inkpod_core_clear_selected_content(entry.core, &result)
                )
                guard cutStatus == .ok else {
                    _ = inkpod_clipboard_release(&clipboard)
                    return .failed(.coreOperation(cutStatus))
                }
            }
            guard let id = installClipboard(&clipboard) else {
                _ = inkpod_clipboard_release(&clipboard)
                return .failed(.identityOverflow)
            }
            let updated = cut ? projection(for: entry) : before
            guard let updated else {
                _ = releaseClipboard(id)
                return .failed(.coreOperation(.panic))
            }
            return .clipboardCopied(
                CoreClipboardProjection(id: id, raster: raster, session: updated)
            )
        }
    }

    private func createClipboard(from raster: CoreClipboardRaster) -> CoreRequestOutcome {
        guard raster.isValid else { return .failed(.invalidRequest) }
        var clipboard: OpaquePointer?
        var input = InkpodClipboardRgbaInput()
        input.struct_size = UInt32(MemoryLayout<InkpodClipboardRgbaInput>.size)
        input.origin_x = raster.originX
        input.origin_y = raster.originY
        input.width = raster.width
        input.height = raster.height
        input.pixel_bytes = UInt64(raster.rgba8.count)
        input.row_stride_bytes = raster.rowStrideBytes
        let status = raster.rgba8.withUnsafeBytes { rawBytes in
            input.pixels_rgba8 = rawBytes.bindMemory(to: UInt8.self).baseAddress
            return CoreStatus(cValue: inkpod_clipboard_create_rgba8(&input, &clipboard))
        }
        guard status == .ok, clipboard != nil else {
            if clipboard != nil { _ = inkpod_clipboard_release(&clipboard) }
            return .failed(.coreOperation(status == .ok ? .panic : status))
        }
        guard let id = installClipboard(&clipboard) else {
            _ = inkpod_clipboard_release(&clipboard)
            return .failed(.identityOverflow)
        }
        return .clipboardCopied(
            CoreClipboardProjection(id: id, raster: raster, session: nil)
        )
    }

    private func releaseClipboard(_ id: CoreClipboardID) -> CoreRequestOutcome {
        guard id.rawValue != 0 else { return .failed(.invalidRequest) }
        guard let ownedClipboard = clipboards.removeValue(forKey: id) else {
            return .noOp(nil)
        }
        var clipboard: OpaquePointer? = ownedClipboard
        let status = CoreStatus(cValue: inkpod_clipboard_release(&clipboard))
        guard status == .ok, clipboard == nil else {
            return .failed(.coreOperation(status))
        }
        return .acknowledged
    }

    private func beginPaste(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        clipboard id: CoreClipboardID,
        mode: CorePasteMode
    ) -> CoreRequestOutcome {
        guard id.rawValue != 0, mode.isValid else { return .failed(.invalidRequest) }
        guard let clipboard = clipboards[id] else { return .failed(.staleTarget) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case var .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            guard entry.activeTransient == nil else { return .noOp(before) }
            let status: CoreStatus
            switch mode {
            case .compatible:
                status = CoreStatus(cValue: inkpod_core_paste_begin_mode(
                    entry.core,
                    clipboard,
                    inkpod_bridge_paste_compatible()
                ))
            case .activePlaneConverted:
                status = CoreStatus(cValue: inkpod_core_paste_begin_mode(
                    entry.core,
                    clipboard,
                    inkpod_bridge_paste_active_converted()
                ))
            case let .newRasterPlane(newPlane):
                var info = documentInfo()
                let infoStatus = CoreStatus(
                    cValue: inkpod_core_get_document_info(entry.core, &info)
                )
                guard infoStatus == .ok else { return .failed(.coreOperation(infoStatus)) }
                let name = Array(newPlane.name.utf8)
                var edit = InkpodTreeEdit()
                edit.struct_size = UInt32(MemoryLayout<InkpodTreeEdit>.size)
                edit.operation = inkpod_bridge_tree_create_plane()
                edit.flags = UInt64(inkpod_bridge_node_visible_editable())
                edit.parent_id = info.layer_id
                edit.kind = inkpod_bridge_typed_plane_raster()
                edit.pixel_format = inkpod_bridge_storage_rgba8()
                edit.opacity_milli = newPlane.opacityMilli
                status = name.withUnsafeBytes { rawName in
                    edit.name_utf8 = rawName.bindMemory(to: UInt8.self).baseAddress
                    edit.name_bytes = UInt64(rawName.count)
                    return CoreStatus(cValue: inkpod_core_paste_begin_new_plane(
                        entry.core,
                        clipboard,
                        &edit
                    ))
                }
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            entry.activeTransient = .floatingPaste
            sessions[target.id] = entry
            guard let updated = projection(for: entry) else {
                _ = inkpod_core_floating_cancel(entry.core)
                entry.activeTransient = nil
                sessions[target.id] = entry
                return .failed(.coreOperation(.panic))
            }
            return .pasteStarted(updated)
        }
    }

    private func finishPaste(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        commit: Bool
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case var .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            guard entry.activeTransient == .floatingPaste else {
                return commit ? .failed(.coreOperation(.invalidState)) : .noOp(before)
            }
            let status: CoreStatus
            if commit {
                var result = InkpodDispatchResult()
                result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
                status = CoreStatus(cValue: inkpod_core_floating_commit(entry.core, &result))
            } else {
                status = CoreStatus(cValue: inkpod_core_floating_cancel(entry.core))
            }
            guard status == .ok else { return .failed(.coreOperation(status)) }
            entry.activeTransient = nil
            sessions[target.id] = entry
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if commit {
                return updated.documentRevision == before.documentRevision
                    ? .noOp(updated)
                    : .documentUpdated(updated)
            }
            return .pasteCancelled(updated)
        }
    }

    private func transformFloatingPaste(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        transform: CoreFloatingTransform
    ) -> CoreRequestOutcome {
        guard transform.isValid else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            guard entry.activeTransient == .floatingPaste else {
                return .failed(.coreOperation(.invalidState))
            }
            var input = InkpodFloatingTransform()
            input.struct_size = UInt32(MemoryLayout<InkpodFloatingTransform>.size)
            input.anchor = transform.anchor.rawValue
            input.target_x = transform.targetX
            input.target_y = transform.targetY
            input.scale_x = transform.scaleX
            input.scale_y = transform.scaleY
            input.rotation_degrees = transform.rotationDegrees
            let status = CoreStatus(cValue: inkpod_core_floating_transform(
                entry.core,
                &input
            ))
            guard status == .ok, let updated = projection(for: entry) else {
                return .failed(.coreOperation(status == .ok ? .panic : status))
            }
            return .floatingTransformed(updated)
        }
    }

    private func beginHistoryVisualization(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreRequestOutcome {
        withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var task: OpaquePointer?
            let taskStatus = CoreStatus(cValue: inkpod_task_create(&task))
            guard taskStatus == .ok, let taskHandle = task else {
                if task != nil { _ = inkpod_task_release(&task) }
                return .failed(.coreOperation(taskStatus == .ok ? .panic : taskStatus))
            }
            var builder: OpaquePointer?
            let beginStatus = CoreStatus(cValue: inkpod_core_history_visualization_builder_begin(
                entry.core,
                taskHandle,
                &builder
            ))
            guard beginStatus == .ok, builder != nil else {
                if builder != nil {
                    _ = inkpod_history_visualization_builder_release(&builder, taskHandle)
                }
                _ = inkpod_task_release(&task)
                return .failed(.coreOperation(beginStatus == .ok ? .panic : beginStatus))
            }
            guard nextHistoryVisualizationID != 0,
                  nextHistoryVisualizationID < UInt64.max
            else {
                _ = inkpod_history_visualization_builder_release(&builder, taskHandle)
                _ = inkpod_task_release(&task)
                return .failed(.identityOverflow)
            }
            let id = CoreHistoryVisualizationID(rawValue: nextHistoryVisualizationID)
            nextHistoryVisualizationID += 1
            let progress = CoreHistoryVisualizationProgressProjection(
                id: id,
                completedEvents: 0,
                totalEvents: 0,
                completedRows: 0,
                rowCount: 0,
                isComplete: false
            )
            historyVisualizations[id] = CoreHistoryVisualizationEntry(
                session: target,
                task: task,
                builder: builder,
                visualization: nil,
                progress: progress
            )
            return .historyVisualizationProgress(progress)
        }
    }

    private func stepHistoryVisualization(
        _ id: CoreHistoryVisualizationID,
        maximumEvents: UInt32
    ) -> CoreRequestOutcome {
        guard id.rawValue != 0, (1 ... 4_096).contains(maximumEvents) else {
            return .failed(.invalidRequest)
        }
        guard var entry = historyVisualizations[id] else {
            return .failed(.staleTarget)
        }
        if entry.progress.isComplete {
            return .historyVisualizationProgress(entry.progress)
        }
        guard let builder = entry.builder, let task = entry.task,
              entry.visualization == nil
        else {
            _ = releaseHistoryVisualizationEntry(id)
            return .failed(.coreOperation(.panic))
        }
        var ffiProgress = InkpodHistoryVisualizationProgress()
        ffiProgress.struct_size = UInt32(
            MemoryLayout<InkpodHistoryVisualizationProgress>.size
        )
        var visualization: OpaquePointer?
        let status = CoreStatus(cValue: inkpod_history_visualization_builder_step(
            builder,
            task,
            maximumEvents,
            &ffiProgress,
            &visualization
        ))
        guard status == .ok else {
            if visualization != nil { _ = inkpod_history_visualization_release(&visualization) }
            _ = releaseHistoryVisualizationEntry(id)
            return .failed(.coreOperation(status))
        }
        guard ffiProgress.completed_events <= ffiProgress.total_events,
              ffiProgress.completed_rows <= ffiProgress.total_rows
        else {
            if visualization != nil { _ = inkpod_history_visualization_release(&visualization) }
            _ = releaseHistoryVisualizationEntry(id)
            return .failed(.coreOperation(.panic))
        }
        let done = ffiProgress.done != 0
        if done {
            guard visualization != nil else {
                _ = releaseHistoryVisualizationEntry(id)
                return .failed(.coreOperation(.panic))
            }
            var builderOwner = entry.builder
            let builderStatus = CoreStatus(cValue:
                inkpod_history_visualization_builder_release(&builderOwner, task)
            )
            entry.builder = builderOwner
            var taskOwner = entry.task
            let taskStatus = CoreStatus(cValue: inkpod_task_release(&taskOwner))
            entry.task = taskOwner
            guard builderStatus == .ok, taskStatus == .ok,
                  entry.builder == nil, entry.task == nil
            else {
                _ = inkpod_history_visualization_release(&visualization)
                historyVisualizations[id] = entry
                _ = releaseHistoryVisualizationEntry(id)
                return .failed(.coreOperation(
                    builderStatus != .ok ? builderStatus : taskStatus
                ))
            }
            entry.visualization = visualization
        } else if visualization != nil {
            _ = inkpod_history_visualization_release(&visualization)
            _ = releaseHistoryVisualizationEntry(id)
            return .failed(.coreOperation(.panic))
        }
        entry.progress = CoreHistoryVisualizationProgressProjection(
            id: id,
            completedEvents: ffiProgress.completed_events,
            totalEvents: ffiProgress.total_events,
            completedRows: ffiProgress.completed_rows,
            rowCount: ffiProgress.total_rows,
            isComplete: done
        )
        historyVisualizations[id] = entry
        return .historyVisualizationProgress(entry.progress)
    }

    private func historyVisualizationRows(
        _ id: CoreHistoryVisualizationID,
        range: Range<UInt64>
    ) -> CoreRequestOutcome {
        guard id.rawValue != 0, range.lowerBound <= range.upperBound,
              range.count <= 256
        else {
            return .failed(.invalidRequest)
        }
        guard let entry = historyVisualizations[id] else {
            return .failed(.staleTarget)
        }
        guard entry.progress.isComplete, let visualization = entry.visualization,
              range.upperBound <= entry.progress.rowCount
        else {
            return .failed(.invalidRequest)
        }
        var rows: [CoreHistoryVisualizationRow] = []
        rows.reserveCapacity(range.count)
        for index in range {
            guard let row = historyVisualizationRow(visualization, index: index) else {
                return .failed(.coreOperation(.panic))
            }
            rows.append(row)
        }
        return .historyVisualizationRows(rows)
    }

    private func historyVisualizationRow(
        _ visualization: OpaquePointer,
        index: UInt64
    ) -> CoreHistoryVisualizationRow? {
        var output = InkpodHistoryVisualizationRowBuffer()
        output.struct_size = UInt32(MemoryLayout<InkpodHistoryVisualizationRowBuffer>.size)
        let queryStatus = CoreStatus(cValue: inkpod_history_visualization_row_get(
            visualization,
            index,
            &output
        ))
        guard queryStatus == .ok || queryStatus == .bufferTooSmall,
              output.primitive_name_bytes <= 4_096,
              output.arguments_bytes <= 32_768,
              output.thumbnail_bytes <= 64 * 64 * 4,
              output.primitive_name_bytes <= UInt64(Int.max),
              output.arguments_bytes <= UInt64(Int.max),
              output.thumbnail_bytes <= UInt64(Int.max)
        else {
            return nil
        }
        var name = [UInt8](repeating: 0, count: Int(output.primitive_name_bytes))
        var arguments = [UInt8](repeating: 0, count: Int(output.arguments_bytes))
        var thumbnail = [UInt8](repeating: 0, count: Int(output.thumbnail_bytes))
        let copyStatus = name.withUnsafeMutableBufferPointer { nameBuffer in
            arguments.withUnsafeMutableBufferPointer { argumentBuffer in
                thumbnail.withUnsafeMutableBufferPointer { thumbnailBuffer in
                    output.primitive_name_utf8 = nameBuffer.isEmpty
                        ? nil : nameBuffer.baseAddress
                    output.primitive_name_capacity = UInt64(nameBuffer.count)
                    output.arguments_utf8 = argumentBuffer.isEmpty
                        ? nil : argumentBuffer.baseAddress
                    output.arguments_capacity = UInt64(argumentBuffer.count)
                    output.thumbnail_rgba8 = thumbnailBuffer.isEmpty
                        ? nil : thumbnailBuffer.baseAddress
                    output.thumbnail_capacity = UInt64(thumbnailBuffer.count)
                    return CoreStatus(cValue: inkpod_history_visualization_row_get(
                        visualization,
                        index,
                        &output
                    ))
                }
            }
        }
        guard copyStatus == .ok,
              let primitiveName = String(bytes: name, encoding: .utf8),
              let argumentText = String(bytes: arguments, encoding: .utf8),
              output.thumbnail_width <= 64, output.thumbnail_height <= 64,
              UInt64(output.thumbnail_stride_bytes) * UInt64(output.thumbnail_height)
                == output.thumbnail_bytes
        else {
            return nil
        }
        return CoreHistoryVisualizationRow(
            journalEventID: output.journal_event_id,
            procedureID: output.procedure_id,
            committedStateID: output.committed_state_id,
            branchID: output.branch_id,
            primitiveID: output.primitive_id,
            primitiveName: primitiveName,
            arguments: argumentText,
            thumbnailWidth: output.thumbnail_width,
            thumbnailHeight: output.thumbnail_height,
            thumbnailStrideBytes: output.thumbnail_stride_bytes,
            thumbnailChecksum: output.thumbnail_checksum,
            thumbnailRGBA8: thumbnail
        )
    }

    private func releaseHistoryVisualization(
        _ id: CoreHistoryVisualizationID
    ) -> CoreRequestOutcome {
        guard id.rawValue != 0 else { return .failed(.invalidRequest) }
        guard historyVisualizations[id] != nil else { return .noOp(nil) }
        return releaseHistoryVisualizationEntry(id)
            ? .acknowledged : .failed(.coreOperation(.panic))
    }

    private func buildSnapshot(route: CoreSnapshotRoute) -> CoreRequestOutcome {
        guard route.session == route.view.session,
              route.surface.id.rawValue != 0,
              route.surface.generation.rawValue != 0
        else {
            return .failed(.invalidRequest)
        }
        switch resolve(route.view) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry, view):
            guard let projection = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            var options = InkpodSnapshotOptions()
            options.struct_size = UInt32(MemoryLayout<InkpodSnapshotOptions>.size)
            options.feature_flags = inkpod_bridge_feature_none()
            var rawSnapshot: OpaquePointer?
            let status: CoreStatus
            if view.coreViewID == 0 {
                status = CoreStatus(
                    cValue: inkpod_core_build_snapshot(entry.core, &options, &rawSnapshot)
                )
            } else {
                status = CoreStatus(cValue: inkpod_core_build_snapshot_for_view(
                    entry.core,
                    view.coreViewID,
                    &options,
                    &rawSnapshot
                ))
            }
            guard status == .ok, let rawSnapshot else {
                if rawSnapshot != nil {
                    _ = inkpod_snapshot_release(&rawSnapshot)
                }
                return .failed(.coreOperation(status == .ok ? .panic : status))
            }
            let owner = CoreOwnedSnapshot(raw: rawSnapshot)
            do {
                let viewRevision = try owner.withBorrowedRenderView { $0.transform.viewRevision }
                return .snapshot(
                    CoreSnapshotEnvelope(
                        route: route,
                        documentRevision: projection.documentRevision,
                        viewRevision: viewRevision,
                        owner: owner
                    )
                )
            } catch {
                owner.release()
                return .failed(.coreOperation(.panic))
            }
        }
    }

    private func inspectSession(target: CoreSessionTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case let .live(entry):
            guard let projection = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            return .inspected(projection)
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        }
    }

    private func closeSession(target: CoreSessionTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired:
            return .noOp(nil)
        case .invalid:
            return .failed(.invalidTarget)
        case .stale:
            return .failed(.staleTarget)
        case var .live(entry):
            let cancelledTransient = entry.activeTransient != nil
            if let transient = entry.activeTransient {
                let cancelStatus = cancelTransient(transient, core: entry.core)
                guard cancelStatus == .ok else {
                    return .failed(.coreOperation(cancelStatus))
                }
                entry.activeTransient = nil
                sessions[target.id] = entry
            }
            guard releaseColorChartPreviews(for: target) else {
                return .failed(.coreOperation(.panic))
            }
            guard releaseHistoryVisualizations(for: target) else {
                return .failed(.coreOperation(.panic))
            }
            var core: OpaquePointer? = entry.core
            let destroyStatus = CoreStatus(cValue: inkpod_core_destroy(&core))
            guard destroyStatus == .ok, core == nil else {
                return .failed(.coreOperation(destroyStatus))
            }
            sessions.removeValue(forKey: target.id)
            sessionByDocumentUUID.removeValue(forKey: entry.documentUUID)
            retiredGenerations[target.id] = target.generation
            return .closed(
                CoreSessionCloseProjection(
                    target: target,
                    ownerThreadID: ownerThreadID,
                    cancelledActiveTransient: cancelledTransient
                )
            )
        }
    }

    private func createCut(
        cutUUID: CoreCutUUID,
        metadata: CoreCutMetadata,
        defaults: CoreCutDefaults,
        members: [CoreCutMember]
    ) -> CoreRequestOutcome {
        guard cutUUID.isValid, metadata.isValid, defaults.isValid,
              members.count <= 10_000, members.allSatisfy(\.isValid),
              nextCutID != 0, nextCutGeneration != 0,
              nextCutID < UInt64.max, nextCutGeneration < UInt64.max
        else {
            return .failed(.invalidRequest)
        }
        var rawCut: OpaquePointer?
        let status = withCutMetadata(metadata) { metadataInput in
            var metadataInput = metadataInput
            var defaultsInput = ffiCutDefaults(defaults)
            return withUnsafePointer(to: &metadataInput) { metadataPointer in
                withUnsafePointer(to: &defaultsInput) { defaultsPointer in
                    withCutMembers(members) { memberPointer, memberCount in
                        var request = InkpodCutCreateRequest()
                        request.struct_size = UInt32(MemoryLayout<InkpodCutCreateRequest>.size)
                        request.cut_uuid_high = cutUUID.high
                        request.cut_uuid_low = cutUUID.low
                        request.metadata = metadataPointer
                        request.defaults = defaultsPointer
                        request.members = memberPointer
                        request.member_count = memberCount
                        request.member_stride_bytes = UInt64(
                            MemoryLayout<InkpodCutMemberInput>.stride
                        )
                        return CoreStatus(cValue: inkpod_cut_create(&request, &rawCut))
                    }
                }
            }
        }
        guard status == .ok, let rawCut else {
            if rawCut != nil { _ = inkpod_cut_destroy(&rawCut) }
            return .failed(.coreOperation(status == .ok ? .panic : status))
        }
        let target = CoreCutTarget(
            id: CoreCutID(rawValue: nextCutID),
            generation: CoreCutGeneration(rawValue: nextCutGeneration)
        )
        nextCutID += 1
        nextCutGeneration += 1
        let entry = CoreCutEntry(cut: rawCut, target: target)
        cuts[target.id] = entry
        guard let projection = cutProjection(entry) else {
            cuts.removeValue(forKey: target.id)
            var owned: OpaquePointer? = rawCut
            _ = inkpod_cut_destroy(&owned)
            return .failed(.coreOperation(.panic))
        }
        return .cut(projection)
    }

    private func openCut(pathUTF8: [UInt8], recovery: Bool) -> CoreRequestOutcome {
        guard validPath(pathUTF8), cuts.count < 64,
              nextCutID != 0, nextCutGeneration != 0,
              nextCutID < UInt64.max, nextCutGeneration < UInt64.max
        else {
            return .failed(.invalidRequest)
        }
        var rawCut: OpaquePointer?
        let status = withPath(pathUTF8) { pointer, count in
            CoreStatus(cValue: recovery
                ? inkpod_cut_open_recovery(pointer, count, &rawCut)
                : inkpod_cut_open(pointer, count, &rawCut))
        }
        guard status == .ok, let rawCut else {
            if rawCut != nil { _ = inkpod_cut_destroy(&rawCut) }
            return .failed(.coreOperation(status == .ok ? .panic : status))
        }
        let target = CoreCutTarget(
            id: CoreCutID(rawValue: nextCutID),
            generation: CoreCutGeneration(rawValue: nextCutGeneration)
        )
        nextCutID += 1
        nextCutGeneration += 1
        let entry = CoreCutEntry(cut: rawCut, target: target)
        cuts[target.id] = entry
        guard let projection = cutProjection(entry) else {
            cuts.removeValue(forKey: target.id)
            var owned: OpaquePointer? = rawCut
            _ = inkpod_cut_destroy(&owned)
            return .failed(.coreOperation(.panic))
        }
        return .cut(projection)
    }

    private func inspectCut(_ target: CoreCutTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case let .live(entry):
            return cutProjection(entry).map(CoreRequestOutcome.cut)
                ?? .failed(.coreOperation(.panic))
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        }
    }

    private func closeCut(_ target: CoreCutTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired:
            return .noOp(nil)
        case .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            var cut: OpaquePointer? = entry.cut
            let status = CoreStatus(cValue: inkpod_cut_destroy(&cut))
            guard status == .ok, cut == nil else { return .failed(.coreOperation(status)) }
            cuts.removeValue(forKey: target.id)
            retiredCutGenerations[target.id] = target.generation
            return .acknowledged
        }
    }

    private func updateCut(
        _ target: CoreCutTarget,
        expectedRevision: UInt64,
        metadata: CoreCutMetadata,
        defaults: CoreCutDefaults
    ) -> CoreRequestOutcome {
        guard metadata.isValid, defaults.isValid else { return .failed(.invalidRequest) }
        return withLiveCut(target, expectedRevision: expectedRevision) { entry in
            var result = dispatchResult()
            let status = withCutMetadata(metadata) { metadataInput in
                var metadataInput = metadataInput
                var defaultsInput = ffiCutDefaults(defaults)
                return withUnsafePointer(to: &metadataInput) { metadataPointer in
                    withUnsafePointer(to: &defaultsInput) { defaultsPointer in
                        var request = InkpodCutUpdateRequest()
                        request.struct_size = UInt32(MemoryLayout<InkpodCutUpdateRequest>.size)
                        request.base_revision = expectedRevision
                        request.metadata = metadataPointer
                        request.defaults = defaultsPointer
                        return CoreStatus(cValue: inkpod_cut_update(
                            entry.cut,
                            &request,
                            &result
                        ))
                    }
                }
            }
            return self.cutMutationOutcome(status, entry: entry, applied: result.accepted_command_count > 0)
        }
    }

    private func cancelCutUpdate(_ target: CoreCutTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case let .live(entry):
            var result = dispatchResult()
            let status = CoreStatus(cValue: inkpod_cut_cancel_update(entry.cut, &result))
            return cutMutationOutcome(status, entry: entry, applied: false)
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        }
    }

    private func editCutSequence(
        _ target: CoreCutTarget,
        expectedRevision: UInt64,
        operations: [CoreCutSequenceOperation]
    ) -> CoreRequestOutcome {
        guard !operations.isEmpty, operations.count <= 10_000,
              operations.allSatisfy(cutOperationIsValid)
        else {
            return .failed(.invalidRequest)
        }
        return withLiveCut(target, expectedRevision: expectedRevision) { entry in
            var result = InkpodCutSequenceEditResult()
            result.struct_size = UInt32(MemoryLayout<InkpodCutSequenceEditResult>.size)
            let status = withCutSequenceOperations(operations) { pointer, count in
                var request = InkpodCutSequenceEditRequest()
                request.struct_size = UInt32(MemoryLayout<InkpodCutSequenceEditRequest>.size)
                request.base_revision = expectedRevision
                request.operations = pointer
                request.operation_count = count
                request.operation_stride_bytes = UInt64(
                    MemoryLayout<InkpodCutSequenceEditOperation>.stride
                )
                return CoreStatus(cValue: inkpod_cut_sequence_edit(entry.cut, &request, &result))
            }
            guard status == .ok else {
                return .failed(.coreOperation(status))
            }
            guard let cut = self.cutProjection(entry) else {
                return .failed(.coreOperation(.panic))
            }
            return .cutMutation(CoreCutMutationProjection(
                cut: cut,
                applied: result.flags & 1 != 0,
                failedOperationIndex: result.failed_operation_index == UInt32.max
                    ? nil : result.failed_operation_index
            ))
        }
    }

    private func cancelCutSequence(_ target: CoreCutTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case let .live(entry):
            var result = InkpodCutSequenceEditResult()
            result.struct_size = UInt32(MemoryLayout<InkpodCutSequenceEditResult>.size)
            let status = CoreStatus(cValue: inkpod_cut_sequence_cancel(entry.cut, &result))
            guard status == .ok, let cut = cutProjection(entry) else {
                return .failed(.coreOperation(status == .ok ? .panic : status))
            }
            return .cutMutation(CoreCutMutationProjection(
                cut: cut,
                applied: false,
                failedOperationIndex: nil
            ))
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        }
    }

    private func cutHistory(
        _ target: CoreCutTarget,
        expectedRevision: UInt64,
        redo: Bool
    ) -> CoreRequestOutcome {
        withLiveCut(target, expectedRevision: expectedRevision) { entry in
            var result = dispatchResult()
            let status = CoreStatus(cValue: redo
                ? inkpod_cut_redo(entry.cut, &result)
                : inkpod_cut_undo(entry.cut, &result))
            return self.cutMutationOutcome(
                status,
                entry: entry,
                applied: result.accepted_command_count > 0
            )
        }
    }

    private func saveCut(
        _ target: CoreCutTarget,
        expectedRevision: UInt64,
        pathUTF8: [UInt8],
        recovery: Bool
    ) -> CoreRequestOutcome {
        guard validPath(pathUTF8) else { return .failed(.invalidRequest) }
        return withLiveCut(target, expectedRevision: expectedRevision) { entry in
            var info = InkpodCutInfo()
            info.struct_size = UInt32(MemoryLayout<InkpodCutInfo>.size)
            let status = withPath(pathUTF8) { pointer, count in
                CoreStatus(cValue: recovery
                    ? inkpod_cut_autosave(entry.cut, pointer, count, &info)
                    : inkpod_cut_save(entry.cut, pointer, count, &info))
            }
            guard status == .ok, let projection = self.cutProjection(entry) else {
                return .failed(.coreOperation(status == .ok ? .panic : status))
            }
            return .cut(projection)
        }
    }

    private func cutMutationOutcome(
        _ status: CoreStatus,
        entry: CoreCutEntry,
        applied: Bool
    ) -> CoreRequestOutcome {
        guard status == .ok, let projection = cutProjection(entry) else {
            return .failed(.coreOperation(status == .ok ? .panic : status))
        }
        return .cutMutation(CoreCutMutationProjection(
            cut: projection,
            applied: applied,
            failedOperationIndex: nil
        ))
    }

    private func beginTransientForTesting(target: CoreSessionTarget) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case var .live(entry):
            if entry.activeTransient != nil {
                return .noOp(projection(for: entry))
            }
            var sample = InkpodStrokeSample()
            sample.struct_size = UInt32(MemoryLayout<InkpodStrokeSample>.size)
            sample.x = 1
            sample.y = 1
            sample.pressure = 1
            var input = InkpodStrokeInput()
            input.struct_size = UInt32(MemoryLayout<InkpodStrokeInput>.size)
            input.tool = inkpod_bridge_tool_pencil()
            input.plane = inkpod_bridge_plane_color()
            input.coordinate_space = inkpod_bridge_coordinate_document()
            input.color_rgba = 0x1122_33FF
            input.diameter = 1
            input.sample_count = 1
            input.sample_stride_bytes = UInt64(MemoryLayout<InkpodStrokeSample>.stride)
            input.shape = inkpod_bridge_brush_round()
            input.start_color = inkpod_bridge_start_color_any()
            let status = withUnsafePointer(to: &sample) { samplePointer in
                input.samples = samplePointer
                return CoreStatus(cValue: inkpod_core_stroke_begin(entry.core, &input))
            }
            guard status == .ok else {
                return .failed(.coreOperation(status))
            }
            entry.activeTransient = .stroke
            sessions[target.id] = entry
            return .acknowledged
        }
    }

    private func selectAll(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard let before = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            guard before.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            guard before.documentWidth <= UInt32(Int32.max),
                  before.documentHeight <= UInt32(Int32.max)
            else {
                return .failed(.invalidRequest)
            }
            var input = InkpodSelectionInput()
            input.struct_size = UInt32(MemoryLayout<InkpodSelectionInput>.size)
            input.shape = inkpod_bridge_selection_rectangle()
            input.operation = inkpod_bridge_selection_new()
            input.bounds = InkpodFrameRect(
                x: 0,
                y: 0,
                width: Int32(before.documentWidth),
                height: Int32(before.documentHeight)
            )
            input.interpretation = inkpod_bridge_range_normal()
            input.trace_shape = inkpod_bridge_trace_round()
            input.view_zoom_q16 = 1 << 16
            var result = InkpodDispatchResult()
            result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
            let status = CoreStatus(
                cValue: inkpod_core_apply_selection(entry.core, &input, &result)
            )
            guard status == .ok else { return .failed(.coreOperation(status)) }
            guard let updated = projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            if updated.documentRevision == before.documentRevision {
                return .noOp(updated)
            }
            return .documentUpdated(updated)
        }
    }

    private func createBareCore() -> BareCoreResult {
        var config = InkpodCoreConfig()
        config.struct_size = UInt32(MemoryLayout<InkpodCoreConfig>.size)
        config.abi_version = inkpod_bridge_abi_version()
        config.feature_flags = inkpod_bridge_feature_none()
        var core: OpaquePointer?
        let status = CoreStatus(cValue: inkpod_core_create(&config, &core))
        guard status == .ok, let core else {
            if core != nil { _ = inkpod_core_destroy(&core) }
            return .failure(status == .ok ? .panic : status)
        }
        return .success(core)
    }

    private func installStagedCore(
        stagedOwner: inout OpaquePointer?,
        replacing oldEntry: CoreSessionEntry,
        documentUUID: CoreDocumentUUID,
        operation: CoreFileOperation
    ) -> CoreRequestOutcome {
        guard documentUUID.isValid, let staged = stagedOwner else {
            _ = inkpod_core_destroy(&stagedOwner)
            return .failed(.coreOperation(.invalidArgument))
        }
        if let duplicate = sessionByDocumentUUID[documentUUID],
           duplicate != oldEntry.target.id
        {
            _ = inkpod_core_destroy(&stagedOwner)
            return .failed(.staleTarget)
        }
        var replacementViews: [CoreViewID: CoreViewEntry] = [:]
        for view in oldEntry.views.values.sorted(by: {
            $0.target.id.rawValue < $1.target.id.rawValue
        }) {
            if view.coreViewID == 0 {
                replacementViews[view.target.id] = view
                continue
            }
            var newCoreViewID: UInt64 = 0
            let createStatus = CoreStatus(
                cValue: inkpod_core_view_create(staged, &newCoreViewID)
            )
            guard createStatus == .ok, newCoreViewID != 0 else {
                _ = inkpod_core_destroy(&stagedOwner)
                return .failed(.coreOperation(createStatus == .ok ? .panic : createStatus))
            }
            replacementViews[view.target.id] = CoreViewEntry(
                target: view.target,
                coreViewID: newCoreViewID
            )
        }
        var oldCore: OpaquePointer? = oldEntry.core
        let destroyStatus = CoreStatus(cValue: inkpod_core_destroy(&oldCore))
        guard destroyStatus == .ok, oldCore == nil else {
            _ = inkpod_core_destroy(&stagedOwner)
            return .failed(.coreOperation(destroyStatus))
        }
        stagedOwner = nil
        var entry = oldEntry
        entry.core = staged
        entry.documentUUID = documentUUID
        entry.views = replacementViews
        entry.activeTransient = nil
        sessions[entry.target.id] = entry
        sessionByDocumentUUID.removeValue(forKey: oldEntry.documentUUID)
        sessionByDocumentUUID[documentUUID] = entry.target.id
        guard let updated = projection(for: entry) else {
            return .failed(.coreOperation(.panic))
        }
        return .fileCompleted(CoreFileProjection(operation: operation, session: updated))
    }

    private func renderClipboard(_ clipboard: OpaquePointer) -> CoreClipboardRaster? {
        var output = InkpodClipboardRasterBuffer()
        output.struct_size = UInt32(MemoryLayout<InkpodClipboardRasterBuffer>.size)
        let queryStatus = CoreStatus(
            cValue: inkpod_clipboard_render_rgba8(clipboard, &output)
        )
        guard queryStatus == .ok,
              output.required_bytes > 0,
              output.required_bytes <= 256 * 1_024 * 1_024,
              output.required_bytes <= UInt64(Int.max)
        else {
            return nil
        }
        var pixels = [UInt8](repeating: 0, count: Int(output.required_bytes))
        let renderStatus = pixels.withUnsafeMutableBytes { rawPixels in
            output.pixels_rgba8 = rawPixels.bindMemory(to: UInt8.self).baseAddress
            output.pixel_capacity = UInt64(rawPixels.count)
            return CoreStatus(cValue: inkpod_clipboard_render_rgba8(clipboard, &output))
        }
        guard renderStatus == .ok else { return nil }
        return CoreClipboardRaster(
            originX: output.origin_x,
            originY: output.origin_y,
            width: output.width,
            height: output.height,
            rowStrideBytes: output.row_stride_bytes,
            rgba8: pixels
        )
    }

    private func installClipboard(_ clipboard: inout OpaquePointer?) -> CoreClipboardID? {
        guard let handle = clipboard,
              nextClipboardID != 0,
              nextClipboardID < UInt64.max
        else {
            return nil
        }
        let id = CoreClipboardID(rawValue: nextClipboardID)
        nextClipboardID += 1
        clipboards[id] = handle
        clipboard = nil
        return id
    }

    private func inspectM8(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard projection(for: entry)?.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            guard let state = m8Projection(for: entry) else {
                return .failed(.coreOperation(.panic))
            }
            return .m8State(state)
        }
    }

    private func performM8(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        command: CoreM8Command,
        requestID: CoreRequestID
    ) -> CoreRequestOutcome {
        guard command.isValid else { return .failed(.invalidRequest) }
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case var .live(entry):
            guard projection(for: entry)?.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            switch command {
            case let .beginFilterPreview(input):
                guard entry.activeTransient == nil else {
                    return .failed(.coreOperation(.invalidState))
                }
                var info = InkpodFilterPreviewInfo()
                info.struct_size = UInt32(MemoryLayout<InkpodFilterPreviewInfo>.size)
                let status = withCoreTask(requestID: requestID) { task in
                    withFFIFilter(input) { filter in
                        CoreStatus(cValue: inkpod_core_filter_preview_begin_task(
                            entry.core,
                            filter,
                            task,
                            &info
                        ))
                    }
                }
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = .filterPreview
                sessions[target.id] = entry
                return filterPreviewProjection(info, entry: entry)
            case let .updateFilterPreview(input):
                guard entry.activeTransient == .filterPreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                var info = InkpodFilterPreviewInfo()
                info.struct_size = UInt32(MemoryLayout<InkpodFilterPreviewInfo>.size)
                let status = withCoreTask(requestID: requestID) { task in
                    withFFIFilter(input) { filter in
                        CoreStatus(cValue: inkpod_core_filter_preview_update_task(
                            entry.core,
                            filter,
                            task,
                            &info
                        ))
                    }
                }
                guard status == .ok else { return m8Failure(status) }
                return filterPreviewProjection(info, entry: entry)
            case .cancelFilterPreview:
                guard entry.activeTransient == .filterPreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                var info = InkpodFilterPreviewInfo()
                info.struct_size = UInt32(MemoryLayout<InkpodFilterPreviewInfo>.size)
                let status = CoreStatus(cValue: inkpod_core_filter_preview_cancel(
                    entry.core,
                    &info
                ))
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = nil
                sessions[target.id] = entry
                return m8StateOutcome(entry)
            case .applyFilterPreview:
                guard entry.activeTransient == .filterPreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                var result = dispatchResult()
                let status = CoreStatus(cValue: inkpod_core_filter_preview_apply(
                    entry.core,
                    &result
                ))
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = nil
                sessions[target.id] = entry
                return m8MutationOutcome(entry, before: expectedDocumentRevision)
            case let .applyLastFilter(planeID):
                guard entry.activeTransient == nil else {
                    return .failed(.coreOperation(.invalidState))
                }
                var result = dispatchResult()
                let status = withCoreTask(requestID: requestID) { task in
                    CoreStatus(cValue: inkpod_core_filter_apply_last_task(
                        entry.core,
                        planeID,
                        task,
                        &result
                    ))
                }
                guard status == .ok else { return m8Failure(status) }
                return m8MutationOutcome(entry, before: expectedDocumentRevision)
            case let .createAdjustment(input, name):
                guard !name.isEmpty, name.utf8.count <= 4_096,
                      entry.activeTransient == nil
                else { return .failed(.invalidRequest) }
                var result = dispatchResult()
                var layerID: UInt64 = 0
                let bytes = Array(name.utf8)
                let status = withFFIFilter(input) { filter in
                    bytes.withUnsafeBufferPointer { nameBuffer in
                        CoreStatus(cValue: inkpod_core_adjustment_create(
                            entry.core,
                            filter,
                            nameBuffer.baseAddress,
                            UInt64(nameBuffer.count),
                            &result,
                            &layerID
                        ))
                    }
                }
                guard status == .ok else { return m8Failure(status) }
                return m8MutationOutcome(
                    entry,
                    before: expectedDocumentRevision,
                    createdIDs: layerID == 0 ? [] : [layerID]
                )
            case let .updateAdjustment(layerID, input):
                guard layerID != 0, entry.activeTransient == nil else {
                    return .failed(.invalidRequest)
                }
                var result = dispatchResult()
                let status = withFFIFilter(input) { filter in
                    CoreStatus(cValue: inkpod_core_adjustment_update(
                        entry.core,
                        layerID,
                        filter,
                        &result
                    ))
                }
                guard status == .ok else { return m8Failure(status) }
                return m8MutationOutcome(entry, before: expectedDocumentRevision)
            case let .beginGeometryPreview(input):
                guard entry.activeTransient == nil else {
                    return .failed(.coreOperation(.invalidState))
                }
                var info = InkpodGeometryPreviewInfo()
                info.struct_size = UInt32(MemoryLayout<InkpodGeometryPreviewInfo>.size)
                let status = withFFIGeometry(input) { geometry in
                    CoreStatus(cValue: inkpod_core_geometry_preview_begin(
                        entry.core,
                        geometry,
                        &info
                    ))
                }
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = .geometryPreview
                sessions[target.id] = entry
                return geometryPreviewProjection(info, entry: entry)
            case let .updateGeometryPreview(input):
                guard entry.activeTransient == .geometryPreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                var info = InkpodGeometryPreviewInfo()
                info.struct_size = UInt32(MemoryLayout<InkpodGeometryPreviewInfo>.size)
                let status = withFFIGeometry(input) { geometry in
                    CoreStatus(cValue: inkpod_core_geometry_preview_update(
                        entry.core,
                        geometry,
                        &info
                    ))
                }
                guard status == .ok else { return m8Failure(status) }
                return geometryPreviewProjection(info, entry: entry)
            case .cancelGeometryPreview:
                guard entry.activeTransient == .geometryPreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                let status = CoreStatus(cValue: inkpod_core_geometry_preview_cancel(entry.core))
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = nil
                sessions[target.id] = entry
                return m8StateOutcome(entry)
            case .commitGeometryPreview:
                guard entry.activeTransient == .geometryPreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                var result = dispatchResult()
                var pathID: UInt64 = 0
                var fillID: UInt64 = 0
                let status = CoreStatus(cValue: inkpod_core_geometry_preview_commit(
                    entry.core,
                    &result,
                    &pathID,
                    &fillID
                ))
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = nil
                sessions[target.id] = entry
                return m8MutationOutcome(
                    entry,
                    before: expectedDocumentRevision,
                    createdIDs: [pathID, fillID].filter { $0 != 0 }
                )
            case let .applyGeometry(input):
                guard entry.activeTransient == nil else {
                    return .failed(.coreOperation(.invalidState))
                }
                var result = dispatchResult()
                var pathID: UInt64 = 0
                var fillID: UInt64 = 0
                let status = withFFIGeometry(input) { geometry in
                    CoreStatus(cValue: inkpod_core_geometry_apply(
                        entry.core,
                        geometry,
                        &result,
                        &pathID,
                        &fillID
                    ))
                }
                guard status == .ok else { return m8Failure(status) }
                return m8MutationOutcome(
                    entry,
                    before: expectedDocumentRevision,
                    createdIDs: [pathID, fillID].filter { $0 != 0 }
                )
            case let .vector(command):
                guard entry.activeTransient == nil else {
                    return .failed(.coreOperation(.invalidState))
                }
                return performVector(
                    command,
                    entry: entry,
                    before: expectedDocumentRevision
                )
            case let .effect(command):
                guard entry.activeTransient == nil else {
                    return .failed(.coreOperation(.invalidState))
                }
                return performEffect(
                    command,
                    entry: entry,
                    before: expectedDocumentRevision,
                    requestID: requestID
                )
            case let .annotation(edits):
                guard entry.activeTransient == nil else {
                    return .failed(.coreOperation(.invalidState))
                }
                return performAnnotation(
                    edits,
                    entry: entry,
                    before: expectedDocumentRevision
                )
            case let .shootingFrameCreate(frame, preview):
                return performShootingFrame(
                    frame,
                    kind: 1,
                    preview: preview,
                    entry: &entry,
                    before: expectedDocumentRevision
                )
            case let .shootingFrameUpdate(frame, preview):
                return performShootingFrame(
                    frame,
                    kind: 2,
                    preview: preview,
                    entry: &entry,
                    before: expectedDocumentRevision
                )
            case let .shootingFrameDelete(id):
                guard entry.activeTransient == nil else {
                    return .failed(.coreOperation(.invalidState))
                }
                var revision: UInt64 = 0
                var outputID: UInt64 = 0
                let status = CoreStatus(cValue: inkpod_core_shooting_frame_edit(
                    entry.core,
                    expectedDocumentRevision,
                    3,
                    id,
                    nil,
                    &revision,
                    &outputID
                ))
                guard status == .ok else { return m8Failure(status) }
                return m8MutationOutcome(entry, before: expectedDocumentRevision)
            case let .shootingFramePreviewUpdate(frame):
                guard entry.activeTransient == .shootingFramePreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                var input = ffiShootingFrame(frame)
                let status = CoreStatus(cValue: inkpod_core_shooting_frame_preview_update(
                    entry.core,
                    &input
                ))
                guard status == .ok else { return m8Failure(status) }
                return m8StateOutcome(entry)
            case .shootingFramePreviewApply:
                guard entry.activeTransient == .shootingFramePreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                var revision: UInt64 = 0
                var frameID: UInt64 = 0
                let status = CoreStatus(cValue: inkpod_core_shooting_frame_preview_apply(
                    entry.core,
                    &revision,
                    &frameID
                ))
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = nil
                sessions[target.id] = entry
                return m8MutationOutcome(
                    entry,
                    before: expectedDocumentRevision,
                    createdIDs: frameID == 0 ? [] : [frameID]
                )
            case .shootingFramePreviewCancel:
                guard entry.activeTransient == .shootingFramePreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                let status = CoreStatus(cValue: inkpod_core_shooting_frame_preview_cancel(entry.core))
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = nil
                sessions[target.id] = entry
                return m8StateOutcome(entry)
            case let .vanishingPointCreate(point, preview):
                return performVanishingPoint(
                    point,
                    kind: 1,
                    preview: preview,
                    entry: &entry,
                    before: expectedDocumentRevision
                )
            case let .vanishingPointUpdate(point, preview):
                return performVanishingPoint(
                    point,
                    kind: 2,
                    preview: preview,
                    entry: &entry,
                    before: expectedDocumentRevision
                )
            case let .vanishingPointDelete(id):
                return editVanishingPoint(
                    kind: 3,
                    pointID: id,
                    input: nil,
                    entry: entry,
                    before: expectedDocumentRevision
                )
            case .vanishingPointDeleteAll:
                return editVanishingPoint(
                    kind: 4,
                    pointID: 0,
                    input: nil,
                    entry: entry,
                    before: expectedDocumentRevision
                )
            case let .vanishingPointPreviewUpdate(point):
                guard entry.activeTransient == .vanishingPointPreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                var input = ffiVanishingPoint(point)
                let status = CoreStatus(cValue: inkpod_core_vanishing_point_preview_update(
                    entry.core,
                    &input
                ))
                guard status == .ok else { return m8Failure(status) }
                return m8StateOutcome(entry)
            case .vanishingPointPreviewApply:
                guard entry.activeTransient == .vanishingPointPreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                var revision: UInt64 = 0
                var pointID: UInt64 = 0
                let status = CoreStatus(cValue: inkpod_core_vanishing_point_preview_apply(
                    entry.core,
                    &revision,
                    &pointID
                ))
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = nil
                sessions[target.id] = entry
                return m8MutationOutcome(
                    entry,
                    before: expectedDocumentRevision,
                    createdIDs: pointID == 0 ? [] : [pointID]
                )
            case .vanishingPointPreviewCancel:
                guard entry.activeTransient == .vanishingPointPreview else {
                    return .failed(.coreOperation(.invalidState))
                }
                let status = CoreStatus(cValue: inkpod_core_vanishing_point_preview_cancel(entry.core))
                guard status == .ok else { return m8Failure(status) }
                entry.activeTransient = nil
                sessions[target.id] = entry
                return m8StateOutcome(entry)
            }
        }
    }

    private func withCoreTask(
        requestID: CoreRequestID,
        body: (OpaquePointer) -> CoreStatus
    ) -> CoreStatus {
        var task: OpaquePointer?
        let createStatus = CoreStatus(cValue: inkpod_task_create(&task))
        guard createStatus == .ok, let rawTask = task else {
            if task != nil { _ = inkpod_task_release(&task) }
            return createStatus == .ok ? .panic : createStatus
        }
        cancellations.begin(requestID, task: rawTask)
        defer {
            cancellations.finish(requestID)
            _ = inkpod_task_release(&task)
        }
        return body(rawTask)
    }

    private func m8Failure(_ status: CoreStatus) -> CoreRequestOutcome {
        return .failed(status == .cancelled ? .cancelled : .coreOperation(status))
    }

    private func m8StateOutcome(_ entry: CoreSessionEntry) -> CoreRequestOutcome {
        guard let state = m8Projection(for: entry) else {
            return .failed(.coreOperation(.panic))
        }
        return .m8State(state)
    }

    private func m8MutationOutcome(
        _ entry: CoreSessionEntry,
        before: UInt64,
        createdIDs: [UInt64] = []
    ) -> CoreRequestOutcome {
        guard let state = m8Projection(for: entry) else {
            return .failed(.coreOperation(.panic))
        }
        return state.session.documentRevision == before
            ? .noOp(state.session)
            : .m8Mutation(CoreM8MutationProjection(state: state, createdIDs: createdIDs))
    }

    private func filterPreviewProjection(
        _ info: InkpodFilterPreviewInfo,
        entry: CoreSessionEntry
    ) -> CoreRequestOutcome {
        guard let session = projection(for: entry) else {
            return .failed(.coreOperation(.panic))
        }
        return .filterPreview(CoreFilterPreviewProjection(
            session: session,
            planeID: info.plane_id,
            baseChecksum: info.base_checksum,
            previewChecksum: info.preview_checksum,
            previewRevision: info.preview_revision
        ))
    }

    private func geometryPreviewProjection(
        _ info: InkpodGeometryPreviewInfo,
        entry: CoreSessionEntry
    ) -> CoreRequestOutcome {
        guard let session = projection(for: entry) else {
            return .failed(.coreOperation(.panic))
        }
        return .geometryPreview(CoreGeometryPreviewProjection(
            session: session,
            planeID: info.plane_id,
            baseRevision: info.base_revision,
            previewRevision: info.preview_revision
        ))
    }

    private func m8Projection(for entry: CoreSessionEntry) -> CoreM8Projection? {
        guard let session = projection(for: entry) else { return nil }
        var present: UInt32 = 0
        var frame = InkpodShootingFrameInfo()
        frame.struct_size = UInt32(MemoryLayout<InkpodShootingFrameInfo>.size)
        guard CoreStatus(cValue: inkpod_core_shooting_frame_get(
            entry.core,
            &present,
            &frame
        )) == .ok else { return nil }
        let shootingFrame: CoreShootingFrameProjection?
        if present != 0, let anchor = CoreShootingFrameAnchor(rawValue: frame.anchor) {
            let corners = withUnsafeBytes(of: frame.corners) { raw -> [(Int64, Int64)] in
                let points = raw.bindMemory(to: InkpodShootingFramePoint.self)
                return points.prefix(4).map { ($0.x_milli, $0.y_milli) }
            }
            shootingFrame = CoreShootingFrameProjection(
                id: frame.frame_id,
                anchor: anchor,
                centerXMilli: frame.center_x_milli,
                centerYMilli: frame.center_y_milli,
                widthMilli: frame.width_milli,
                heightMilli: frame.height_milli,
                rotationTurns: frame.rotation_turns,
                visible: frame.visible != 0,
                includeInInstructionExport: frame.include_in_instruction_export != 0,
                corners: corners
            )
        } else {
            shootingFrame = nil
        }
        var count: UInt64 = 0
        let stride = UInt64(MemoryLayout<InkpodVanishingPointInfo>.stride)
        let queryStatus = CoreStatus(cValue: inkpod_core_vanishing_points_copy(
            entry.core,
            nil,
            0,
            stride,
            &count
        ))
        guard (queryStatus == .ok || queryStatus == .bufferTooSmall),
            count <= 4_096,
            count <= UInt64(Int.max)
        else { return nil }
        var records = [InkpodVanishingPointInfo](
            repeating: InkpodVanishingPointInfo(),
            count: Int(count)
        )
        for index in records.indices {
            records[index].struct_size = UInt32(MemoryLayout<InkpodVanishingPointInfo>.size)
        }
        if !records.isEmpty {
            let status = records.withUnsafeMutableBufferPointer { buffer in
                CoreStatus(cValue: inkpod_core_vanishing_points_copy(
                    entry.core,
                    buffer.baseAddress,
                    UInt64(buffer.count),
                    stride,
                    &count
                ))
            }
            guard status == .ok else { return nil }
        }
        let points = records.compactMap(coreVanishingPoint)
        guard points.count == records.count else { return nil }
        return CoreM8Projection(
            session: session,
            shootingFrame: shootingFrame,
            vanishingPoints: points
        )
    }

    private func withFFIFilter<Result>(
        _ request: CoreFilterRequest,
        body: (UnsafePointer<InkpodFilterInput>) -> Result
    ) -> Result {
        let points = request.curvePoints.map { point in
            var output = InkpodCurvePoint()
            output.struct_size = UInt32(MemoryLayout<InkpodCurvePoint>.size)
            output.input = point.input
            output.output = point.output
            return output
        }
        return points.withUnsafeBufferPointer { pointBuffer in
            var input = InkpodFilterInput()
            input.struct_size = UInt32(MemoryLayout<InkpodFilterInput>.size)
            input.kind = request.kind.rawValue
            input.plane_id = request.planeID
            input.channel = request.channel.rawValue
            input.interpolation = request.interpolation.rawValue
            let parameters = request.parameters + Array(
                repeating: 0,
                count: 5 - request.parameters.count
            )
            input.parameter_0 = parameters[0]
            input.parameter_1 = parameters[1]
            input.parameter_2 = parameters[2]
            input.parameter_3 = parameters[3]
            input.parameter_4 = parameters[4]
            input.point_stride_bytes = pointBuffer.isEmpty
                ? 0
                : UInt32(MemoryLayout<InkpodCurvePoint>.stride)
            input.points = pointBuffer.isEmpty ? nil : pointBuffer.baseAddress
            input.point_count = UInt64(pointBuffer.count)
            return withUnsafePointer(to: &input, body)
        }
    }

    private func withFFIGeometry<Result>(
        _ request: CoreGeometryRequest,
        body: (UnsafePointer<InkpodGeometryInput>) -> Result
    ) -> Result {
        let points = request.points.map { point in
            var output = InkpodGeometryPoint()
            output.struct_size = UInt32(MemoryLayout<InkpodGeometryPoint>.size)
            output.x = point.x
            output.y = point.y
            return output
        }
        return points.withUnsafeBufferPointer { pointBuffer in
            var input = InkpodGeometryInput()
            input.struct_size = UInt32(MemoryLayout<InkpodGeometryInput>.size)
            input.primitive = request.primitive.rawValue
            input.feature_flags = request.options.featureFlags
            input.plane_id = request.planeID
            input.base_revision = request.baseRevision
            input.outline_color = ffiColor(request.outlineColor)
            input.fill_color = ffiColor(request.fillColor)
            input.outline_width = request.outlineWidth
            input.aspect_ratio_q16 = request.aspectRatioQ16
            input.polygon_sides = request.polygonSides
            input.rotation_turns = request.rotationTurns
            input.points = pointBuffer.baseAddress
            input.point_count = UInt64(pointBuffer.count)
            input.point_stride_bytes = UInt64(MemoryLayout<InkpodGeometryPoint>.stride)
            return withUnsafePointer(to: &input, body)
        }
    }

    private func performVector(
        _ command: CoreVectorCommand,
        entry: CoreSessionEntry,
        before: UInt64
    ) -> CoreRequestOutcome {
        var result = dispatchResult()
        var createdID: UInt64 = 0
        let status: CoreStatus
        switch command {
        case let .erase(planeID, x, y, radius, mode):
            var input = InkpodVectorEraseInput()
            input.struct_size = UInt32(MemoryLayout<InkpodVectorEraseInput>.size)
            input.mode = mode.rawValue
            input.plane_id = planeID
            input.x = x
            input.y = y
            input.radius = radius
            status = CoreStatus(cValue: inkpod_core_vector_erase(entry.core, &input, &result))
        case let .connect(planeID, maximumGap):
            status = CoreStatus(cValue: inkpod_core_vector_connect(
                entry.core,
                planeID,
                maximumGap,
                &result,
                &createdID
            ))
        case let .correctWidth(ids, mode, parameter):
            status = ids.withUnsafeBufferPointer { buffer in
                var input = InkpodVectorWidthInput()
                input.struct_size = UInt32(MemoryLayout<InkpodVectorWidthInput>.size)
                input.mode = mode.rawValue
                input.path_ids = buffer.baseAddress
                input.path_count = UInt64(buffer.count)
                input.parameter = parameter
                return CoreStatus(cValue: inkpod_core_vector_correct_width(
                    entry.core,
                    &input,
                    &result
                ))
            }
        case let .select(mode, bounds):
            return vectorSelection(mode: mode, bounds: bounds, entry: entry)
        case let .rasterizeToLayer(layerID, scale, antialias, name):
            var input = InkpodVectorRasterizeInput()
            input.struct_size = UInt32(MemoryLayout<InkpodVectorRasterizeInput>.size)
            input.feature_flags = antialias ? 1 : 0
            input.layer_id = layerID
            input.scale = scale
            let bytes = Array(name.utf8)
            status = bytes.withUnsafeBufferPointer { buffer in
                CoreStatus(cValue: inkpod_core_vector_rasterize_to_layer(
                    entry.core,
                    &input,
                    buffer.baseAddress,
                    UInt64(buffer.count),
                    &result,
                    &createdID
                ))
            }
        case let .vectorize(sourcePlaneID, targetLayerID, alphaThreshold):
            var input = InkpodRasterVectorizeInput()
            input.struct_size = UInt32(MemoryLayout<InkpodRasterVectorizeInput>.size)
            input.alpha_threshold = alphaThreshold
            input.source_plane_id = sourcePlaneID
            input.target_layer_id = targetLayerID
            status = CoreStatus(cValue: inkpod_core_raster_vectorize(
                entry.core,
                &input,
                &result,
                &createdID
            ))
        }
        guard status == .ok else { return m8Failure(status) }
        return m8MutationOutcome(
            entry,
            before: before,
            createdIDs: createdID == 0 ? [] : [createdID]
        )
    }

    private func vectorSelection(
        mode: CoreVectorSelectionMode,
        bounds: CoreFrameRect,
        entry: CoreSessionEntry
    ) -> CoreRequestOutcome {
        var input = InkpodVectorSelectionInput()
        input.struct_size = UInt32(MemoryLayout<InkpodVectorSelectionInput>.size)
        input.mode = mode.rawValue
        input.bounds = ffiFrame(bounds)
        var output = InkpodVectorSelectionBuffer()
        output.struct_size = UInt32(MemoryLayout<InkpodVectorSelectionBuffer>.size)
        var status = CoreStatus(cValue: inkpod_core_vector_select(entry.core, &input, &output))
        guard status == .ok || status == .bufferTooSmall,
              output.range_count <= 65_536,
              output.fill_count <= 65_536
        else { return m8Failure(status) }
        var ranges = [InkpodVectorSelectionRange](
            repeating: InkpodVectorSelectionRange(),
            count: Int(output.range_count)
        )
        var fillIDs = [UInt64](repeating: 0, count: Int(output.fill_count))
        for index in ranges.indices {
            ranges[index].struct_size = UInt32(MemoryLayout<InkpodVectorSelectionRange>.size)
        }
        status = ranges.withUnsafeMutableBufferPointer { rangeBuffer in
            fillIDs.withUnsafeMutableBufferPointer { fillBuffer in
                output.ranges = rangeBuffer.isEmpty ? nil : rangeBuffer.baseAddress
                output.range_capacity = UInt64(rangeBuffer.count)
                output.fill_ids = fillBuffer.isEmpty ? nil : fillBuffer.baseAddress
                output.fill_capacity = UInt64(fillBuffer.count)
                return CoreStatus(cValue: inkpod_core_vector_select(
                    entry.core,
                    &input,
                    &output
                ))
            }
        }
        guard status == .ok, let session = projection(for: entry) else {
            return m8Failure(status == .ok ? .panic : status)
        }
        return .vectorSelection(CoreVectorSelectionProjection(
            session: session,
            ranges: ranges.map {
                CoreVectorSelectionRange(
                    pathID: $0.path_id,
                    startMillion: $0.start_million,
                    endMillion: $0.end_million
                )
            },
            fillIDs: fillIDs
        ))
    }

    private func performEffect(
        _ command: CoreEffectCommand,
        entry: CoreSessionEntry,
        before: UInt64,
        requestID: CoreRequestID
    ) -> CoreRequestOutcome {
        var result = dispatchResult()
        let status: CoreStatus
        switch command {
        case let .gradient(request, alphaOnly):
            status = withFFIGradient(request) { input in
                CoreStatus(cValue: alphaOnly
                    ? inkpod_core_alpha_gradient(entry.core, input, &result)
                    : inkpod_core_effect_gradient(entry.core, input, &result))
            }
        case let .airbrush(planeID, x, y, radius, hardness, opacity, color):
            var input = InkpodAirbrushInput()
            input.struct_size = UInt32(MemoryLayout<InkpodAirbrushInput>.size)
            input.plane_id = planeID
            input.center_x_milli = Int64((x * 1_000).rounded())
            input.center_y_milli = Int64((y * 1_000).rounded())
            input.radius_milli = UInt32((radius * 1_000).rounded())
            input.hardness_milli = hardness
            input.opacity_milli = opacity
            input.color = ffiColor(color)
            status = CoreStatus(cValue: inkpod_core_effect_airbrush(entry.core, &input, &result))
        case let .boundaryAirbrush(planeID, width, strength, colors):
            let ffiColors = colors.map(ffiColor)
            status = ffiColors.withUnsafeBufferPointer { buffer in
                var input = InkpodBoundaryAirbrushInput()
                input.struct_size = UInt32(MemoryLayout<InkpodBoundaryAirbrushInput>.size)
                input.plane_id = planeID
                input.width = width
                input.strength_milli = strength
                input.colors.struct_size = UInt32(MemoryLayout<InkpodColorArray>.size)
                input.colors.colors = buffer.baseAddress
                input.colors.color_count = UInt64(buffer.count)
                input.colors.color_stride_bytes = UInt64(MemoryLayout<InkpodColorValue>.stride)
                return CoreStatus(cValue: inkpod_core_effect_boundary_airbrush(
                    entry.core,
                    &input,
                    &result
                ))
            }
        case let .blur(planeID, radius, strength):
            var input = InkpodBlurEffectInput()
            input.struct_size = UInt32(MemoryLayout<InkpodBlurEffectInput>.size)
            input.plane_id = planeID
            input.radius = radius
            input.strength_milli = strength
            status = CoreStatus(cValue: inkpod_core_effect_blur(entry.core, &input, &result))
        case let .stamp(planeID, sourceX, sourceY, destinationX, destinationY, width, height, opacity):
            var input = InkpodStampInput()
            input.struct_size = UInt32(MemoryLayout<InkpodStampInput>.size)
            input.plane_id = planeID
            input.source_x = sourceX
            input.source_y = sourceY
            input.destination_x = destinationX
            input.destination_y = destinationY
            input.width = width
            input.height = height
            input.opacity_milli = opacity
            status = CoreStatus(cValue: inkpod_core_effect_stamp(entry.core, &input, &result))
        case let .dust(planeID, mode, maximumPixels):
            var input = InkpodDustInput()
            input.struct_size = UInt32(MemoryLayout<InkpodDustInput>.size)
            input.mode = mode.rawValue
            input.plane_id = planeID
            input.coordinate_space = inkpod_bridge_coordinate_document()
            input.maximum_pixels = maximumPixels
            status = withCoreTask(requestID: requestID) { task in
                CoreStatus(cValue: inkpod_core_dust_remove(
                    entry.core,
                    &input,
                    task,
                    &result
                ))
            }
        }
        guard status == .ok else { return m8Failure(status) }
        return m8MutationOutcome(entry, before: before)
    }

    private func withFFIGradient<Result>(
        _ request: CoreGradientRequest,
        body: (UnsafePointer<InkpodGradientInput>) -> Result
    ) -> Result {
        let stops = request.stops.map { stop in
            var output = InkpodGradientStop()
            output.struct_size = UInt32(MemoryLayout<InkpodGradientStop>.size)
            output.position_milli = stop.positionMilli
            output.color = ffiColor(stop.color)
            return output
        }
        return stops.withUnsafeBufferPointer { buffer in
            var input = InkpodGradientInput()
            input.struct_size = UInt32(MemoryLayout<InkpodGradientInput>.size)
            input.kind = request.kind.rawValue
            input.feature_flags = request.constrainTo45Degrees ? 1 : 0
            input.plane_id = request.planeID
            input.mode = request.mode.rawValue
            input.dither = request.dither ? 1 : 0
            input.start_x_milli = Int64((request.startX * 1_000).rounded())
            input.start_y_milli = Int64((request.startY * 1_000).rounded())
            input.end_x_milli = Int64((request.endX * 1_000).rounded())
            input.end_y_milli = Int64((request.endY * 1_000).rounded())
            input.stops = buffer.baseAddress
            input.stop_count = UInt64(buffer.count)
            input.stop_stride_bytes = UInt64(MemoryLayout<InkpodGradientStop>.stride)
            return withUnsafePointer(to: &input, body)
        }
    }

    private func performAnnotation(
        _ edits: [CoreAnnotationEdit],
        entry: CoreSessionEntry,
        before: UInt64
    ) -> CoreRequestOutcome {
        let createCount = edits.reduce(into: 0) { count, edit in
            if case .create = edit { count += 1 }
        }
        var createdIDs = [UInt64](repeating: 0, count: createCount)
        var result = InkpodAnnotationEditResult()
        result.struct_size = UInt32(MemoryLayout<InkpodAnnotationEditResult>.size)
        let status = createdIDs.withUnsafeMutableBufferPointer { createdBuffer in
            result.created_ids = createdBuffer.baseAddress
            result.created_capacity = UInt64(createdBuffer.count)
            return withFFIAnnotationEdits(edits) { editBuffer in
                CoreStatus(cValue: inkpod_core_annotation_edit(
                    entry.core,
                    before,
                    editBuffer.baseAddress,
                    UInt64(editBuffer.count),
                    UInt64(MemoryLayout<InkpodAnnotationEdit>.stride),
                    &result
                ))
            }
        }
        guard status == .ok else { return m8Failure(status) }
        return m8MutationOutcome(
            entry,
            before: before,
            createdIDs: Array(createdIDs.prefix(Int(result.created_count)))
        )
    }

    private func withFFIAnnotationEdits<Result>(
        _ edits: [CoreAnnotationEdit],
        body: (UnsafeBufferPointer<InkpodAnnotationEdit>) -> Result
    ) -> Result {
        let objects: [CoreAnnotationObject?] = edits.map { edit in
            switch edit {
            case let .create(object), let .update(_, object): object
            case .move, .delete: nil
            }
        }
        var ffiObjects = [InkpodAnnotationObjectInput](
            repeating: InkpodAnnotationObjectInput(),
            count: edits.count
        )
        var ffiEdits = [InkpodAnnotationEdit](
            repeating: InkpodAnnotationEdit(),
            count: edits.count
        )
        let fonts = objects.map { $0.map { Array($0.fontFamily.utf8) } ?? [] }
        let texts = objects.map { $0.map { Array($0.text.utf8) } ?? [] }
        let points: [[InkpodAnnotationPoint]] = objects.map { object in
            object?.points.map { point in
                var output = InkpodAnnotationPoint()
                output.struct_size = UInt32(MemoryLayout<InkpodAnnotationPoint>.size)
                output.x_milli = point.xMilli
                output.y_milli = point.yMilli
                return output
            } ?? []
        }
        return ffiObjects.withUnsafeMutableBufferPointer { objectBuffer in
            ffiEdits.withUnsafeMutableBufferPointer { editBuffer in
                func bind(_ index: Int) -> Result {
                    guard index < edits.count else {
                        return body(UnsafeBufferPointer(editBuffer))
                    }
                    editBuffer[index].struct_size = UInt32(MemoryLayout<InkpodAnnotationEdit>.size)
                    switch edits[index] {
                    case .create:
                        editBuffer[index].kind = 1
                    case let .update(id, _):
                        editBuffer[index].kind = 2
                        editBuffer[index].object_id = id
                    case let .move(id, deltaX, deltaY):
                        editBuffer[index].kind = 3
                        editBuffer[index].object_id = id
                        editBuffer[index].delta_x = deltaX
                        editBuffer[index].delta_y = deltaY
                        return bind(index + 1)
                    case let .delete(id):
                        editBuffer[index].kind = 4
                        editBuffer[index].object_id = id
                        return bind(index + 1)
                    }
                    let object = objects[index]!
                    return fonts[index].withUnsafeBufferPointer { fontBuffer in
                        texts[index].withUnsafeBufferPointer { textBuffer in
                            points[index].withUnsafeBufferPointer { pointBuffer in
                                objectBuffer[index].struct_size = UInt32(
                                    MemoryLayout<InkpodAnnotationObjectInput>.size
                                )
                                objectBuffer[index].kind = object.kind.rawValue
                                objectBuffer[index].layer_id = object.layerID
                                objectBuffer[index].output = object.output.rawValue
                                objectBuffer[index].style_flags =
                                    (object.bold ? 1 : 0) | (object.italic ? 2 : 0)
                                        | (object.underline ? 4 : 0)
                                objectBuffer[index].bounds = ffiFrame(object.bounds)
                                objectBuffer[index].font_family_utf8 = fontBuffer.isEmpty
                                    ? nil
                                    : fontBuffer.baseAddress
                                objectBuffer[index].font_family_bytes = UInt64(fontBuffer.count)
                                objectBuffer[index].font_size_milli = object.fontSizeMilli
                                objectBuffer[index].stroke_width_milli = object.strokeWidthMilli
                                objectBuffer[index].color = ffiColor(object.color)
                                objectBuffer[index].text_utf8 = textBuffer.isEmpty
                                    ? nil
                                    : textBuffer.baseAddress
                                objectBuffer[index].text_bytes = UInt64(textBuffer.count)
                                objectBuffer[index].points = pointBuffer.isEmpty
                                    ? nil
                                    : pointBuffer.baseAddress
                                objectBuffer[index].point_count = UInt64(pointBuffer.count)
                                objectBuffer[index].point_stride_bytes = pointBuffer.isEmpty
                                    ? 0
                                    : UInt64(MemoryLayout<InkpodAnnotationPoint>.stride)
                                editBuffer[index].input = UnsafePointer(
                                    objectBuffer.baseAddress!.advanced(by: index)
                                )
                                return bind(index + 1)
                            }
                        }
                    }
                }
                return bind(0)
            }
        }
    }

    private func performShootingFrame(
        _ frame: CoreShootingFrame,
        kind: UInt32,
        preview: Bool,
        entry: inout CoreSessionEntry,
        before: UInt64
    ) -> CoreRequestOutcome {
        guard entry.activeTransient == nil else {
            return .failed(.coreOperation(.invalidState))
        }
        var input = ffiShootingFrame(frame)
        if preview {
            let status = CoreStatus(cValue: inkpod_core_shooting_frame_preview_begin(
                entry.core,
                before,
                kind,
                frame.id,
                &input
            ))
            guard status == .ok else { return m8Failure(status) }
            entry.activeTransient = .shootingFramePreview
            sessions[entry.target.id] = entry
            return m8StateOutcome(entry)
        }
        var revision: UInt64 = 0
        var frameID: UInt64 = 0
        let status = CoreStatus(cValue: inkpod_core_shooting_frame_edit(
            entry.core,
            before,
            kind,
            frame.id,
            &input,
            &revision,
            &frameID
        ))
        guard status == .ok else { return m8Failure(status) }
        return m8MutationOutcome(
            entry,
            before: before,
            createdIDs: frameID == 0 ? [] : [frameID]
        )
    }

    private func ffiShootingFrame(_ frame: CoreShootingFrame) -> InkpodShootingFrameInput {
        var input = InkpodShootingFrameInput()
        input.struct_size = UInt32(MemoryLayout<InkpodShootingFrameInput>.size)
        input.anchor = frame.anchor.rawValue
        input.center_x = frame.centerX
        input.center_y = frame.centerY
        input.width = frame.width
        input.height = frame.height
        input.rotation_degrees = frame.rotationDegrees
        input.visible = frame.visible ? 1 : 0
        input.include_in_instruction_export = frame.includeInInstructionExport ? 1 : 0
        return input
    }

    private func performVanishingPoint(
        _ point: CoreVanishingPoint,
        kind: UInt32,
        preview: Bool,
        entry: inout CoreSessionEntry,
        before: UInt64
    ) -> CoreRequestOutcome {
        guard entry.activeTransient == nil else {
            return .failed(.coreOperation(.invalidState))
        }
        var input = ffiVanishingPoint(point)
        if preview {
            let status = CoreStatus(cValue: inkpod_core_vanishing_point_preview_begin(
                entry.core,
                before,
                kind,
                point.id,
                &input
            ))
            guard status == .ok else { return m8Failure(status) }
            entry.activeTransient = .vanishingPointPreview
            sessions[entry.target.id] = entry
            return m8StateOutcome(entry)
        }
        return editVanishingPoint(
            kind: kind,
            pointID: point.id,
            input: &input,
            entry: entry,
            before: before
        )
    }

    private func editVanishingPoint(
        kind: UInt32,
        pointID: UInt64,
        input: UnsafePointer<InkpodVanishingPointInput>?,
        entry: CoreSessionEntry,
        before: UInt64
    ) -> CoreRequestOutcome {
        guard entry.activeTransient == nil else {
            return .failed(.coreOperation(.invalidState))
        }
        var revision: UInt64 = 0
        var outputID: UInt64 = 0
        let status = CoreStatus(cValue: inkpod_core_vanishing_point_edit(
            entry.core,
            before,
            kind,
            pointID,
            input,
            &revision,
            &outputID
        ))
        guard status == .ok else { return m8Failure(status) }
        return m8MutationOutcome(
            entry,
            before: before,
            createdIDs: outputID == 0 ? [] : [outputID]
        )
    }

    private func ffiVanishingPoint(_ point: CoreVanishingPoint) -> InkpodVanishingPointInput {
        var input = InkpodVanishingPointInput()
        input.struct_size = UInt32(MemoryLayout<InkpodVanishingPointInput>.size)
        input.visible = point.visible ? 1 : 0
        input.layer_id = point.layerID
        input.x_milli = point.xMilli
        input.y_milli = point.yMilli
        input.interval_milli_degrees = point.intervalMilliDegrees
        input.angle_milli_degrees = point.angleMilliDegrees
        input.opacity_milli = point.opacityMilli
        input.color = ffiColor(point.color)
        return input
    }

    private func coreVanishingPoint(_ input: InkpodVanishingPointInfo) -> CoreVanishingPoint? {
        guard let color = coreColor(input.color) else { return nil }
        return CoreVanishingPoint(
            id: input.point_id,
            layerID: input.layer_id,
            xMilli: input.x_milli,
            yMilli: input.y_milli,
            intervalMilliDegrees: input.interval_milli_degrees,
            angleMilliDegrees: input.angle_milli_degrees,
            opacityMilli: input.opacity_milli,
            visible: input.visible != 0,
            color: color
        )
    }

    private func exportInstructionRaster(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        format: CoreCommonRasterFormat,
        compositeWhite: Bool
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard projection(for: entry)?.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            var buffer: OpaquePointer?
            let status = CoreStatus(cValue: inkpod_core_export_instruction_common_raster(
                entry.core,
                ffiRasterFormat(format),
                compositeWhite ? 1 : 0,
                &buffer
            ))
            guard status == .ok, let liveBuffer = buffer else {
                if buffer != nil { _ = inkpod_byte_buffer_release(&buffer) }
                return m8Failure(status == .ok ? .panic : status)
            }
            var pointer: UnsafePointer<UInt8>?
            var count: UInt64 = 0
            let viewStatus = CoreStatus(cValue: inkpod_byte_buffer_view(
                liveBuffer,
                &pointer,
                &count
            ))
            guard viewStatus == .ok,
                  count <= 512 * 1_024 * 1_024,
                  count <= UInt64(Int.max),
                  let pointer
            else {
                _ = inkpod_byte_buffer_release(&buffer)
                return m8Failure(viewStatus == .ok ? .invalidArgument : viewStatus)
            }
            let bytes = Array(UnsafeBufferPointer(start: pointer, count: Int(count)))
            let releaseStatus = CoreStatus(cValue: inkpod_byte_buffer_release(&buffer))
            guard releaseStatus == .ok, buffer == nil else {
                return m8Failure(releaseStatus)
            }
            return .rasterExported(CoreRasterExport(format: format, bytes: bytes))
        }
    }

    private func inspectAnimation(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreRequestOutcome {
        withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            self.animationProjection(for: entry).map(CoreRequestOutcome.animation)
                ?? .failed(.coreOperation(.panic))
        }
    }

    private func performAnimation(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        command: CoreAnimationCommand
    ) -> CoreRequestOutcome {
        withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            switch command {
            case let .replaceSequence(sources):
                guard !sources.isEmpty, sources.count <= 10_000,
                      sources.allSatisfy(\.isValid)
                else { return .failed(.invalidRequest) }
                let status = self.withSequenceSources(sources) { pointer, count in
                    var input = InkpodSequenceInput()
                    input.struct_size = UInt32(MemoryLayout<InkpodSequenceInput>.size)
                    input.cells = pointer
                    input.cell_count = count
                    input.cell_stride_bytes = UInt64(MemoryLayout<InkpodSequenceCellInput>.stride)
                    return CoreStatus(cValue: inkpod_core_sequence_set(entry.core, &input))
                }
                return self.animationQueryOutcome(status, entry: entry)
            case let .importSequence(files):
                guard !files.isEmpty, files.count <= 10_000,
                      files.allSatisfy({
                          !$0.name.isEmpty && $0.name.utf8.count <= 4_096
                              && !$0.bytes.isEmpty && $0.bytes.count <= 512 * 1_024 * 1_024
                      })
                else { return .failed(.invalidRequest) }
                let status = self.withNamedRasters(files) { pointer, count in
                    CoreStatus(cValue: inkpod_core_sequence_import_mixed_encoded(
                        entry.core,
                        pointer,
                        count,
                        UInt64(MemoryLayout<InkpodNamedRasterInput>.stride)
                    ))
                }
                return self.animationQueryOutcome(status, entry: entry)
            case let .importIdentifiedSequence(files):
                guard !files.isEmpty, files.count <= 10_000,
                      files.allSatisfy(\.isValid)
                else { return .failed(.invalidRequest) }
                let status = self.withIdentifiedNamedRasters(files) {
                    filePointer, identityPointer, count in
                    CoreStatus(cValue: inkpod_core_sequence_import_mixed_encoded_identified(
                        entry.core,
                        filePointer,
                        count,
                        UInt64(MemoryLayout<InkpodNamedRasterInput>.stride),
                        identityPointer,
                        UInt64(MemoryLayout<InkpodSequenceSourceIdentity>.stride)
                    ))
                }
                return self.animationQueryOutcome(status, entry: entry)
            case let .exportSequence(format, compositeWhite):
                return self.exportSequence(
                    entry: entry,
                    format: format,
                    compositeWhite: compositeWhite
                )
            case let .activateSequence(index):
                guard self.sequenceTargetIsAvailable(entry: entry, index: index) else {
                    return .failed(.staleTarget)
                }
                var info = self.documentInfo()
                let status = CoreStatus(cValue: inkpod_core_sequence_activate(
                    entry.core,
                    index,
                    &info
                ))
                return self.animationDocumentSwitchOutcome(status, entry: entry)
            case let .resolveStep(direction, policy):
                var plan = InkpodSequenceStepPlan()
                plan.struct_size = UInt32(MemoryLayout<InkpodSequenceStepPlan>.size)
                let status = CoreStatus(cValue: inkpod_core_sequence_step_resolve(
                    entry.core,
                    direction.rawValue,
                    policy.rawValue,
                    &plan
                ))
                guard status == .ok, let projection = self.sequenceStepPlan(plan) else {
                    return self.animationFailure(status)
                }
                return .sequenceStepPlan(projection)
            case let .commitStep(plan):
                guard plan.result == .empty
                    || plan.targetDocumentUUID.map({
                        self.sessionByDocumentUUID[$0].map { $0 == entry.target.id } ?? true
                    }) == true
                else { return .failed(.staleTarget) }
                var ffiPlan = self.ffiSequenceStepPlan(plan)
                var info = self.documentInfo()
                let status = CoreStatus(cValue: inkpod_core_sequence_step_commit(
                    entry.core,
                    &ffiPlan,
                    &info
                ))
                return self.animationDocumentSwitchOutcome(status, entry: entry, staleInvalidState: true)
            case let .addLightTableItem(input):
                guard input.source.isValid, input.opacityMilli <= 1_000 else {
                    return .failed(.invalidRequest)
                }
                var result = self.dispatchResult()
                var itemID: UInt64 = 0
                let status = self.withLightTableItem(input) { ffiInput in
                    var ffiInput = ffiInput
                    return CoreStatus(cValue: inkpod_core_light_table_add_item(
                        entry.core,
                        &ffiInput,
                        &result,
                        &itemID
                    ))
                }
                return self.animationMutationOutcome(
                    status,
                    entry: entry,
                    createdIDs: itemID == 0 ? [] : [itemID],
                    applied: result.accepted_command_count > 0
                )
            case let .addLightTableRaster(file, documentUUID, sourceRevision):
                guard documentUUID.isValid, sourceRevision > 0,
                      !file.name.isEmpty, !file.bytes.isEmpty
                else { return .failed(.invalidRequest) }
                var result = self.dispatchResult()
                var itemID: UInt64 = 0
                let name = Array(file.name.utf8)
                let status = file.bytes.withUnsafeBufferPointer { byteBuffer in
                    name.withUnsafeBufferPointer { nameBuffer in
                        CoreStatus(cValue: inkpod_core_light_table_add_common_raster(
                            entry.core,
                            self.ffiRasterFormat(file.format),
                            byteBuffer.baseAddress,
                            UInt64(byteBuffer.count),
                            nameBuffer.baseAddress,
                            UInt64(nameBuffer.count),
                            documentUUID.high,
                            documentUUID.low,
                            sourceRevision,
                            &result,
                            &itemID
                        ))
                    }
                }
                return self.animationMutationOutcome(
                    status,
                    entry: entry,
                    createdIDs: itemID == 0 ? [] : [itemID],
                    applied: result.accepted_command_count > 0
                )
            case let .reloadLightTableRaster(itemID, file, documentUUID, sourceRevision):
                guard itemID > 0, documentUUID.isValid, sourceRevision > 0,
                      !file.bytes.isEmpty
                else { return .failed(.invalidRequest) }
                var result = self.dispatchResult()
                let status = file.bytes.withUnsafeBufferPointer { bytes in
                    CoreStatus(cValue: inkpod_core_light_table_reload_common_raster(
                        entry.core,
                        itemID,
                        self.ffiRasterFormat(file.format),
                        bytes.baseAddress,
                        UInt64(bytes.count),
                        documentUUID.high,
                        documentUUID.low,
                        sourceRevision,
                        &result
                    ))
                }
                return self.animationMutationOutcome(
                    status,
                    entry: entry,
                    createdIDs: [],
                    applied: result.accepted_command_count > 0
                )
            case let .editLightTable(command):
                return self.editLightTable(entry: entry, command: command)
            case let .previewLightTableBulk(setID, direction, count, opacity, step):
                guard setID > 0, count <= 10_000, opacity <= 1_000, step <= 1_000 else {
                    return .failed(.invalidRequest)
                }
                return self.previewLightTableBulk(
                    entry: entry,
                    setID: setID,
                    direction: direction,
                    neighborCount: count,
                    baseOpacityMilli: opacity,
                    distanceStepMilli: step
                )
            case let .registerLightTableBulk(request):
                return self.registerLightTableBulk(entry: entry, request: request)
            case let .sampleLightTable(x, y):
                var color = InkpodColorValue()
                color.struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
                let status = CoreStatus(cValue: inkpod_core_light_table_sample(
                    entry.core,
                    x,
                    y,
                    &color
                ))
                guard status == .ok, let sample = self.coreColor(color) else {
                    return self.animationFailure(status)
                }
                return .animationSample(sample)
            case let .swapLightTable(itemID):
                guard itemID > 0 else { return .failed(.invalidRequest) }
                guard let item = self.lightTableItems(core: entry.core)?.first(where: {
                    $0.id == itemID
                }), self.sessionByDocumentUUID[item.sourceDocumentUUID].map({
                    $0 == entry.target.id
                }) ?? true
                else { return .failed(.staleTarget) }
                var info = self.documentInfo()
                let status = CoreStatus(cValue: inkpod_core_light_table_swap(
                    entry.core,
                    itemID,
                    &info
                ))
                return self.animationDocumentSwitchOutcome(status, entry: entry)
            case let .setSubpalette(index):
                let status = CoreStatus(cValue: inkpod_core_subpalette_set(entry.core, index))
                return self.animationQueryOutcome(status, entry: entry)
            case let .sampleSubpalette(x, y):
                var color = InkpodColorValue()
                color.struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
                let status = CoreStatus(cValue: inkpod_core_subpalette_sample(
                    entry.core,
                    x,
                    y,
                    &color
                ))
                guard status == .ok, let sample = self.coreColor(color) else {
                    return self.animationFailure(status)
                }
                return .animationSample(sample)
            case let .motionStart(fps, loop, includeSelection, includeLightTable):
                guard [8, 10, 12, 24, 25, 30].contains(fps), entry.activeTransient == nil else {
                    return .failed(.invalidRequest)
                }
                var input = InkpodMotionCheckInput()
                input.struct_size = UInt32(MemoryLayout<InkpodMotionCheckInput>.size)
                input.fps = fps
                input.flags = (loop ? UInt64(1) : 0)
                    | (includeSelection ? UInt64(2) : 0)
                    | (includeLightTable ? UInt64(4) : 0)
                var frame = InkpodMotionFrame()
                frame.struct_size = UInt32(MemoryLayout<InkpodMotionFrame>.size)
                let status = CoreStatus(cValue: inkpod_core_motion_check_start(
                    entry.core,
                    &input,
                    &frame
                ))
                return self.motionOutcome(status, entry: entry, frame: frame, started: true)
            case let .motionStep(direction):
                guard entry.activeTransient == .motion else { return .failed(.invalidRequest) }
                var frame = InkpodMotionFrame()
                frame.struct_size = UInt32(MemoryLayout<InkpodMotionFrame>.size)
                let status = CoreStatus(cValue: inkpod_core_motion_check_step(
                    entry.core,
                    direction.rawValue,
                    &frame
                ))
                return self.motionOutcome(status, entry: entry, frame: frame, started: false)
            case .motionTogglePause:
                guard entry.activeTransient == .motion else { return .failed(.invalidRequest) }
                var frame = InkpodMotionFrame()
                frame.struct_size = UInt32(MemoryLayout<InkpodMotionFrame>.size)
                let status = CoreStatus(cValue: inkpod_core_motion_check_toggle_pause(
                    entry.core,
                    &frame
                ))
                return self.motionOutcome(status, entry: entry, frame: frame, started: false)
            case .motionStop:
                let status = CoreStatus(cValue: inkpod_core_motion_check_stop(entry.core))
                guard status == .ok else { return self.animationFailure(status) }
                var updated = entry
                updated.activeTransient = nil
                updated.motion = nil
                self.sessions[entry.target.id] = updated
                return self.animationProjection(for: updated).map(CoreRequestOutcome.animation)
                    ?? .failed(.coreOperation(.panic))
            }
        }
    }

    private func cancelTransient(
        _ transient: CoreTransientKind,
        core: OpaquePointer
    ) -> CoreStatus {
        switch transient {
        case .stroke:
            return CoreStatus(cValue: inkpod_core_stroke_cancel(core))
        case .floatingPaste:
            return CoreStatus(cValue: inkpod_core_floating_cancel(core))
        case .filterPreview:
            var info = InkpodFilterPreviewInfo()
            info.struct_size = UInt32(MemoryLayout<InkpodFilterPreviewInfo>.size)
            return CoreStatus(cValue: inkpod_core_filter_preview_cancel(core, &info))
        case .geometryPreview:
            return CoreStatus(cValue: inkpod_core_geometry_preview_cancel(core))
        case .shootingFramePreview:
            return CoreStatus(cValue: inkpod_core_shooting_frame_preview_cancel(core))
        case .vanishingPointPreview:
            return CoreStatus(cValue: inkpod_core_vanishing_point_preview_cancel(core))
        case .annotationStroke:
            return CoreStatus(cValue: inkpod_core_annotation_stroke_cancel(core))
        case .motion:
            return CoreStatus(cValue: inkpod_core_motion_check_stop(core))
        }
    }

    private func withCutMetadata<Result>(
        _ metadata: CoreCutMetadata,
        _ body: (InkpodCutMetadataInput) -> Result
    ) -> Result {
        let workTitle = Array(metadata.workTitle.utf8)
        let episode = Array(metadata.episode.utf8)
        let scene = Array(metadata.scene.utf8)
        let cutName = Array(metadata.cutName.utf8)
        let instruction = Array(metadata.instruction.utf8)
        return workTitle.withUnsafeBufferPointer { workBuffer in
            episode.withUnsafeBufferPointer { episodeBuffer in
                scene.withUnsafeBufferPointer { sceneBuffer in
                    cutName.withUnsafeBufferPointer { nameBuffer in
                        instruction.withUnsafeBufferPointer { instructionBuffer in
                            var input = InkpodCutMetadataInput()
                            input.struct_size = UInt32(MemoryLayout<InkpodCutMetadataInput>.size)
                            input.duration_frames = metadata.durationFrames
                            input.work_title = InkpodUtf8Span(
                                bytes: workBuffer.baseAddress,
                                byte_count: UInt64(workBuffer.count)
                            )
                            input.episode = InkpodUtf8Span(
                                bytes: episodeBuffer.baseAddress,
                                byte_count: UInt64(episodeBuffer.count)
                            )
                            input.scene = InkpodUtf8Span(
                                bytes: sceneBuffer.baseAddress,
                                byte_count: UInt64(sceneBuffer.count)
                            )
                            input.cut_name = InkpodUtf8Span(
                                bytes: nameBuffer.baseAddress,
                                byte_count: UInt64(nameBuffer.count)
                            )
                            input.instruction = InkpodUtf8Span(
                                bytes: instructionBuffer.baseAddress,
                                byte_count: UInt64(instructionBuffer.count)
                            )
                            return body(input)
                        }
                    }
                }
            }
        }
    }

    private func ffiCutDefaults(_ defaults: CoreCutDefaults) -> InkpodCutDefaultsInput {
        var input = InkpodCutDefaultsInput()
        input.struct_size = UInt32(MemoryLayout<InkpodCutDefaultsInput>.size)
        input.sizing_mode = defaults.sizingMode
        input.width = defaults.width
        input.height = defaults.height
        input.dpi_x_milli = defaults.dpiXMilli
        input.dpi_y_milli = defaults.dpiYMilli
        input.margin_milli = defaults.marginMilli
        input.safe_frame_ratio_milli = defaults.safeFrameRatioMilli
        input.maximum_close_ratio_milli = defaults.maximumCloseRatioMilli
        input.anchor = defaults.anchor
        input.initial_layer_kind = defaults.initialLayerKind.rawValue
        input.pixel_format = defaults.pixelFormat.rawValue
        return input
    }

    private func withCutMembers<Result>(
        _ members: [CoreCutMember],
        _ body: (UnsafePointer<InkpodCutMemberInput>?, UInt64) -> Result
    ) -> Result {
        var records: [InkpodCutMemberInput] = []
        records.reserveCapacity(members.count)
        func append(_ index: Int) -> Result {
            guard index < members.count else {
                return records.withUnsafeBufferPointer { buffer in
                    body(buffer.baseAddress, UInt64(buffer.count))
                }
            }
            let member = members[index]
            let path = Array(member.relativePath.utf8)
            return path.withUnsafeBufferPointer { pathBuffer in
                var record = InkpodCutMemberInput()
                record.struct_size = UInt32(MemoryLayout<InkpodCutMemberInput>.size)
                record.display_number = member.displayNumber
                record.cell_id = member.cellID
                record.document_uuid_high = member.documentUUID.high
                record.document_uuid_low = member.documentUUID.low
                record.relative_path = InkpodUtf8Span(
                    bytes: pathBuffer.baseAddress,
                    byte_count: UInt64(pathBuffer.count)
                )
                records.append(record)
                defer { records.removeLast() }
                return append(index + 1)
            }
        }
        return append(0)
    }

    private func cutOperationIsValid(_ operation: CoreCutSequenceOperation) -> Bool {
        switch operation {
        case let .insert(member, position):
            member.isValid && position <= 10_000
        case let .remove(cellID, documentUUID):
            cellID > 0 && documentUUID.isValid
        case let .moveBefore(cellID, documentUUID, anchorCellID, anchorUUID),
             let .moveAfter(cellID, documentUUID, anchorCellID, anchorUUID):
            cellID > 0 && anchorCellID > 0 && documentUUID.isValid && anchorUUID.isValid
                && (cellID != anchorCellID || documentUUID != anchorUUID)
        case let .renumber(position, count, first, step):
            position <= 10_000 && count > 0 && count <= 10_000 && first > 0 && step > 0
        }
    }

    private func withCutSequenceOperations<Result>(
        _ operations: [CoreCutSequenceOperation],
        _ body: (UnsafePointer<InkpodCutSequenceEditOperation>?, UInt64) -> Result
    ) -> Result {
        var records: [InkpodCutSequenceEditOperation] = []
        records.reserveCapacity(operations.count)
        func append(_ index: Int) -> Result {
            guard index < operations.count else {
                return records.withUnsafeBufferPointer { buffer in
                    body(buffer.baseAddress, UInt64(buffer.count))
                }
            }
            let operation = operations[index]
            let path: [UInt8]
            if case let .insert(member, _) = operation {
                path = Array(member.relativePath.utf8)
            } else {
                path = []
            }
            return path.withUnsafeBufferPointer { pathBuffer in
                var record = InkpodCutSequenceEditOperation()
                record.struct_size = UInt32(MemoryLayout<InkpodCutSequenceEditOperation>.size)
                switch operation {
                case let .insert(member, position):
                    record.kind = 1
                    record.cell_id = member.cellID
                    record.document_uuid_high = member.documentUUID.high
                    record.document_uuid_low = member.documentUUID.low
                    record.position = position
                    record.display_number = member.displayNumber
                    record.relative_path = InkpodUtf8Span(
                        bytes: pathBuffer.baseAddress,
                        byte_count: UInt64(pathBuffer.count)
                    )
                case let .remove(cellID, uuid):
                    record.kind = 2
                    record.cell_id = cellID
                    record.document_uuid_high = uuid.high
                    record.document_uuid_low = uuid.low
                case let .moveBefore(cellID, uuid, anchorID, anchorUUID):
                    record.kind = 3
                    record.cell_id = cellID
                    record.document_uuid_high = uuid.high
                    record.document_uuid_low = uuid.low
                    record.anchor_cell_id = anchorID
                    record.anchor_document_uuid_high = anchorUUID.high
                    record.anchor_document_uuid_low = anchorUUID.low
                case let .moveAfter(cellID, uuid, anchorID, anchorUUID):
                    record.kind = 4
                    record.cell_id = cellID
                    record.document_uuid_high = uuid.high
                    record.document_uuid_low = uuid.low
                    record.anchor_cell_id = anchorID
                    record.anchor_document_uuid_high = anchorUUID.high
                    record.anchor_document_uuid_low = anchorUUID.low
                case let .renumber(position, count, first, step):
                    record.kind = 5
                    record.position = position
                    record.count = count
                    record.first_number = first
                    record.step = step
                }
                records.append(record)
                defer { records.removeLast() }
                return append(index + 1)
            }
        }
        return append(0)
    }

    private func withLiveCut(
        _ target: CoreCutTarget,
        expectedRevision: UInt64,
        _ body: (CoreCutEntry) -> CoreRequestOutcome
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case let .live(entry):
            guard cutProjection(entry)?.revision == expectedRevision else {
                return .failed(.staleTarget)
            }
            return body(entry)
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        }
    }

    private func cutProjection(_ entry: CoreCutEntry) -> CoreCutProjection? {
        var info = InkpodCutInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodCutInfo>.size)
        guard CoreStatus(cValue: inkpod_cut_info(entry.cut, &info)) == .ok,
              info.member_count <= 10_000,
              let metadata = cutMetadata(entry.cut, info: info),
              let layerKind = CoreLayerKind(rawValue: info.initial_layer_kind),
              let pixelFormat = CorePixelStorageFormat(rawValue: info.pixel_format)
        else { return nil }
        var members: [CoreCutMember] = []
        members.reserveCapacity(Int(info.member_count))
        for index in 0 ..< info.member_count {
            guard let member = cutMember(entry.cut, index: index) else { return nil }
            members.append(member)
        }
        return CoreCutProjection(
            target: entry.target,
            cutID: info.cut_id,
            cutUUID: CoreCutUUID(high: info.cut_uuid_high, low: info.cut_uuid_low),
            revision: info.revision,
            stateID: info.state_id,
            metadata: metadata,
            defaults: CoreCutDefaults(
                sizingMode: info.sizing_mode,
                width: info.width,
                height: info.height,
                dpiXMilli: info.dpi_x_milli,
                dpiYMilli: info.dpi_y_milli,
                marginMilli: info.margin_milli,
                safeFrameRatioMilli: info.safe_frame_ratio_milli,
                maximumCloseRatioMilli: info.maximum_close_ratio_milli,
                anchor: info.anchor,
                initialLayerKind: layerKind,
                pixelFormat: pixelFormat
            ),
            members: members,
            isDirty: info.flags & 1 != 0,
            canUndo: info.flags & 2 != 0,
            canRedo: info.flags & 4 != 0,
            isRecovered: info.flags & 8 != 0,
            ownerThreadID: ownerThreadID
        )
    }

    private func cutMetadata(
        _ cut: OpaquePointer,
        info: InkpodCutInfo
    ) -> CoreCutMetadata? {
        let counts = [info.work_title_bytes, info.episode_bytes, info.scene_bytes,
                      info.cut_name_bytes, info.instruction_bytes]
        guard counts.allSatisfy({ $0 <= 4_096 && $0 <= UInt64(Int.max) }) else { return nil }
        var workTitle = [UInt8](repeating: 0, count: Int(info.work_title_bytes))
        var episode = [UInt8](repeating: 0, count: Int(info.episode_bytes))
        var scene = [UInt8](repeating: 0, count: Int(info.scene_bytes))
        var cutName = [UInt8](repeating: 0, count: Int(info.cut_name_bytes))
        var instruction = [UInt8](repeating: 0, count: Int(info.instruction_bytes))
        let status = workTitle.withUnsafeMutableBufferPointer { workBuffer in
            episode.withUnsafeMutableBufferPointer { episodeBuffer in
                scene.withUnsafeMutableBufferPointer { sceneBuffer in
                    cutName.withUnsafeMutableBufferPointer { nameBuffer in
                        instruction.withUnsafeMutableBufferPointer { instructionBuffer in
                            func output(_ buffer: UnsafeMutableBufferPointer<UInt8>)
                                -> InkpodUtf8Buffer
                            {
                                InkpodUtf8Buffer(
                                    bytes: buffer.baseAddress,
                                    capacity: UInt64(buffer.count),
                                    byte_count: 0
                                )
                            }
                            var metadata = InkpodCutMetadataBuffer()
                            metadata.struct_size = UInt32(
                                MemoryLayout<InkpodCutMetadataBuffer>.size
                            )
                            metadata.work_title = output(workBuffer)
                            metadata.episode = output(episodeBuffer)
                            metadata.scene = output(sceneBuffer)
                            metadata.cut_name = output(nameBuffer)
                            metadata.instruction = output(instructionBuffer)
                            return CoreStatus(cValue: inkpod_cut_metadata_copy(cut, &metadata))
                        }
                    }
                }
            }
        }
        guard status == .ok else { return nil }
        let strings = [workTitle, episode, scene, cutName, instruction].map {
            String(bytes: $0, encoding: .utf8)
        }
        guard strings.allSatisfy({ $0 != nil }) else { return nil }
        return CoreCutMetadata(
            workTitle: strings[0]!,
            episode: strings[1]!,
            scene: strings[2]!,
            cutName: strings[3]!,
            instruction: strings[4]!,
            durationFrames: info.duration_frames
        )
    }

    private func cutMember(_ cut: OpaquePointer, index: UInt32) -> CoreCutMember? {
        var query = InkpodCutMemberInfo()
        query.struct_size = UInt32(MemoryLayout<InkpodCutMemberInfo>.size)
        let queryStatus = CoreStatus(cValue: inkpod_cut_member_get(cut, index, &query))
        guard queryStatus == .bufferTooSmall || queryStatus == .ok,
              query.relative_path.byte_count > 0,
              query.relative_path.byte_count <= 4_096,
              query.relative_path.byte_count <= UInt64(Int.max)
        else { return nil }
        var path = [UInt8](repeating: 0, count: Int(query.relative_path.byte_count))
        let status = path.withUnsafeMutableBufferPointer { buffer -> CoreStatus in
            var output = InkpodCutMemberInfo()
            output.struct_size = UInt32(MemoryLayout<InkpodCutMemberInfo>.size)
            output.relative_path = InkpodUtf8Buffer(
                bytes: buffer.baseAddress,
                capacity: UInt64(buffer.count),
                byte_count: 0
            )
            return CoreStatus(cValue: inkpod_cut_member_get(cut, index, &output))
        }
        guard status == .ok, let relativePath = String(bytes: path, encoding: .utf8) else {
            return nil
        }
        // Scalar values are identical between size query and copy; query retains them.
        return CoreCutMember(
            displayNumber: query.display_number,
            cellID: query.cell_id,
            documentUUID: CoreDocumentUUID(
                high: query.document_uuid_high,
                low: query.document_uuid_low
            ),
            relativePath: relativePath
        )
    }

    private func withSequenceSources<Result>(
        _ sources: [CoreRGBA8Source],
        _ body: (UnsafePointer<InkpodSequenceCellInput>?, UInt64) -> Result
    ) -> Result {
        var records: [InkpodSequenceCellInput] = []
        records.reserveCapacity(sources.count)
        func append(_ index: Int) -> Result {
            guard index < sources.count else {
                return records.withUnsafeBufferPointer { buffer in
                    body(buffer.baseAddress, UInt64(buffer.count))
                }
            }
            let source = sources[index]
            let name = Array(source.name.utf8)
            return name.withUnsafeBufferPointer { nameBuffer in
                source.rgba8.withUnsafeBufferPointer { pixelBuffer in
                    var raster = InkpodRasterSourceInput()
                    raster.struct_size = UInt32(MemoryLayout<InkpodRasterSourceInput>.size)
                    raster.pixel_format = CorePixelStorageFormat.rgba8.rawValue
                    raster.document_uuid_high = source.documentUUID.high
                    raster.document_uuid_low = source.documentUUID.low
                    raster.source_revision = source.sourceGeneration
                    raster.width = source.width
                    raster.height = source.height
                    raster.dpi_x_milli = source.dpiXMilli
                    raster.dpi_y_milli = source.dpiYMilli
                    raster.reference_frame = InkpodFrameRect(
                        x: 0,
                        y: 0,
                        width: Int32(source.width),
                        height: Int32(source.height)
                    )
                    raster.pixels = pixelBuffer.baseAddress
                    raster.pixel_bytes = UInt64(pixelBuffer.count)
                    raster.row_stride_bytes = UInt64(source.width) * 4
                    var record = InkpodSequenceCellInput()
                    record.struct_size = UInt32(MemoryLayout<InkpodSequenceCellInput>.size)
                    record.name_utf8 = nameBuffer.baseAddress
                    record.name_bytes = UInt64(nameBuffer.count)
                    record.source = raster
                    records.append(record)
                    defer { records.removeLast() }
                    return append(index + 1)
                }
            }
        }
        return append(0)
    }

    private func withNamedRasters<Result>(
        _ files: [CoreNamedRaster],
        _ body: (UnsafePointer<InkpodNamedRasterInput>?, UInt64) -> Result
    ) -> Result {
        var records: [InkpodNamedRasterInput] = []
        records.reserveCapacity(files.count)
        func append(_ index: Int) -> Result {
            guard index < files.count else {
                return records.withUnsafeBufferPointer { buffer in
                    body(buffer.baseAddress, UInt64(buffer.count))
                }
            }
            let file = files[index]
            let name = Array(file.name.utf8)
            return name.withUnsafeBufferPointer { nameBuffer in
                file.bytes.withUnsafeBufferPointer { byteBuffer in
                    var record = InkpodNamedRasterInput()
                    record.struct_size = UInt32(MemoryLayout<InkpodNamedRasterInput>.size)
                    record.format = ffiRasterFormat(file.format)
                    record.name_utf8 = nameBuffer.baseAddress
                    record.name_bytes = UInt64(nameBuffer.count)
                    record.bytes = byteBuffer.baseAddress
                    record.byte_count = UInt64(byteBuffer.count)
                    records.append(record)
                    defer { records.removeLast() }
                    return append(index + 1)
                }
            }
        }
        return append(0)
    }

    private func withIdentifiedNamedRasters<Result>(
        _ files: [CoreIdentifiedNamedRaster],
        _ body: (
            UnsafePointer<InkpodNamedRasterInput>?,
            UnsafePointer<InkpodSequenceSourceIdentity>?,
            UInt64
        ) -> Result
    ) -> Result {
        var records: [InkpodNamedRasterInput] = []
        var identities: [InkpodSequenceSourceIdentity] = []
        records.reserveCapacity(files.count)
        identities.reserveCapacity(files.count)
        func append(_ index: Int) -> Result {
            guard index < files.count else {
                return records.withUnsafeBufferPointer { recordBuffer in
                    identities.withUnsafeBufferPointer { identityBuffer in
                        body(
                            recordBuffer.baseAddress,
                            identityBuffer.baseAddress,
                            UInt64(recordBuffer.count)
                        )
                    }
                }
            }
            let file = files[index]
            let name = Array(file.raster.name.utf8)
            return name.withUnsafeBufferPointer { nameBuffer in
                file.raster.bytes.withUnsafeBufferPointer { byteBuffer in
                    var record = InkpodNamedRasterInput()
                    record.struct_size = UInt32(MemoryLayout<InkpodNamedRasterInput>.size)
                    record.format = ffiRasterFormat(file.raster.format)
                    record.name_utf8 = nameBuffer.baseAddress
                    record.name_bytes = UInt64(nameBuffer.count)
                    record.bytes = byteBuffer.baseAddress
                    record.byte_count = UInt64(byteBuffer.count)
                    var identity = InkpodSequenceSourceIdentity()
                    identity.struct_size = UInt32(
                        MemoryLayout<InkpodSequenceSourceIdentity>.size
                    )
                    identity.document_uuid_high = file.documentUUID.high
                    identity.document_uuid_low = file.documentUUID.low
                    identity.source_generation = file.sourceGeneration
                    records.append(record)
                    identities.append(identity)
                    defer {
                        records.removeLast()
                        identities.removeLast()
                    }
                    return append(index + 1)
                }
            }
        }
        return append(0)
    }

    private func sequenceCell(core: OpaquePointer, index: UInt32) -> CoreSequenceCellProjection? {
        var query = InkpodSequenceCellInfo()
        query.struct_size = UInt32(MemoryLayout<InkpodSequenceCellInfo>.size)
        let queryStatus = CoreStatus(cValue: inkpod_core_sequence_cell_get(core, index, &query))
        guard queryStatus == .bufferTooSmall || queryStatus == .ok,
              query.name_bytes <= 4_096, query.name_bytes <= UInt64(Int.max)
        else { return nil }
        var nameBytes = [UInt8](repeating: 0, count: Int(query.name_bytes))
        var output = InkpodSequenceCellInfo()
        output.struct_size = UInt32(MemoryLayout<InkpodSequenceCellInfo>.size)
        let status = nameBytes.withUnsafeMutableBufferPointer { buffer -> CoreStatus in
            output.name_utf8 = buffer.baseAddress
            output.name_capacity = UInt64(buffer.count)
            return CoreStatus(cValue: inkpod_core_sequence_cell_get(core, index, &output))
        }
        guard status == .ok, let name = String(bytes: nameBytes, encoding: .utf8),
              let thumbnail = sequenceThumbnail(core: core, index: index)
        else { return nil }
        var identity = InkpodSequenceSourceIdentity()
        identity.struct_size = UInt32(MemoryLayout<InkpodSequenceSourceIdentity>.size)
        guard CoreStatus(cValue: inkpod_core_sequence_source_identity(
            core,
            index,
            &identity
        )) == .ok,
            identity.document_uuid_high == output.document_uuid_high,
            identity.document_uuid_low == output.document_uuid_low,
            identity.source_generation > 0
        else { return nil }
        return CoreSequenceCellProjection(
            index: index,
            documentUUID: CoreDocumentUUID(
                high: output.document_uuid_high,
                low: output.document_uuid_low
            ),
            sourceGeneration: identity.source_generation,
            cellNumber: output.cell_number,
            name: name,
            width: output.width,
            height: output.height,
            thumbnailWidth: thumbnail.width,
            thumbnailHeight: thumbnail.height,
            thumbnailChecksum: thumbnail.checksum,
            thumbnailRGBA8: thumbnail.bytes
        )
    }

    private func sequenceThumbnail(
        core: OpaquePointer,
        index: UInt32
    ) -> (width: UInt32, height: UInt32, checksum: UInt64, bytes: [UInt8])? {
        var query = InkpodSequenceThumbnailBuffer()
        query.struct_size = UInt32(MemoryLayout<InkpodSequenceThumbnailBuffer>.size)
        let queryStatus = CoreStatus(cValue: inkpod_core_sequence_thumbnail_get(
            core,
            index,
            &query
        ))
        guard queryStatus == .bufferTooSmall || queryStatus == .ok,
              query.required_bytes <= 64 * 1_024 * 1_024,
              query.required_bytes <= UInt64(Int.max)
        else { return nil }
        var bytes = [UInt8](repeating: 0, count: Int(query.required_bytes))
        var output = InkpodSequenceThumbnailBuffer()
        output.struct_size = UInt32(MemoryLayout<InkpodSequenceThumbnailBuffer>.size)
        let status = bytes.withUnsafeMutableBufferPointer { buffer -> CoreStatus in
            output.pixels_rgba8 = buffer.baseAddress
            output.pixel_capacity = UInt64(buffer.count)
            return CoreStatus(cValue: inkpod_core_sequence_thumbnail_get(core, index, &output))
        }
        guard status == .ok, output.required_bytes == UInt64(bytes.count) else { return nil }
        return (output.width, output.height, output.checksum, bytes)
    }

    private func animationProjection(for entry: CoreSessionEntry) -> CoreAnimationProjection? {
        guard let session = projection(for: entry) else { return nil }
        var sequence: [CoreSequenceCellProjection] = []
        for index in UInt32(0) ..< UInt32(10_000) {
            var query = InkpodSequenceCellInfo()
            query.struct_size = UInt32(MemoryLayout<InkpodSequenceCellInfo>.size)
            let status = CoreStatus(cValue: inkpod_core_sequence_cell_get(entry.core, index, &query))
            if status == .invalidArgument || status == .invalidState { break }
            guard status == .ok || status == .bufferTooSmall,
                  let cell = sequenceCell(core: entry.core, index: index)
            else { return nil }
            sequence.append(cell)
        }
        let activeSequenceIndex = sequence.first {
            $0.documentUUID == session.documentUUID
        }?.index
        guard let lightTableSets = lightTableSets(core: entry.core) else { return nil }
        return CoreAnimationProjection(
            session: session,
            sequence: sequence,
            activeSequenceIndex: activeSequenceIndex,
            lightTableSets: lightTableSets,
            motion: entry.motion
        )
    }

    private func lightTableSets(core: OpaquePointer) -> [CoreLightTableSetProjection]? {
        var sets: [(id: UInt64, name: String, opacity: UInt32, active: Bool)] = []
        for index in UInt32(0) ..< UInt32(1_024) {
            var query = InkpodLightTableSetInfo()
            query.struct_size = UInt32(MemoryLayout<InkpodLightTableSetInfo>.size)
            let queryStatus = CoreStatus(cValue: inkpod_core_light_table_set_get(
                core,
                index,
                &query
            ))
            if queryStatus == .invalidArgument { break }
            guard queryStatus == .ok || queryStatus == .bufferTooSmall,
                  query.name_bytes <= 4_096, query.name_bytes <= UInt64(Int.max)
            else { return nil }
            var nameBytes = [UInt8](repeating: 0, count: Int(query.name_bytes))
            var output = InkpodLightTableSetInfo()
            output.struct_size = UInt32(MemoryLayout<InkpodLightTableSetInfo>.size)
            let status = nameBytes.withUnsafeMutableBufferPointer { buffer -> CoreStatus in
                output.name_utf8 = buffer.baseAddress
                output.name_capacity = UInt64(buffer.count)
                return CoreStatus(cValue: inkpod_core_light_table_set_get(core, index, &output))
            }
            guard status == .ok, let name = String(bytes: nameBytes, encoding: .utf8) else {
                return nil
            }
            sets.append((output.id, name, output.opacity_milli, output.flags & 2 != 0))
        }
        var result: [CoreLightTableSetProjection] = []
        result.reserveCapacity(sets.count)
        for set in sets {
            let items: [CoreLightTableItemProjection]
            if set.active {
                guard let activeItems = lightTableItems(core: core) else { return nil }
                items = activeItems
            } else {
                items = []
            }
            result.append(CoreLightTableSetProjection(
                id: set.id,
                name: set.name,
                opacityMilli: set.opacity,
                isActive: set.active,
                items: items
            ))
        }
        return result
    }

    private func lightTableItems(core: OpaquePointer) -> [CoreLightTableItemProjection]? {
        var items: [CoreLightTableItemProjection] = []
        for index in UInt32(0) ..< UInt32(10_000) {
            var query = InkpodLightTableItemInfo()
            query.struct_size = UInt32(MemoryLayout<InkpodLightTableItemInfo>.size)
            let queryStatus = CoreStatus(cValue: inkpod_core_light_table_item_get(
                core,
                index,
                &query
            ))
            if queryStatus == .invalidArgument { break }
            guard queryStatus == .ok || queryStatus == .bufferTooSmall,
                  query.name_bytes <= 4_096, query.name_bytes <= UInt64(Int.max)
            else { return nil }
            var nameBytes = [UInt8](repeating: 0, count: Int(query.name_bytes))
            var output = InkpodLightTableItemInfo()
            output.struct_size = UInt32(MemoryLayout<InkpodLightTableItemInfo>.size)
            let status = nameBytes.withUnsafeMutableBufferPointer { buffer -> CoreStatus in
                output.name_utf8 = buffer.baseAddress
                output.name_capacity = UInt64(buffer.count)
                return CoreStatus(cValue: inkpod_core_light_table_item_get(core, index, &output))
            }
            guard status == .ok,
                  let name = String(bytes: nameBytes, encoding: .utf8),
                  let mode = CoreLightTableDisplayMode(rawValue: output.display_mode),
                  let color = coreColor(output.display_color)
            else { return nil }
            items.append(CoreLightTableItemProjection(
                id: output.id,
                name: name,
                sourcePlaneID: output.source_plane_id,
                sourceDocumentUUID: CoreDocumentUUID(
                    high: output.source_document_uuid_high,
                    low: output.source_document_uuid_low
                ),
                sourceRevision: output.source_revision,
                opacityMilli: output.opacity_milli,
                effectiveOpacityMilli: output.effective_opacity_milli,
                displayMode: mode,
                displayColor: color,
                translateXMilli: output.translate_x_milli,
                translateYMilli: output.translate_y_milli,
                scaleXMilli: output.scale_x_milli,
                scaleYMilli: output.scale_y_milli,
                rotationMilliDegrees: output.rotation_milli_degrees,
                isVisible: output.flags & 1 != 0
            ))
        }
        return items
    }

    private func withLightTableItem<Result>(
        _ input: CoreLightTableItemSource,
        _ body: (InkpodLightTableItemInput) -> Result
    ) -> Result {
        let source = input.source
        let name = Array(source.name.utf8)
        return name.withUnsafeBufferPointer { nameBuffer in
            source.rgba8.withUnsafeBufferPointer { pixelBuffer in
                var raster = InkpodRasterSourceInput()
                raster.struct_size = UInt32(MemoryLayout<InkpodRasterSourceInput>.size)
                raster.pixel_format = CorePixelStorageFormat.rgba8.rawValue
                raster.document_uuid_high = source.documentUUID.high
                raster.document_uuid_low = source.documentUUID.low
                raster.source_revision = source.sourceGeneration
                raster.width = source.width
                raster.height = source.height
                raster.dpi_x_milli = source.dpiXMilli
                raster.dpi_y_milli = source.dpiYMilli
                raster.reference_frame = InkpodFrameRect(
                    x: 0,
                    y: 0,
                    width: Int32(source.width),
                    height: Int32(source.height)
                )
                raster.pixels = pixelBuffer.baseAddress
                raster.pixel_bytes = UInt64(pixelBuffer.count)
                raster.row_stride_bytes = UInt64(source.width) * 4
                var ffiInput = InkpodLightTableItemInput()
                ffiInput.struct_size = UInt32(MemoryLayout<InkpodLightTableItemInput>.size)
                ffiInput.flags = input.isVisible ? 1 : 0
                ffiInput.opacity_milli = input.opacityMilli
                ffiInput.display_mode = input.displayMode.rawValue
                ffiInput.display_color = ffiColor(input.displayColor)
                ffiInput.scale_x_milli = 1_000
                ffiInput.scale_y_milli = 1_000
                ffiInput.name_utf8 = nameBuffer.baseAddress
                ffiInput.name_bytes = UInt64(nameBuffer.count)
                ffiInput.source = raster
                return body(ffiInput)
            }
        }
    }

    private func editLightTable(
        entry: CoreSessionEntry,
        command: CoreLightTableEditCommand
    ) -> CoreRequestOutcome {
        if case let .setGlobalOpacity(opacity) = command {
            guard opacity <= 1_000 else { return .failed(.invalidRequest) }
            var result = dispatchResult()
            let status = CoreStatus(cValue: inkpod_core_light_table_set_global_opacity(
                entry.core,
                opacity,
                &result
            ))
            return animationMutationOutcome(
                status,
                entry: entry,
                createdIDs: [],
                applied: result.accepted_command_count > 0
            )
        }
        var edit = InkpodLightTableEdit()
        edit.struct_size = UInt32(MemoryLayout<InkpodLightTableEdit>.size)
        let name: String
        switch command {
        case let .createSet(value):
            edit.operation = 1
            name = value
        case let .duplicateSet(id, value):
            edit.operation = 2
            edit.object_id = id
            name = value
        case let .deleteSet(id):
            edit.operation = 3
            edit.object_id = id
            name = ""
        case let .renameSet(id, value):
            edit.operation = 4
            edit.object_id = id
            name = value
        case let .reorderSet(id, destination):
            edit.operation = 5
            edit.object_id = id
            edit.destination_index = destination
            name = ""
        case let .activateSet(id):
            edit.operation = 6
            edit.object_id = id
            name = ""
        case let .removeItem(id):
            edit.operation = 7
            edit.object_id = id
            name = ""
        case let .reorderItem(id, destination):
            edit.operation = 8
            edit.object_id = id
            edit.destination_index = destination
            name = ""
        case let .updateItem(
            id,
            value,
            opacity,
            mode,
            color,
            visible,
            translateX,
            translateY,
            scaleX,
            scaleY,
            rotation
        ):
            guard opacity <= 1_000, scaleX > 0, scaleY > 0 else {
                return .failed(.invalidRequest)
            }
            edit.operation = 9
            edit.object_id = id
            edit.flags = visible ? 1 : 0
            edit.opacity_milli = opacity
            edit.display_mode = mode.rawValue
            edit.display_color = ffiColor(color)
            edit.translate_x_milli = translateX
            edit.translate_y_milli = translateY
            edit.scale_x_milli = scaleX
            edit.scale_y_milli = scaleY
            edit.rotation_milli_degrees = rotation
            name = value
        case .setGlobalOpacity:
            preconditionFailure("handled above")
        }
        guard edit.operation > 0, name.utf8.count <= 4_096,
              !name.utf8.contains(0), edit.object_id > 0 || edit.operation == 1,
              ![1, 2, 4, 9].contains(edit.operation) || !name.isEmpty
        else { return .failed(.invalidRequest) }
        let nameBytes = Array(name.utf8)
        var result = dispatchResult()
        var objectID: UInt64 = 0
        let status = nameBytes.withUnsafeBufferPointer { buffer -> CoreStatus in
            edit.name_utf8 = buffer.baseAddress
            edit.name_bytes = UInt64(buffer.count)
            return CoreStatus(cValue: inkpod_core_light_table_edit(
                entry.core,
                &edit,
                &result,
                &objectID
            ))
        }
        return animationMutationOutcome(
            status,
            entry: entry,
            createdIDs: objectID == 0 ? [] : [objectID],
            applied: result.accepted_command_count > 0
        )
    }

    private func previewLightTableBulk(
        entry: CoreSessionEntry,
        setID: UInt64,
        direction: CoreLightTableBulkDirection,
        neighborCount: UInt32,
        baseOpacityMilli: UInt32,
        distanceStepMilli: UInt32
    ) -> CoreRequestOutcome {
        var request = InkpodLightTableBulkRequest()
        request.struct_size = UInt32(MemoryLayout<InkpodLightTableBulkRequest>.size)
        let requestStatus = CoreStatus(cValue: inkpod_core_light_table_bulk_request(
            entry.core,
            setID,
            direction.rawValue,
            neighborCount,
            baseOpacityMilli,
            distanceStepMilli,
            &request
        ))
        guard requestStatus == .ok, let token = lightTableBulkRequest(request) else {
            return animationFailure(requestStatus)
        }
        var info = InkpodLightTableBulkPreviewInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodLightTableBulkPreviewInfo>.size)
        let queryStatus = CoreStatus(cValue: inkpod_core_light_table_bulk_preview(
            entry.core,
            &request,
            nil,
            0,
            0,
            &info
        ))
        guard queryStatus == .ok, info.entry_count <= 10_000,
              info.entry_count <= UInt64(Int.max)
        else { return animationFailure(queryStatus) }
        if info.entry_count == 0 {
            return .lightTableBulkPreview(CoreLightTableBulkPreview(
                request: token,
                entries: [],
                addCount: info.add_count,
                skipCount: info.skip_count
            ))
        }
        var entries = [InkpodLightTableBulkPreviewEntry](
            repeating: InkpodLightTableBulkPreviewEntry(),
            count: Int(info.entry_count)
        )
        for index in entries.indices {
            entries[index].struct_size = UInt32(
                MemoryLayout<InkpodLightTableBulkPreviewEntry>.size
            )
        }
        var copied = InkpodLightTableBulkPreviewInfo()
        copied.struct_size = UInt32(MemoryLayout<InkpodLightTableBulkPreviewInfo>.size)
        let copyStatus = entries.withUnsafeMutableBufferPointer { buffer in
            CoreStatus(cValue: inkpod_core_light_table_bulk_preview(
                entry.core,
                &request,
                buffer.baseAddress,
                UInt64(buffer.count),
                UInt64(MemoryLayout<InkpodLightTableBulkPreviewEntry>.stride),
                &copied
            ))
        }
        guard copyStatus == .ok else { return animationFailure(copyStatus) }
        var projectionEntries: [CoreLightTableBulkEntry] = []
        projectionEntries.reserveCapacity(entries.count)
        for item in entries {
            guard let action = CoreLightTableBulkAction(rawValue: item.action) else {
                return .failed(.coreOperation(.panic))
            }
            projectionEntries.append(CoreLightTableBulkEntry(
                action: action,
                sequenceIndex: item.sequence_index,
                cellNumber: item.cell_number,
                distance: item.distance,
                opacityMilli: item.opacity_milli,
                documentUUID: CoreDocumentUUID(
                    high: item.document_uuid_high,
                    low: item.document_uuid_low
                ),
                sourceGeneration: item.source_generation,
                existingSourceRevision: item.flags & 1 != 0
                    ? item.existing_source_revision : nil
            ))
        }
        return .lightTableBulkPreview(CoreLightTableBulkPreview(
            request: token,
            entries: projectionEntries,
            addCount: copied.add_count,
            skipCount: copied.skip_count
        ))
    }

    private func registerLightTableBulk(
        entry: CoreSessionEntry,
        request: CoreLightTableBulkRequest
    ) -> CoreRequestOutcome {
        guard request.targetSetID > 0, request.neighborCount <= 10_000,
              request.baseOpacityMilli <= 1_000, request.distanceStepMilli <= 1_000,
              request.activeDocumentUUID.isValid, request.activeSourceGeneration > 0
        else { return .failed(.invalidRequest) }
        var ffiRequest = ffiLightTableBulkRequest(request)
        var previewInfo = InkpodLightTableBulkPreviewInfo()
        previewInfo.struct_size = UInt32(MemoryLayout<InkpodLightTableBulkPreviewInfo>.size)
        let queryStatus = CoreStatus(cValue: inkpod_core_light_table_bulk_preview(
            entry.core,
            &ffiRequest,
            nil,
            0,
            0,
            &previewInfo
        ))
        guard queryStatus == .ok, previewInfo.add_count <= 10_000 else {
            return animationFailure(queryStatus, staleInvalidState: true)
        }
        var ids = [UInt64](repeating: 0, count: Int(previewInfo.add_count))
        var result = dispatchResult()
        var summary = InkpodLightTableBulkSummary()
        summary.struct_size = UInt32(MemoryLayout<InkpodLightTableBulkSummary>.size)
        let status = ids.withUnsafeMutableBufferPointer { buffer in
            CoreStatus(cValue: inkpod_core_light_table_bulk_register(
                entry.core,
                &ffiRequest,
                &result,
                &summary,
                buffer.baseAddress,
                UInt64(buffer.count)
            ))
        }
        return animationMutationOutcome(
            status,
            entry: entry,
            createdIDs: ids,
            applied: result.accepted_command_count > 0,
            staleInvalidState: true
        )
    }

    private func lightTableBulkRequest(
        _ input: InkpodLightTableBulkRequest
    ) -> CoreLightTableBulkRequest? {
        guard let direction = CoreLightTableBulkDirection(rawValue: input.direction) else {
            return nil
        }
        return CoreLightTableBulkRequest(
            targetSetID: input.target_set_id,
            direction: direction,
            neighborCount: input.neighbor_count,
            baseOpacityMilli: input.base_opacity_milli,
            distanceStepMilli: input.distance_step_milli,
            baseDocumentRevision: input.base_document_revision,
            sequenceRevision: input.sequence_revision,
            activeDocumentUUID: CoreDocumentUUID(
                high: input.active_document_uuid_high,
                low: input.active_document_uuid_low
            ),
            activeSourceGeneration: input.active_source_generation
        )
    }

    private func ffiLightTableBulkRequest(
        _ input: CoreLightTableBulkRequest
    ) -> InkpodLightTableBulkRequest {
        var request = InkpodLightTableBulkRequest()
        request.struct_size = UInt32(MemoryLayout<InkpodLightTableBulkRequest>.size)
        request.direction = input.direction.rawValue
        request.target_set_id = input.targetSetID
        request.neighbor_count = input.neighborCount
        request.base_opacity_milli = input.baseOpacityMilli
        request.distance_step_milli = input.distanceStepMilli
        request.base_document_revision = input.baseDocumentRevision
        request.sequence_revision = input.sequenceRevision
        request.active_document_uuid_high = input.activeDocumentUUID.high
        request.active_document_uuid_low = input.activeDocumentUUID.low
        request.active_source_generation = input.activeSourceGeneration
        return request
    }

    private func sequenceStepPlan(_ input: InkpodSequenceStepPlan) -> CoreSequenceStepPlan? {
        guard let direction = CoreSequenceDirection(rawValue: input.direction),
              let policy = CoreSequenceEndpointPolicy(rawValue: input.endpoint_policy),
              let result = CoreSequenceStepResult(rawValue: input.result_class)
        else { return nil }
        return CoreSequenceStepPlan(
            direction: direction,
            endpointPolicy: policy,
            result: result,
            sequenceRevision: input.sequence_revision,
            sourceDocumentUUID: input.source_document_uuid_high == 0
                && input.source_document_uuid_low == 0
                ? nil : CoreDocumentUUID(
                    high: input.source_document_uuid_high,
                    low: input.source_document_uuid_low
                ),
            sourceGeneration: input.source_generation,
            targetDocumentUUID: input.target_document_uuid_high == 0
                && input.target_document_uuid_low == 0
                ? nil : CoreDocumentUUID(
                    high: input.target_document_uuid_high,
                    low: input.target_document_uuid_low
                ),
            targetGeneration: input.target_generation,
            sourceIndex: input.source_index == UInt32.max ? nil : input.source_index,
            targetIndex: input.target_index == UInt32.max ? nil : input.target_index,
            sourceCellNumber: input.source_cell_number,
            targetCellNumber: input.target_cell_number
        )
    }

    private func ffiSequenceStepPlan(_ input: CoreSequenceStepPlan) -> InkpodSequenceStepPlan {
        var plan = InkpodSequenceStepPlan()
        plan.struct_size = UInt32(MemoryLayout<InkpodSequenceStepPlan>.size)
        plan.direction = input.direction.rawValue
        plan.endpoint_policy = input.endpointPolicy.rawValue
        plan.result_class = input.result.rawValue
        plan.sequence_revision = input.sequenceRevision
        plan.source_document_uuid_high = input.sourceDocumentUUID?.high ?? 0
        plan.source_document_uuid_low = input.sourceDocumentUUID?.low ?? 0
        plan.source_generation = input.sourceGeneration
        plan.target_document_uuid_high = input.targetDocumentUUID?.high ?? 0
        plan.target_document_uuid_low = input.targetDocumentUUID?.low ?? 0
        plan.target_generation = input.targetGeneration
        plan.source_index = input.sourceIndex ?? UInt32.max
        plan.target_index = input.targetIndex ?? UInt32.max
        plan.source_cell_number = input.sourceCellNumber
        plan.target_cell_number = input.targetCellNumber
        return plan
    }

    private func sequenceTargetIsAvailable(entry: CoreSessionEntry, index: UInt32) -> Bool {
        guard let cell = sequenceCell(core: entry.core, index: index) else { return false }
        return sessionByDocumentUUID[cell.documentUUID].map { $0 == entry.target.id } ?? true
    }

    private func animationFailure(
        _ status: CoreStatus,
        staleInvalidState: Bool = false
    ) -> CoreRequestOutcome {
        if status == .invalidArgument { return .failed(.invalidRequest) }
        if staleInvalidState, status == .invalidState { return .failed(.staleTarget) }
        return .failed(.coreOperation(status == .ok ? .panic : status))
    }

    private func animationQueryOutcome(
        _ status: CoreStatus,
        entry: CoreSessionEntry
    ) -> CoreRequestOutcome {
        guard status == .ok else { return animationFailure(status) }
        return animationProjection(for: entry).map(CoreRequestOutcome.animation)
            ?? .failed(.coreOperation(.panic))
    }

    private func animationMutationOutcome(
        _ status: CoreStatus,
        entry: CoreSessionEntry,
        createdIDs: [UInt64],
        applied: Bool,
        staleInvalidState: Bool = false
    ) -> CoreRequestOutcome {
        guard status == .ok else {
            return animationFailure(status, staleInvalidState: staleInvalidState)
        }
        guard let state = animationProjection(for: entry) else {
            return .failed(.coreOperation(.panic))
        }
        return .animationMutation(CoreAnimationMutationProjection(
            state: state,
            createdIDs: createdIDs,
            applied: applied
        ))
    }

    private func animationDocumentSwitchOutcome(
        _ status: CoreStatus,
        entry: CoreSessionEntry,
        staleInvalidState: Bool = false
    ) -> CoreRequestOutcome {
        guard status == .ok else {
            return animationFailure(status, staleInvalidState: staleInvalidState)
        }
        guard let projected = projection(for: entry) else {
            return .failed(.coreOperation(.panic))
        }
        if let existing = sessionByDocumentUUID[projected.documentUUID], existing != entry.target.id {
            return .failed(.staleTarget)
        }
        var updated = entry
        sessionByDocumentUUID.removeValue(forKey: entry.documentUUID)
        updated.documentUUID = projected.documentUUID
        updated.activeTransient = nil
        updated.motion = nil
        sessions[entry.target.id] = updated
        sessionByDocumentUUID[updated.documentUUID] = updated.target.id
        return animationProjection(for: updated).map(CoreRequestOutcome.animation)
            ?? .failed(.coreOperation(.panic))
    }

    private func motionProjection(_ frame: InkpodMotionFrame) -> CoreMotionProjection {
        CoreMotionProjection(
            sequenceIndex: frame.sequence_index,
            cellNumber: frame.cell_number,
            thumbnailWidth: frame.thumbnail_width,
            thumbnailHeight: frame.thumbnail_height,
            thumbnailChecksum: frame.thumbnail_checksum,
            isPaused: frame.flags & 1 != 0,
            includesSelection: frame.flags & 2 != 0,
            includesLightTable: frame.flags & 4 != 0
        )
    }

    private func motionOutcome(
        _ status: CoreStatus,
        entry: CoreSessionEntry,
        frame: InkpodMotionFrame,
        started: Bool
    ) -> CoreRequestOutcome {
        guard status == .ok else { return animationFailure(status) }
        var updated = entry
        let projection = motionProjection(frame)
        if started { updated.activeTransient = .motion }
        updated.motion = projection
        sessions[entry.target.id] = updated
        return .motion(projection)
    }

    private func exportSequence(
        entry: CoreSessionEntry,
        format: CoreCommonRasterFormat,
        compositeWhite: Bool
    ) -> CoreRequestOutcome {
        var sequence: OpaquePointer?
        let status = CoreStatus(cValue: inkpod_core_sequence_export_encoded(
            entry.core,
            ffiRasterFormat(format),
            compositeWhite ? 1 : 0,
            &sequence
        ))
        guard status == .ok, let rawSequence = sequence else {
            if sequence != nil { _ = inkpod_encoded_sequence_release(&sequence) }
            return animationFailure(status)
        }
        var count: UInt64 = 0
        let countStatus = CoreStatus(cValue: inkpod_encoded_sequence_count(rawSequence, &count))
        guard countStatus == .ok, count <= 10_000, count <= UInt64(Int.max) else {
            _ = inkpod_encoded_sequence_release(&sequence)
            return animationFailure(countStatus == .ok ? .invalidArgument : countStatus)
        }
        var output: [CoreSequenceExportItem] = []
        output.reserveCapacity(Int(count))
        for index in 0 ..< count {
            var namePointer: UnsafePointer<UInt8>?
            var nameCount: UInt64 = 0
            var bytePointer: UnsafePointer<UInt8>?
            var byteCount: UInt64 = 0
            let getStatus = CoreStatus(cValue: inkpod_encoded_sequence_get(
                rawSequence,
                index,
                &namePointer,
                &nameCount,
                &bytePointer,
                &byteCount
            ))
            guard getStatus == .ok, nameCount <= 4_096,
                  byteCount <= 512 * 1_024 * 1_024,
                  nameCount <= UInt64(Int.max), byteCount <= UInt64(Int.max),
                  let namePointer, let bytePointer,
                  let name = String(
                      bytes: UnsafeBufferPointer(start: namePointer, count: Int(nameCount)),
                      encoding: .utf8
                  )
            else {
                _ = inkpod_encoded_sequence_release(&sequence)
                return animationFailure(getStatus == .ok ? .invalidArgument : getStatus)
            }
            output.append(CoreSequenceExportItem(
                name: name,
                bytes: Array(UnsafeBufferPointer(start: bytePointer, count: Int(byteCount)))
            ))
        }
        let releaseStatus = CoreStatus(cValue: inkpod_encoded_sequence_release(&sequence))
        guard releaseStatus == .ok, sequence == nil else { return animationFailure(releaseStatus) }
        return .sequenceExported(output)
    }

    private func validPath(_ bytes: [UInt8]) -> Bool {
        !bytes.isEmpty && bytes.count <= 32_768 && !bytes.contains(0)
            && String(bytes: bytes, encoding: .utf8) != nil
    }

    private func withPath<Result>(
        _ bytes: [UInt8],
        _ body: (UnsafePointer<UInt8>?, UInt64) -> Result
    ) -> Result {
        bytes.withUnsafeBytes { rawBytes in
            body(rawBytes.bindMemory(to: UInt8.self).baseAddress, UInt64(rawBytes.count))
        }
    }

    private func documentInfo() -> InkpodDocumentInfo {
        var info = InkpodDocumentInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodDocumentInfo>.size)
        return info
    }

    private func ffiRasterFormat(_ format: CoreCommonRasterFormat) -> InkpodCommonRasterFormat {
        switch format {
        case .png:
            inkpod_bridge_common_raster_png()
        case .tiff:
            inkpod_bridge_common_raster_tiff()
        case .tga:
            inkpod_bridge_common_raster_tga()
        case .bmp:
            inkpod_bridge_common_raster_bmp()
        }
    }

    private func bytes<T>(of value: T) -> [UInt8] {
        var value = value
        return withUnsafeBytes(of: &value) { Array($0) }
    }

    private func copy<T>(_ bytes: [UInt8], to value: inout T) {
        withUnsafeMutableBytes(of: &value) { destination in
            destination.copyBytes(from: bytes)
        }
    }

    private func destroyCores(_ cores: inout [OpaquePointer]) {
        for core in cores {
            var owner: OpaquePointer? = core
            _ = inkpod_core_destroy(&owner)
        }
        cores.removeAll(keepingCapacity: false)
    }

    private func cellPlanItem(
        from item: InkpodCellCreationPlanItem
    ) -> CoreCellCreationPlanItem? {
        guard let sizingMode = CoreCellSizingMode(rawValue: item.sizing_mode),
              let layerKind = CoreLayerKind(rawValue: item.initial_layer_kind),
              let pixelFormat = CorePixelStorageFormat(rawValue: item.pixel_format)
        else {
            return nil
        }
        return CoreCellCreationPlanItem(
            sizingMode: sizingMode,
            width: item.width,
            height: item.height,
            dpiXMilli: item.dpi_x_milli,
            dpiYMilli: item.dpi_y_milli,
            initialLayerKind: layerKind,
            pixelFormat: pixelFormat,
            hundredFrame: coreFrame(item.hundred_frame),
            referenceFrame: coreFrame(item.reference_frame),
            drawingFrame: coreFrame(item.drawing_frame),
            safeFrame: coreFrame(item.safe_frame),
            shootingFrame: coreFrame(item.shooting_frame),
            maximumCloseFrame: coreFrame(item.maximum_close_frame),
            margins: CoreMargins(
                left: item.margin_left,
                top: item.margin_top,
                right: item.margin_right,
                bottom: item.margin_bottom
            )
        )
    }

    private func coreFrame(_ frame: InkpodFrameRect) -> CoreFrameRect {
        CoreFrameRect(
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: frame.height
        )
    }

    private func ffiFrame(_ frame: CoreFrameRect) -> InkpodFrameRect {
        InkpodFrameRect(x: frame.x, y: frame.y, width: frame.width, height: frame.height)
    }

    private func paperFrames(from info: InkpodDocumentInfo) -> CorePaperFrames {
        CorePaperFrames(
            hundred: coreFrame(info.hundred_frame),
            reference: coreFrame(info.reference_frame),
            drawing: coreFrame(info.drawing_frame),
            safe: coreFrame(info.safe_frame),
            shooting: coreFrame(info.shooting_frame),
            maximumClose: coreFrame(info.maximum_close_frame),
            margins: CoreMargins(
                left: info.margin_left,
                top: info.margin_top,
                right: info.margin_right,
                bottom: info.margin_bottom
            )
        )
    }

    private func ffiResize(_ resize: CoreDocumentResize) -> InkpodDocumentResizeInput {
        var input = InkpodDocumentResizeInput()
        input.struct_size = UInt32(MemoryLayout<InkpodDocumentResizeInput>.size)
        input.anchor = resize.anchor.rawValue
        input.flags = resize.resample ? inkpod_bridge_document_resize_resample() : 0
        input.width = resize.width
        input.height = resize.height
        input.dpi_x_milli = resize.dpiXMilli
        input.dpi_y_milli = resize.dpiYMilli
        return input
    }

    private func nodeFlags(visible: Bool, editable: Bool) -> UInt64 {
        (visible ? UInt64(inkpod_bridge_node_visible()) : 0)
            | (editable ? UInt64(inkpod_bridge_node_editable()) : 0)
    }

    private func nodeInfo(
        core: OpaquePointer,
        layerIndex: UInt32,
        planeIndex: UInt32
    ) -> (InkpodNodeInfo, String)? {
        var info = InkpodNodeInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodNodeInfo>.size)
        let queryStatus = CoreStatus(
            cValue: inkpod_core_node_get(core, layerIndex, planeIndex, &info)
        )
        guard queryStatus == .ok || queryStatus == .bufferTooSmall,
              info.name_bytes <= 4_096,
              info.name_bytes <= UInt64(Int.max)
        else {
            return nil
        }
        var nameBytes = [UInt8](repeating: 0, count: Int(info.name_bytes))
        if !nameBytes.isEmpty {
            let copyStatus = nameBytes.withUnsafeMutableBufferPointer { buffer in
                info.name_utf8 = buffer.baseAddress
                info.name_capacity = UInt64(buffer.count)
                return CoreStatus(
                    cValue: inkpod_core_node_get(core, layerIndex, planeIndex, &info)
                )
            }
            guard copyStatus == .ok else { return nil }
        }
        guard let name = String(bytes: nameBytes, encoding: .utf8) else { return nil }
        return (info, name)
    }

    private func ffiColor(_ color: CoreColorValue) -> InkpodColorValue {
        var output = InkpodColorValue()
        output.struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
        output.depth = color.depth.rawValue
        output.red = color.red
        output.green = color.green
        output.blue = color.blue
        output.alpha = color.alpha
        return output
    }

    private func coreColor(_ color: InkpodColorValue) -> CoreColorValue? {
        guard let depth = CoreColorDepth(rawValue: color.depth) else { return nil }
        let output = CoreColorValue(
            depth: depth,
            red: color.red,
            green: color.green,
            blue: color.blue,
            alpha: color.alpha
        )
        return output.hasValidNativeComponents ? output : nil
    }

    private func ffiFillOptions(_ options: CoreFillOptions) -> InkpodEditorFillOptions {
        var output = InkpodEditorFillOptions()
        output.struct_size = UInt32(MemoryLayout<InkpodEditorFillOptions>.size)
        output.operation = options.operation.rawValue
        output.flags = inkpod_bridge_editor_fill_flags(
            options.detachedRegions ? 1 : 0,
            options.overflowAbort ? 1 : 0,
            options.transparentOnly ? 1 : 0,
            options.useDocumentSelection ? 1 : 0,
            options.useLightTableBoundary ? 1 : 0,
            options.useLightTableColor ? 1 : 0
        )
        output.tolerance = options.tolerance
        output.gap_close = options.gapClose
        output.inclusion_mode = options.inclusionMode.rawValue
        output.extension_distance = options.extensionDistance
        output.inclusion_color_count = UInt32(options.inclusionColors.count)
        withUnsafeMutableBytes(of: &output.inclusion_colors) { raw in
            let colors = raw.bindMemory(to: InkpodColorValue.self)
            for (index, color) in options.inclusionColors.enumerated() {
                colors[index] = ffiColor(color)
            }
        }
        return output
    }

    private func coreFillOptions(_ options: InkpodEditorFillOptions) -> CoreFillOptions? {
        guard let operation = CoreFillOperation(rawValue: options.operation),
              let inclusion = CoreInclusionMode(rawValue: options.inclusion_mode),
              options.inclusion_color_count <= 6
        else {
            return nil
        }
        var inclusionColors: [CoreColorValue] = []
        inclusionColors.reserveCapacity(Int(options.inclusion_color_count))
        var copy = options.inclusion_colors
        let valid = withUnsafeBytes(of: &copy) { raw -> Bool in
            let colors = raw.bindMemory(to: InkpodColorValue.self)
            for index in 0 ..< Int(options.inclusion_color_count) {
                guard let color = coreColor(colors[index]) else { return false }
                inclusionColors.append(color)
            }
            return true
        }
        guard valid else { return nil }
        let detachedFlag = inkpod_bridge_editor_fill_flags(1, 0, 0, 0, 0, 0)
        let overflowFlag = inkpod_bridge_editor_fill_flags(0, 1, 0, 0, 0, 0)
        let transparentFlag = inkpod_bridge_editor_fill_flags(0, 0, 1, 0, 0, 0)
        let selectionFlag = inkpod_bridge_editor_fill_flags(0, 0, 0, 1, 0, 0)
        let boundaryFlag = inkpod_bridge_editor_fill_flags(0, 0, 0, 0, 1, 0)
        let lightColorFlag = inkpod_bridge_editor_fill_flags(0, 0, 0, 0, 0, 1)
        return CoreFillOptions(
            operation: operation,
            detachedRegions: options.flags & detachedFlag != 0,
            overflowAbort: options.flags & overflowFlag != 0,
            transparentOnly: options.flags & transparentFlag != 0,
            useDocumentSelection: options.flags & selectionFlag != 0,
            useLightTableBoundary: options.flags & boundaryFlag != 0,
            useLightTableColor: options.flags & lightColorFlag != 0,
            tolerance: options.tolerance,
            gapClose: options.gap_close,
            inclusionMode: inclusion,
            extensionDistance: options.extension_distance,
            inclusionColors: inclusionColors
        )
    }

    private func coreBrushOptions(_ options: InkpodEditorBrushOptions) -> CoreBrushOptions? {
        guard let shape = CoreBrushShape(rawValue: options.shape),
              let startColor = CoreStartColorPredicate(rawValue: options.start_color)
        else {
            return nil
        }
        let output = CoreBrushOptions(
            shape: shape,
            smoothing: options.smoothing,
            startColor: startColor
        )
        return output.isValid ? output : nil
    }

    private func ffiSelectionOptions(
        _ options: CoreSelectionOptions
    ) -> InkpodEditorSelectionOptions {
        var output = InkpodEditorSelectionOptions()
        output.struct_size = UInt32(MemoryLayout<InkpodEditorSelectionOptions>.size)
        output.shape = options.shape.rawValue
        output.operation = options.operation.rawValue
        output.tolerance = options.tolerance
        output.gap_close = options.gapClose
        output.diameter_q16 = Int64((options.diameter * 65_536).rounded())
        output.interpretation = options.interpretation.rawValue
        output.aspect_ratio_q16 = UInt32(
            min(Double(UInt32.max), (options.aspectRatio * 65_536).rounded())
        )
        output.construction_flags = inkpod_bridge_selection_construction_flags(
            options.fromCenter ? 1 : 0,
            options.constrainRotationTo45Degrees ? 1 : 0,
            options.pressureControlsSize ? 1 : 0,
            options.screenSizedTrace ? 1 : 0
        )
        output.rotation_turns = options.rotationTurns
        output.trace_shape = options.traceShape.rawValue
        return output
    }

    private func coreSelectionOptions(
        _ options: InkpodEditorSelectionOptions
    ) -> CoreSelectionOptions? {
        guard let shape = CoreSelectionShape(rawValue: options.shape),
              let operation = CoreSelectionOperation(rawValue: options.operation),
              let interpretation = CoreRangeInterpretation(rawValue: options.interpretation),
              let traceShape = CoreTraceBrushShape(rawValue: options.trace_shape)
        else {
            return nil
        }
        let fromCenter = inkpod_bridge_selection_construction_flags(1, 0, 0, 0)
        let rotation = inkpod_bridge_selection_construction_flags(0, 1, 0, 0)
        let pressure = inkpod_bridge_selection_construction_flags(0, 0, 1, 0)
        let screen = inkpod_bridge_selection_construction_flags(0, 0, 0, 1)
        let output = CoreSelectionOptions(
            shape: shape,
            operation: operation,
            tolerance: options.tolerance,
            gapClose: options.gap_close,
            diameter: Double(options.diameter_q16) / 65_536,
            interpretation: interpretation,
            aspectRatio: Double(options.aspect_ratio_q16) / 65_536,
            fromCenter: options.construction_flags & fromCenter != 0,
            constrainRotationTo45Degrees: options.construction_flags & rotation != 0,
            pressureControlsSize: options.construction_flags & pressure != 0,
            screenSizedTrace: options.construction_flags & screen != 0,
            rotationTurns: options.rotation_turns,
            traceShape: traceShape
        )
        return output
    }

    private func editorProjection(
        for entry: CoreSessionEntry,
        session: CoreSessionProjection? = nil
    ) -> CoreEditorProjection? {
        let session = session ?? projection(for: entry)
        guard let session else { return nil }
        var editor = InkpodEditorStateInfo()
        editor.struct_size = UInt32(MemoryLayout<InkpodEditorStateInfo>.size)
        guard CoreStatus(cValue: inkpod_core_get_editor_state(entry.core, &editor)) == .ok,
              let activeTool = CoreEditorTool(rawValue: editor.active_tool),
              let color = coreColor(editor.current_color),
              let fill = coreFillOptions(editor.fill),
              let brush = coreBrushOptions(editor.brush),
              let selection = coreSelectionOptions(editor.selection)
        else {
            return nil
        }
        return CoreEditorProjection(
            session: session,
            editorRevision: editor.editor_revision,
            activeTool: activeTool,
            lastColorConsumingTool: CoreEditorTool(rawValue: editor.last_color_consuming_tool),
            currentColor: color,
            diameter: Double(editor.current_diameter_q16) / 65_536,
            activeLayerID: editor.active_layer_id,
            activePlaneID: editor.active_plane_id,
            fillOptions: fill,
            brushOptions: brush,
            selectionOptions: selection
        )
    }

    private func paletteProjection(core: OpaquePointer) -> CorePaletteProjection? {
        var query = InkpodColorBuffer()
        query.struct_size = UInt32(MemoryLayout<InkpodColorBuffer>.size)
        var status = CoreStatus(cValue: inkpod_core_palette_get(core, &query))
        guard status == .ok || status == .bufferTooSmall,
              query.color_count <= 4_096,
              query.color_count <= UInt64(Int.max)
        else {
            return nil
        }
        var colors = [InkpodColorValue](
            repeating: InkpodColorValue(),
            count: Int(query.color_count)
        )
        if colors.isEmpty {
            return CorePaletteProjection(colors: [])
        }
        for index in colors.indices {
            colors[index].struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
        }
        status = colors.withUnsafeMutableBufferPointer { buffer in
            query.colors = buffer.baseAddress
            query.color_capacity = UInt64(buffer.count)
            query.color_stride_bytes = UInt64(MemoryLayout<InkpodColorValue>.stride)
            return CoreStatus(cValue: inkpod_core_palette_get(core, &query))
        }
        guard status == .ok else { return nil }
        let values = colors.compactMap(coreColor)
        guard values.count == colors.count else { return nil }
        return CorePaletteProjection(colors: values)
    }

    private func colorChartEntry(
        core: OpaquePointer,
        index: UInt64,
        frequency: UInt64? = nil
    ) -> CoreColorChartEntry? {
        var color = InkpodColorValue()
        color.struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
        var nameCount: UInt64 = 0
        var status = CoreStatus(cValue: inkpod_core_color_chart_get(
            core,
            index,
            &color,
            nil,
            0,
            &nameCount
        ))
        guard status == .ok || status == .bufferTooSmall,
              nameCount <= 4_096,
              nameCount <= UInt64(Int.max)
        else {
            return nil
        }
        var bytes = [UInt8](repeating: 0, count: Int(nameCount))
        let hasNameBytes = !bytes.isEmpty
        status = CoreStatus(cValue: bytes.withUnsafeMutableBufferPointer { buffer in
            inkpod_core_color_chart_get(
                core,
                index,
                &color,
                hasNameBytes ? buffer.baseAddress : nil,
                UInt64(buffer.count),
                &nameCount
            )
        })
        guard status == .ok,
              let value = coreColor(color),
              let name = String(bytes: bytes, encoding: .utf8)
        else {
            return nil
        }
        return CoreColorChartEntry(index: index, color: value, name: name, frequency: frequency)
    }

    private func colorChartProjection(core: OpaquePointer) -> CoreColorChartProjection? {
        var info = InkpodColorChartInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodColorChartInfo>.size)
        guard CoreStatus(cValue: inkpod_core_color_chart_info(core, &info)) == .ok,
              info.entry_count <= 4_096
        else {
            return nil
        }
        var entries: [CoreColorChartEntry] = []
        entries.reserveCapacity(Int(info.entry_count))
        for index in 0 ..< info.entry_count {
            guard let entry = colorChartEntry(core: core, index: index) else { return nil }
            entries.append(entry)
        }
        return CoreColorChartProjection(
            entries: entries,
            isLocked: info.flags & inkpod_bridge_color_chart_locked() != 0,
            selectedIndex: info.flags & inkpod_bridge_color_chart_has_selection() != 0
                ? info.selected_index : nil,
            page: info.page
        )
    }

    private func paintProjection(for entry: CoreSessionEntry) -> CorePaintProjection? {
        guard let editor = editorProjection(for: entry),
              let palette = paletteProjection(core: entry.core),
              let chart = colorChartProjection(core: entry.core)
        else {
            return nil
        }
        return CorePaintProjection(
            editor: editor,
            palette: palette,
            chart: chart,
            colorCheckMode: entry.colorCheckMode
        )
    }

    private func validatePaintExpectation(
        _ expectation: CorePaintExpectation,
        entry: CoreSessionEntry,
        view: CoreViewEntry
    ) -> Bool {
        guard expectation.isValid,
              let editor = editorProjection(for: entry),
              let resolvedViewRevision = viewRevision(
                  core: entry.core,
                  coreViewID: view.coreViewID
              )
        else {
            return false
        }
        return editor.session.documentRevision == expectation.documentRevision
            && resolvedViewRevision == expectation.viewRevision
            && editor.editorRevision == expectation.editorRevision
            && editor.activeLayerID == expectation.layerID
            && editor.activePlaneID == expectation.planeID
    }

    private func resolveDocumentPoints(
        core: OpaquePointer,
        view: CoreViewEntry,
        expectedViewRevision: UInt64,
        samples: [CorePointerSample]
    ) -> (CoreStatus, [CoreDocumentPoint]) {
        guard !samples.isEmpty,
              samples.count <= 1_048_576,
              samples.allSatisfy(\.isValid)
        else {
            return (.invalidArgument, [])
        }
        var resolved: [CoreDocumentPoint] = []
        resolved.reserveCapacity(samples.count)
        for start in stride(from: 0, to: samples.count, by: 256) {
            let end = min(start + 256, samples.count)
            let ffiSamples = makeStrokeSamples(Array(samples[start ..< end]))
            var input = InkpodGeometryPointResolveInput()
            input.struct_size = UInt32(MemoryLayout<InkpodGeometryPointResolveInput>.size)
            input.coordinate_space = inkpod_bridge_coordinate_device()
            input.view_id = view.coreViewID
            input.expected_view_revision = expectedViewRevision
            input.sample_count = UInt64(ffiSamples.count)
            input.sample_stride_bytes = UInt64(MemoryLayout<InkpodStrokeSample>.stride)
            var result = InkpodGeometryPointResolveResult()
            result.struct_size = UInt32(MemoryLayout<InkpodGeometryPointResolveResult>.size)
            var points = [InkpodGeometryPoint](
                repeating: InkpodGeometryPoint(),
                count: ffiSamples.count
            )
            for index in points.indices {
                points[index].struct_size = UInt32(MemoryLayout<InkpodGeometryPoint>.size)
            }
            let status = ffiSamples.withUnsafeBufferPointer { sampleBuffer in
                input.samples = sampleBuffer.baseAddress
                return points.withUnsafeMutableBufferPointer { pointBuffer in
                    CoreStatus(cValue: inkpod_core_geometry_points_resolve(
                        core,
                        &input,
                        &result,
                        pointBuffer.baseAddress,
                        UInt64(pointBuffer.count)
                    ))
                }
            }
            guard status == .ok, result.point_count == UInt64(points.count) else {
                return (status, [])
            }
            resolved.append(contentsOf: points.map { CoreDocumentPoint(x: $0.x, y: $0.y) })
        }
        return (.ok, resolved)
    }

    private func treeProjection(for entry: CoreSessionEntry) -> CoreTreeProjection? {
        guard let session = projection(for: entry) else { return nil }
        var editor = InkpodEditorStateInfo()
        editor.struct_size = UInt32(MemoryLayout<InkpodEditorStateInfo>.size)
        guard CoreStatus(
            cValue: inkpod_core_get_editor_state(entry.core, &editor)
        ) == .ok else {
            return nil
        }
        var layers: [CoreLayerProjection] = []
        for layerIndex in UInt32(0) ..< UInt32(4_096) {
            var query = InkpodNodeInfo()
            query.struct_size = UInt32(MemoryLayout<InkpodNodeInfo>.size)
            let status = CoreStatus(
                cValue: inkpod_core_node_get(entry.core, layerIndex, UInt32.max, &query)
            )
            if status == .invalidArgument { break }
            guard status == .ok || status == .bufferTooSmall,
                  let (layer, layerName) = nodeInfo(
                      core: entry.core,
                      layerIndex: layerIndex,
                      planeIndex: UInt32.max
                  ),
                  let layerKind = CoreLayerKind(rawValue: layer.kind),
                  layer.child_count <= 4_096
            else {
                return nil
            }
            var planes: [CoreNodeProjection] = []
            planes.reserveCapacity(Int(layer.child_count))
            for planeIndex in UInt32(0) ..< layer.child_count {
                guard let (plane, planeName) = nodeInfo(
                    core: entry.core,
                    layerIndex: layerIndex,
                    planeIndex: planeIndex
                ),
                let planeKind = CorePlaneKind(rawValue: plane.kind),
                let pixelFormat = CorePixelStorageFormat(rawValue: plane.pixel_format)
                else {
                    return nil
                }
                planes.append(CoreNodeProjection(
                    id: plane.id,
                    parentID: plane.parent_id,
                    planeKind: planeKind,
                    pixelFormat: pixelFormat,
                    opacityMilli: plane.opacity_milli,
                    index: plane.index,
                    isVisible: plane.flags & inkpod_bridge_node_visible() != 0,
                    isEditable: plane.flags & inkpod_bridge_node_editable() != 0,
                    name: planeName
                ))
            }
            layers.append(CoreLayerProjection(
                id: layer.id,
                kind: layerKind,
                pixelFormat: .none,
                opacityMilli: layer.opacity_milli,
                index: layer.index,
                isVisible: layer.flags & inkpod_bridge_node_visible() != 0,
                isEditable: layer.flags & inkpod_bridge_node_editable() != 0,
                name: layerName,
                planes: planes
            ))
        }
        guard !layers.isEmpty else { return nil }
        return CoreTreeProjection(
            session: session,
            editorRevision: editor.editor_revision,
            activeLayerID: editor.active_layer_id,
            activePlaneID: editor.active_plane_id,
            layers: layers
        )
    }

    private func viewRevision(core: OpaquePointer, coreViewID: UInt64) -> UInt64? {
        var options = InkpodSnapshotOptions()
        options.struct_size = UInt32(MemoryLayout<InkpodSnapshotOptions>.size)
        var snapshot: OpaquePointer?
        let status = coreViewID == 0
            ? CoreStatus(cValue: inkpod_core_build_snapshot(core, &options, &snapshot))
            : CoreStatus(cValue: inkpod_core_build_snapshot_for_view(
                core,
                coreViewID,
                &options,
                &snapshot
            ))
        guard status == .ok, let raw = snapshot else {
            if snapshot != nil { _ = inkpod_snapshot_release(&snapshot) }
            return nil
        }
        let owner = CoreOwnedSnapshot(raw: raw)
        defer { owner.release() }
        return try? owner.withBorrowedRenderView { $0.transform.viewRevision }
    }

    private func projection(for entry: CoreSessionEntry) -> CoreSessionProjection? {
        var replay = InkpodReplayContract()
        replay.struct_size = UInt32(MemoryLayout<InkpodReplayContract>.size)
        guard CoreStatus(
            cValue: inkpod_core_get_replay_contract(entry.core, &replay)
        ) == .ok else {
            return nil
        }
        var info = InkpodDocumentInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodDocumentInfo>.size)
        guard CoreStatus(
            cValue: inkpod_core_get_document_info(entry.core, &info)
        ) == .ok else {
            return nil
        }
        return CoreSessionProjection(
            target: entry.target,
            primaryView: entry.primaryView,
            documentUUID: CoreDocumentUUID(
                high: info.document_uuid_high,
                low: info.document_uuid_low
            ),
            cellID: info.cell_id,
            documentRevision: info.document_revision,
            viewRevision: info.view_revision,
            abiVersion: inkpod_abi_version(),
            replayEpoch: replay.replay_epoch,
            procedureFormatVersion: replay.procedure_format_version,
            ownerThreadID: ownerThreadID,
            hasActiveTransient: entry.activeTransient != nil,
            canUndo: info.flags & inkpod_bridge_document_can_undo() != 0,
            canRedo: info.flags & inkpod_bridge_document_can_redo() != 0,
            isDirty: info.flags & inkpod_bridge_document_dirty() != 0,
            isRecovered: info.flags & inkpod_bridge_document_recovered() != 0,
            documentWidth: info.width,
            documentHeight: info.height,
            dpiXMilli: info.dpi_x_milli,
            dpiYMilli: info.dpi_y_milli,
            paperFrames: paperFrames(from: info)
        )
    }

    private func resolve(_ target: CoreViewTarget) -> CoreViewTargetResolution {
        switch resolve(target.session) {
        case let .live(entry):
            guard target.id.rawValue != 0, target.generation.rawValue != 0 else {
                return .invalid
            }
            if let view = entry.views[target.id] {
                return view.target.generation == target.generation
                    ? .live(entry, view)
                    : .stale
            }
            if let retired = retiredViewGenerations[target.id] {
                return retired == target.generation ? .retired : .stale
            }
            return target.id.rawValue < nextViewID ? .stale : .invalid
        case .retired:
            return .retired
        case .invalid:
            return .invalid
        case .stale:
            return .stale
        }
    }

    private func validSamples(_ samples: [CorePointerSample]) -> Bool {
        !samples.isEmpty && samples.count <= 4_096 && samples.allSatisfy(\.isValid)
    }

    private func dispatchResult() -> InkpodDispatchResult {
        var result = InkpodDispatchResult()
        result.struct_size = UInt32(MemoryLayout<InkpodDispatchResult>.size)
        return result
    }

    private func previewBatch(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        graph: CoreBatchGraphDraft,
        scope: CoreBatchRunScope
    ) -> CoreRequestOutcome {
        guard graph.isRunReady else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            withCreatedBatchGraph(graph) { rawGraph in
                batchPreview(core: entry.core, graph: rawGraph, scope: scope)
            }
        }
    }

    private func executeBatch(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        graph: CoreBatchGraphDraft,
        options: CoreBatchRunOptions,
        requestID: CoreRequestID
    ) -> CoreRequestOutcome {
        guard graph.isRunReady else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            withCreatedBatchGraph(graph) { rawGraph in
                batchExecute(
                    core: entry.core,
                    graph: rawGraph,
                    options: options,
                    requestID: requestID
                )
            }
        }
    }

    private func saveBatchGraph(
        _ graph: CoreBatchGraphDraft,
        pathUTF8: [UInt8]
    ) -> CoreRequestOutcome {
        guard graph.isValid, validBatchPath(pathUTF8) else { return .failed(.invalidRequest) }
        return withCreatedBatchGraph(graph) { rawGraph in
            pathUTF8.withUnsafeBufferPointer { path in
                let status = CoreStatus(cValue: inkpod_batch_graph_save(
                    rawGraph,
                    path.baseAddress,
                    UInt64(path.count)
                ))
                return status == .ok ? .acknowledged : batchFailure(status)
            }
        }
    }

    private func inspectBatchGraph(pathUTF8: [UInt8]) -> CoreRequestOutcome {
        guard validBatchPath(pathUTF8) else { return .failed(.invalidRequest) }
        return withLoadedBatchGraph(pathUTF8) { rawGraph in
            guard let summary = batchGraphSummary(rawGraph) else {
                return .failed(.coreOperation(.panic))
            }
            return .batchGraph(summary)
        }
    }

    private func previewSavedBatch(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8],
        operations: [CoreBatchOperation],
        scope: CoreBatchRunScope
    ) -> CoreRequestOutcome {
        guard validBatchPath(pathUTF8), !operations.isEmpty,
              operations.allSatisfy({ $0.isValid && !$0.configureEachRun })
        else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            withLoadedBatchGraph(pathUTF8) { rawGraph in
                withBatchRunCopy(rawGraph, operations: operations) { runGraph in
                    batchPreview(core: entry.core, graph: runGraph, scope: scope)
                }
            }
        }
    }

    private func executeSavedBatch(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8],
        operations: [CoreBatchOperation],
        options: CoreBatchRunOptions,
        requestID: CoreRequestID
    ) -> CoreRequestOutcome {
        guard validBatchPath(pathUTF8), !operations.isEmpty,
              operations.allSatisfy({ $0.isValid && !$0.configureEachRun })
        else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            withLoadedBatchGraph(pathUTF8) { rawGraph in
                withBatchRunCopy(rawGraph, operations: operations) { runGraph in
                    batchExecute(
                        core: entry.core,
                        graph: runGraph,
                        options: options,
                        requestID: requestID
                    )
                }
            }
        }
    }

    private func extractBatchPairs(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        oldSequenceIndex: UInt32,
        newSequenceIndex: UInt32
    ) -> CoreRequestOutcome {
        guard oldSequenceIndex != newSequenceIndex else { return .failed(.invalidRequest) }
        return withLiveSession(target, expectedDocumentRevision: expectedDocumentRevision) { entry in
            var oldIdentity = InkpodSequenceSourceIdentity()
            oldIdentity.struct_size = UInt32(MemoryLayout<InkpodSequenceSourceIdentity>.size)
            var newIdentity = InkpodSequenceSourceIdentity()
            newIdentity.struct_size = UInt32(MemoryLayout<InkpodSequenceSourceIdentity>.size)
            var status = CoreStatus(cValue: inkpod_core_sequence_source_identity(
                entry.core,
                oldSequenceIndex,
                &oldIdentity
            ))
            guard status == .ok else { return batchFailure(status) }
            status = CoreStatus(cValue: inkpod_core_sequence_source_identity(
                entry.core,
                newSequenceIndex,
                &newIdentity
            ))
            guard status == .ok else { return batchFailure(status) }
            var preview: OpaquePointer?
            status = CoreStatus(cValue: inkpod_core_batch_extract_color_pairs(
                entry.core,
                &oldIdentity,
                &newIdentity,
                &preview
            ))
            guard status == .ok, let rawPreview = preview else {
                if preview != nil { _ = inkpod_batch_pair_preview_release(&preview) }
                return batchFailure(status == .ok ? .panic : status)
            }
            defer { _ = inkpod_batch_pair_preview_release(&preview) }
            guard let projection = batchPairProjection(rawPreview) else {
                return .failed(.coreOperation(.panic))
            }
            return .batchPairPreview(projection)
        }
    }

    private func withCreatedBatchGraph(
        _ draft: CoreBatchGraphDraft,
        body: (OpaquePointer) -> CoreRequestOutcome
    ) -> CoreRequestOutcome {
        let storage = CoreBatchFFIStorage(draft)
        var input = storage.input
        var graph: OpaquePointer?
        let status = CoreStatus(cValue: inkpod_batch_graph_create(&input, &graph))
        guard status == .ok, let rawGraph = graph else {
            if graph != nil { _ = inkpod_batch_graph_release(&graph) }
            return batchFailure(status == .ok ? .panic : status)
        }
        defer { _ = inkpod_batch_graph_release(&graph) }
        return body(rawGraph)
    }

    private func withLoadedBatchGraph(
        _ pathUTF8: [UInt8],
        body: (OpaquePointer) -> CoreRequestOutcome
    ) -> CoreRequestOutcome {
        var graph: OpaquePointer?
        let status = pathUTF8.withUnsafeBufferPointer { path in
            CoreStatus(cValue: inkpod_batch_graph_load(
                path.baseAddress,
                UInt64(path.count),
                &graph
            ))
        }
        guard status == .ok, let rawGraph = graph else {
            if graph != nil { _ = inkpod_batch_graph_release(&graph) }
            return batchFailure(status == .ok ? .panic : status)
        }
        defer { _ = inkpod_batch_graph_release(&graph) }
        return body(rawGraph)
    }

    private func withBatchRunCopy(
        _ graph: OpaquePointer,
        operations: [CoreBatchOperation],
        body: (OpaquePointer) -> CoreRequestOutcome
    ) -> CoreRequestOutcome {
        let storage = CoreBatchFFIStorage(CoreBatchGraphDraft(
            name: "Run",
            inputs: [.currentSequence()],
            operations: operations
        ))
        let input = storage.input
        var runGraph: OpaquePointer?
        let status = CoreStatus(cValue: inkpod_batch_graph_clone_with_operations(
            graph,
            input.operations,
            input.operation_count,
            input.operation_stride_bytes,
            &runGraph
        ))
        guard status == .ok, let rawRunGraph = runGraph else {
            if runGraph != nil { _ = inkpod_batch_graph_release(&runGraph) }
            return batchFailure(status == .ok ? .panic : status)
        }
        defer { _ = inkpod_batch_graph_release(&runGraph) }
        return body(rawRunGraph)
    }

    private func batchPreview(
        core: OpaquePointer,
        graph: OpaquePointer,
        scope: CoreBatchRunScope
    ) -> CoreRequestOutcome {
        var preview: OpaquePointer?
        let status = CoreStatus(cValue: inkpod_core_batch_preview(
            core,
            graph,
            scope.rawValue,
            &preview
        ))
        guard status == .ok, let rawPreview = preview else {
            if preview != nil { _ = inkpod_batch_preview_release(&preview) }
            return batchFailure(status == .ok ? .panic : status)
        }
        defer { _ = inkpod_batch_preview_release(&preview) }
        var count: UInt64 = 0
        guard CoreStatus(cValue: inkpod_batch_preview_count(rawPreview, &count)) == .ok,
              count <= 65_536,
              count <= UInt64(Int.max)
        else { return .failed(.coreOperation(.panic)) }
        var items: [CoreBatchPreviewItem] = []
        items.reserveCapacity(Int(count))
        for index in 0 ..< count {
            var item = InkpodBatchPreviewItem()
            item.struct_size = UInt32(MemoryLayout<InkpodBatchPreviewItem>.size)
            let itemStatus = CoreStatus(cValue: inkpod_batch_preview_get(
                rawPreview,
                index,
                &item
            ))
            guard itemStatus == .ok,
                  let inputName = m10String(item.input_name, item.input_name_bytes),
                  let outputPath = m10String(item.output_path, item.output_path_bytes),
                  let warning = m10String(item.warning, item.warning_bytes)
            else { return .failed(.coreOperation(itemStatus == .ok ? .panic : itemStatus)) }
            items.append(CoreBatchPreviewItem(
                inputName: inputName,
                outputPath: outputPath,
                warning: warning
            ))
        }
        return .batchPreview(CoreBatchPreviewProjection(items: items))
    }

    private func batchExecute(
        core: OpaquePointer,
        graph: OpaquePointer,
        options: CoreBatchRunOptions,
        requestID: CoreRequestID
    ) -> CoreRequestOutcome {
        var task: OpaquePointer?
        let createStatus = CoreStatus(cValue: inkpod_batch_task_create(&task))
        guard createStatus == .ok, let rawTask = task else {
            if task != nil { _ = inkpod_batch_task_release(&task) }
            return batchFailure(createStatus == .ok ? .panic : createStatus)
        }
        batchCancellations.begin(requestID, task: rawTask)
        defer {
            batchCancellations.finish(requestID)
            _ = inkpod_batch_task_release(&task)
        }
        let flags = (options.dryRun ? UInt64(1) : 0)
            | (options.previewConfirmed ? UInt64(1 << 1) : 0)
        var report: OpaquePointer?
        let status = CoreStatus(cValue: inkpod_core_batch_execute(
            core,
            graph,
            options.scope.rawValue,
            flags,
            rawTask,
            &report
        ))
        defer {
            if report != nil { _ = inkpod_batch_report_release(&report) }
        }
        if let rawReport = report, let projection = batchReportProjection(rawReport) {
            return status == .ok || status == .cancelled
                ? .batchReport(projection) : batchFailure(status)
        }
        return batchFailure(status == .ok ? .panic : status)
    }

    private func batchReportProjection(_ report: OpaquePointer) -> CoreBatchReportProjection? {
        var info = InkpodBatchReportInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodBatchReportInfo>.size)
        guard CoreStatus(cValue: inkpod_batch_report_get_info(report, &info)) == .ok,
              info.item_count <= 65_536,
              info.item_count <= UInt64(Int.max)
        else { return nil }
        var items: [CoreBatchReportItem] = []
        items.reserveCapacity(Int(info.item_count))
        for index in 0 ..< info.item_count {
            var item = InkpodBatchReportItem()
            item.struct_size = UInt32(MemoryLayout<InkpodBatchReportItem>.size)
            guard CoreStatus(cValue: inkpod_batch_report_get(report, index, &item)) == .ok,
                  let outcome = CoreBatchItemOutcome(rawValue: item.outcome),
                  let inputName = m10String(item.input_name, item.input_name_bytes),
                  let outputPath = m10String(item.output_path, item.output_path_bytes),
                  let message = m10String(item.message, item.message_bytes)
            else { return nil }
            items.append(CoreBatchReportItem(
                outcome: outcome,
                inputName: inputName,
                outputPath: outputPath,
                message: message
            ))
        }
        return CoreBatchReportProjection(
            cancelled: info.cancelled != 0,
            failureCount: info.failure_count,
            items: items
        )
    }

    private func batchGraphSummary(_ graph: OpaquePointer) -> CoreBatchGraphSummary? {
        var info = InkpodBatchGraphInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodBatchGraphInfo>.size)
        guard CoreStatus(cValue: inkpod_batch_graph_get_info(graph, &info)) == .ok,
              info.operation_count <= 1_024,
              let outputPolicy = CoreBatchOutputPolicy(rawValue: info.output_policy),
              let failurePolicy = CoreBatchFailurePolicy(rawValue: info.failure_policy)
        else { return nil }
        var operations: [CoreBatchOperation] = []
        operations.reserveCapacity(Int(info.operation_count))
        for index in 0 ..< info.operation_count {
            guard let operation = batchOperationProjection(graph, index: index) else { return nil }
            operations.append(operation)
        }
        return CoreBatchGraphSummary(
            version: info.version,
            inputCount: info.input_count,
            operationCount: info.operation_count,
            operationKinds: operations.map(\.kind),
            operations: operations,
            outputPolicy: outputPolicy,
            failurePolicy: failurePolicy
        )
    }

    private func batchOperationProjection(
        _ graph: OpaquePointer,
        index: UInt64
    ) -> CoreBatchOperation? {
        var info = InkpodBatchOperationInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodBatchOperationInfo>.size)
        guard CoreStatus(cValue: inkpod_batch_graph_get_operation(graph, index, &info)) == .ok,
              let kind = CoreBatchOperationKind(rawValue: info.kind),
              info.color_count <= 4_096,
              info.color_pair_count <= 4_096,
              info.seed_count <= 4_096,
              info.curve_point_count <= 4_096
        else { return nil }

        let parameters = withUnsafeBytes(of: &info.parameters) {
            Array($0.bindMemory(to: Int64.self))
        }
        let target: CoreBatchTargetSelector?
        if info.layer_id == 0, info.plane_id == 0, info.layer_kind == 0,
           info.plane_kind == 0, info.missing_policy == 0
        {
            target = nil
        } else {
            guard let missing = CoreBatchMissingPolicy(rawValue: info.missing_policy) else {
                return nil
            }
            let layerKind = info.layer_kind == 0 ? nil : CoreLayerKind(rawValue: info.layer_kind)
            let planeKind = info.plane_kind == 0 ? nil : CorePlaneKind(rawValue: info.plane_kind)
            guard info.layer_kind == 0 || layerKind != nil,
                  info.plane_kind == 0 || planeKind != nil
            else { return nil }
            target = CoreBatchTargetSelector(
                layerID: info.layer_id == 0 ? nil : info.layer_id,
                planeID: info.plane_id == 0 ? nil : info.plane_id,
                layerKind: layerKind,
                planeKind: planeKind,
                missingPolicy: missing
            )
        }

        var colors: [CoreColorValue] = []
        colors.reserveCapacity(Int(info.color_count))
        for row in 0 ..< info.color_count {
            var color = InkpodColorValue()
            color.struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
            guard CoreStatus(cValue: inkpod_batch_graph_get_operation_color(
                graph, index, row, &color
            )) == .ok,
                let value = m10CoreColor(color)
            else { return nil }
            colors.append(value)
        }

        var pairs: [CoreBatchColorPair] = []
        pairs.reserveCapacity(Int(info.color_pair_count))
        for row in 0 ..< info.color_pair_count {
            var pair = InkpodBatchColorPairInput()
            pair.struct_size = UInt32(MemoryLayout<InkpodBatchColorPairInput>.size)
            guard CoreStatus(cValue: inkpod_batch_graph_get_operation_color_pair(
                graph, index, row, &pair
            )) == .ok,
                let oldColor = m10CoreColor(pair.old_color),
                let newColor = m10CoreColor(pair.new_color)
            else { return nil }
            pairs.append(CoreBatchColorPair(
                enabled: pair.enabled != 0,
                oldColor: oldColor,
                newColor: newColor
            ))
        }

        var seeds: [CoreBatchSeed] = []
        seeds.reserveCapacity(Int(info.seed_count))
        for row in 0 ..< info.seed_count {
            var seed = InkpodBatchSeedInput()
            seed.struct_size = UInt32(MemoryLayout<InkpodBatchSeedInput>.size)
            guard CoreStatus(cValue: inkpod_batch_graph_get_operation_seed(
                graph, index, row, &seed
            )) == .ok,
                seed.tolerance <= UInt32(UInt16.max),
                seed.gap_close <= UInt32(UInt8.max),
                let fill = m10CoreColor(seed.fill_color)
            else { return nil }
            let expected: CoreColorValue?
            if seed.flags & 1 != 0 {
                guard let value = m10CoreColor(seed.expected_color) else { return nil }
                expected = value
            } else {
                expected = nil
            }
            seeds.append(CoreBatchSeed(
                enabled: seed.flags & (1 << 1) != 0,
                x: seed.x,
                y: seed.y,
                fillColor: fill,
                tolerance: UInt16(seed.tolerance),
                gapClose: UInt8(seed.gap_close),
                expectedColor: expected
            ))
        }

        var filter: CoreFilterRequest?
        if kind == .filter {
            guard let filterKind = CoreFilterKind(rawValue: info.filter_kind),
                  let channel = CoreFilterChannel(rawValue: info.filter_channel),
                  let interpolation = CoreCurveInterpolation(
                      rawValue: info.filter_interpolation
                  )
            else { return nil }
            var curve: [CoreCurvePoint] = []
            curve.reserveCapacity(Int(info.curve_point_count))
            for row in 0 ..< info.curve_point_count {
                var point = InkpodCurvePoint()
                point.struct_size = UInt32(MemoryLayout<InkpodCurvePoint>.size)
                guard CoreStatus(cValue: inkpod_batch_graph_get_operation_curve_point(
                    graph, index, row, &point
                )) == .ok else { return nil }
                curve.append(CoreCurvePoint(input: point.input, output: point.output))
            }
            let filterParameters = withUnsafeBytes(of: &info.filter_parameters) {
                Array($0.bindMemory(to: Int32.self))
            }
            filter = CoreFilterRequest(
                kind: filterKind,
                planeID: info.plane_id == 0 ? 1 : info.plane_id,
                channel: channel,
                interpolation: interpolation,
                parameters: filterParameters,
                curvePoints: curve
            )
        }

        if kind == .separation {
            guard let replacement = m10CoreColor(info.color_0) else { return nil }
            let empty = CoreColorValue(
                depth: replacement.depth,
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0
            )
            pairs = [CoreBatchColorPair(oldColor: empty, newColor: replacement)]
        }
        return CoreBatchOperation(
            kind: kind,
            enabled: info.flags & 1 != 0,
            configureEachRun: info.flags & (1 << 1) != 0,
            target: target,
            parameters: parameters,
            colors: colors,
            colorPairs: pairs,
            seeds: seeds,
            filter: filter
        )
    }

    private func batchPairProjection(_ preview: OpaquePointer) -> CoreBatchPairPreviewProjection? {
        var info = InkpodBatchPairPreviewInfo()
        info.struct_size = UInt32(MemoryLayout<InkpodBatchPairPreviewInfo>.size)
        guard CoreStatus(cValue: inkpod_batch_pair_preview_get_info(preview, &info)) == .ok,
              info.candidate_count <= 65_536,
              let format = CorePixelStorageFormat(rawValue: info.pixel_format)
        else { return nil }
        var candidates: [CoreBatchPairCandidateProjection] = []
        for index in 0 ..< info.candidate_count {
            var candidate = InkpodBatchPairCandidate()
            candidate.struct_size = UInt32(MemoryLayout<InkpodBatchPairCandidate>.size)
            guard CoreStatus(cValue: inkpod_batch_pair_preview_get_candidate(
                preview,
                index,
                &candidate
            )) == .ok,
                let oldColor = m10CoreColor(candidate.old_color),
                let newColor = m10CoreColor(candidate.new_color)
            else { return nil }
            candidates.append(CoreBatchPairCandidateProjection(
                oldColor: oldColor,
                newColor: newColor,
                pixelCount: candidate.pixel_count,
                affectedBounds: CoreFrameRect(
                    x: candidate.bounds_x,
                    y: candidate.bounds_y,
                    width: candidate.bounds_width,
                    height: candidate.bounds_height
                ),
                ambiguous: candidate.flags & 1 != 0
            ))
        }
        return CoreBatchPairPreviewProjection(
            pixelFormat: format,
            width: info.width,
            height: info.height,
            unchangedPixelCount: info.unchanged_pixel_count,
            ambiguityCount: info.ambiguity_count,
            candidates: candidates
        )
    }

    private func validBatchPath(_ bytes: [UInt8]) -> Bool {
        !bytes.isEmpty && bytes.count <= 32_768 && !bytes.contains(0)
            && String(bytes: bytes, encoding: .utf8) != nil
    }

    private func batchFailure(_ status: CoreStatus) -> CoreRequestOutcome {
        switch status {
        case .invalidArgument:
            .failed(.invalidRequest)
        case .cancelled:
            .failed(.cancelled)
        default:
            .failed(.coreOperation(status))
        }
    }

    private func withLiveSession(
        _ target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        body: (CoreSessionEntry) -> CoreRequestOutcome
    ) -> CoreRequestOutcome {
        switch resolve(target) {
        case .retired, .stale:
            return .failed(.staleTarget)
        case .invalid:
            return .failed(.invalidTarget)
        case let .live(entry):
            guard projection(for: entry)?.documentRevision == expectedDocumentRevision else {
                return .failed(.staleTarget)
            }
            return body(entry)
        }
    }

    private func paintMutationOutcome(
        status: CoreStatus,
        before: UInt64,
        entry: CoreSessionEntry
    ) -> CoreRequestOutcome {
        guard status == .ok else { return .failed(.coreOperation(status)) }
        guard let paint = paintProjection(for: entry) else {
            return .failed(.coreOperation(.panic))
        }
        return paint.editor.session.documentRevision == before
            ? .noOp(paint.editor.session) : .paintUpdated(paint)
    }

    private func documentMutationOutcome(
        status: CoreStatus,
        before: UInt64,
        entry: CoreSessionEntry
    ) -> CoreRequestOutcome {
        guard status == .ok else { return .failed(.coreOperation(status)) }
        guard let updated = projection(for: entry) else {
            return .failed(.coreOperation(.panic))
        }
        return updated.documentRevision == before
            ? .noOp(updated) : .documentUpdated(updated)
    }

    private func documentPixel(_ value: Float, upperBound: UInt32) -> UInt32? {
        guard value.isFinite, value >= 0, value < Float(upperBound) else { return nil }
        return UInt32(value.rounded(.down))
    }

    private func viewZoom(core: OpaquePointer, view: CoreViewEntry) -> Double? {
        var options = InkpodSnapshotOptions()
        options.struct_size = UInt32(MemoryLayout<InkpodSnapshotOptions>.size)
        var snapshot: OpaquePointer?
        let status = view.coreViewID == 0
            ? CoreStatus(cValue: inkpod_core_build_snapshot(core, &options, &snapshot))
            : CoreStatus(cValue: inkpod_core_build_snapshot_for_view(
                core,
                view.coreViewID,
                &options,
                &snapshot
            ))
        guard status == .ok, let raw = snapshot else {
            if snapshot != nil { _ = inkpod_snapshot_release(&snapshot) }
            return nil
        }
        defer { _ = inkpod_snapshot_release(&snapshot) }
        var transform = InkpodSnapshotTransform()
        transform.struct_size = UInt32(MemoryLayout<InkpodSnapshotTransform>.size)
        guard CoreStatus(cValue: inkpod_snapshot_get_transform(raw, &transform)) == .ok,
              transform.zoom.isFinite, transform.zoom > 0
        else {
            return nil
        }
        return transform.zoom
    }

    private func documentBounds(
        from first: CoreDocumentPoint,
        to second: CoreDocumentPoint,
        width: UInt32,
        height: UInt32
    ) -> CoreFrameRect? {
        guard let firstX = documentPixel(first.x, upperBound: width),
              let firstY = documentPixel(first.y, upperBound: height),
              let secondX = documentPixel(second.x, upperBound: width),
              let secondY = documentPixel(second.y, upperBound: height)
        else {
            return nil
        }
        let minX = min(firstX, secondX)
        let minY = min(firstY, secondY)
        let maxX = max(firstX, secondX)
        let maxY = max(firstY, secondY)
        return CoreFrameRect(
            x: Int32(clamping: minX),
            y: Int32(clamping: minY),
            width: Int32(clamping: UInt64(maxX) - UInt64(minX) + 1),
            height: Int32(clamping: UInt64(maxY) - UInt64(minY) + 1)
        )
    }

    private func withFFIColorChartEntries<Result>(
        _ entries: [CoreColorChartEntry],
        body: (UnsafeBufferPointer<InkpodColorChartEntry>) -> Result
    ) -> Result {
        if entries.isEmpty {
            return body(UnsafeBufferPointer(start: nil, count: 0))
        }
        var ffiEntries = entries.map { entry in
            var output = InkpodColorChartEntry()
            output.struct_size = UInt32(MemoryLayout<InkpodColorChartEntry>.size)
            output.color = ffiColor(entry.color)
            return output
        }
        let names = entries.map { Array($0.name.utf8) }
        func bindName(_ index: Int) -> Result {
            guard index < names.count else {
                return ffiEntries.withUnsafeBufferPointer(body)
            }
            return names[index].withUnsafeBufferPointer { buffer in
                ffiEntries[index].name_utf8 = buffer.baseAddress
                ffiEntries[index].name_bytes = UInt64(buffer.count)
                return bindName(index + 1)
            }
        }
        return bindName(0)
    }

    private func colorChartPreviewEntries(
        preview: OpaquePointer,
        count: UInt64
    ) -> [CoreColorChartEntry]? {
        var entries: [CoreColorChartEntry] = []
        entries.reserveCapacity(Int(count))
        for index in 0 ..< count {
            var color = InkpodColorValue()
            color.struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
            var nameCount: UInt64 = 0
            var frequency: UInt64 = 0
            var status = CoreStatus(cValue: inkpod_color_chart_preview_get(
                preview,
                index,
                &color,
                nil,
                0,
                &nameCount,
                &frequency
            ))
            guard status == .ok || status == .bufferTooSmall,
                  nameCount <= 4_096,
                  nameCount <= UInt64(Int.max)
            else {
                return nil
            }
            var bytes = [UInt8](repeating: 0, count: Int(nameCount))
            status = CoreStatus(cValue: bytes.withUnsafeMutableBufferPointer { buffer in
                inkpod_color_chart_preview_get(
                    preview,
                    index,
                    &color,
                    buffer.baseAddress,
                    UInt64(buffer.count),
                    &nameCount,
                    &frequency
                )
            })
            guard status == .ok,
                  let value = coreColor(color),
                  let name = String(bytes: bytes, encoding: .utf8)
            else {
                return nil
            }
            entries.append(CoreColorChartEntry(
                index: index,
                color: value,
                name: name,
                frequency: frequency
            ))
        }
        return entries
    }

    @discardableResult
    private func releaseColorChartPreview(_ id: CoreColorChartPreviewID) -> Bool {
        guard let owned = colorChartPreviews.removeValue(forKey: id) else { return true }
        var raw: OpaquePointer? = owned.raw
        return CoreStatus(cValue: inkpod_color_chart_preview_release(&raw)) == .ok
            && raw == nil
    }

    private func releaseColorChartPreviews(for session: CoreSessionTarget) -> Bool {
        let ids = colorChartPreviews.compactMap { key, value in
            value.session == session ? key : nil
        }.sorted { $0.rawValue < $1.rawValue }
        return ids.allSatisfy(releaseColorChartPreview)
    }

    @discardableResult
    private func releaseHistoryVisualizationEntry(
        _ id: CoreHistoryVisualizationID
    ) -> Bool {
        guard var entry = historyVisualizations.removeValue(forKey: id) else { return true }
        var succeeded = true
        if entry.builder != nil {
            if let task = entry.task {
                succeeded = CoreStatus(cValue:
                    inkpod_history_visualization_builder_release(&entry.builder, task)
                ) == .ok && entry.builder == nil && succeeded
            } else {
                succeeded = false
            }
        }
        if entry.visualization != nil {
            succeeded = CoreStatus(cValue:
                inkpod_history_visualization_release(&entry.visualization)
            ) == .ok && entry.visualization == nil && succeeded
        }
        if entry.task != nil {
            succeeded = CoreStatus(cValue: inkpod_task_release(&entry.task)) == .ok
                && entry.task == nil && succeeded
        }
        return succeeded
    }

    private func releaseHistoryVisualizations(for session: CoreSessionTarget) -> Bool {
        let ids = historyVisualizations.compactMap { key, value in
            value.session == session ? key : nil
        }.sorted { $0.rawValue < $1.rawValue }
        var succeeded = true
        for id in ids {
            succeeded = releaseHistoryVisualizationEntry(id) && succeeded
        }
        return succeeded
    }

    private func colorReplaceSamples(_ region: CoreColorReplaceRegion) -> [CorePointerSample] {
        switch region {
        case .entireSelectionOrDocument:
            []
        case let .rectangle(gesture):
            [gesture.start, gesture.end]
        case let .pen(samples, _), let .polyline(samples), let .lasso(samples):
            samples
        }
    }

    private func selectionPoints(
        _ points: [CoreDocumentPoint],
        source: [CorePointerSample]
    ) -> [InkpodSelectionPoint] {
        zip(points, source).map { point, sample in
            var output = InkpodSelectionPoint()
            output.struct_size = UInt32(MemoryLayout<InkpodSelectionPoint>.size)
            output.x = point.x
            output.y = point.y
            output.pressure = sample.pressure
            return output
        }
    }

    private func makeStrokeSamples(_ samples: [CorePointerSample]) -> [InkpodStrokeSample] {
        samples.map { sample in
            var output = InkpodStrokeSample()
            output.struct_size = UInt32(MemoryLayout<InkpodStrokeSample>.size)
            output.x = sample.deviceX
            output.y = sample.deviceY
            output.pressure = sample.pressure
            return output
        }
    }

    private func resolve(_ target: CoreSessionTarget) -> CoreTargetResolution {
        guard target.id.rawValue != 0, target.generation.rawValue != 0 else {
            return .invalid
        }
        if let live = sessions[target.id] {
            return live.target.generation == target.generation ? .live(live) : .stale
        }
        if let retired = retiredGenerations[target.id] {
            return retired == target.generation ? .retired : .stale
        }
        if target.id.rawValue < nextSessionID {
            return .stale
        }
        return .invalid
    }

    private func resolve(_ target: CoreCutTarget) -> CoreCutTargetResolution {
        guard target.id.rawValue != 0, target.generation.rawValue != 0 else {
            return .invalid
        }
        if let live = cuts[target.id] {
            return live.target.generation == target.generation ? .live(live) : .stale
        }
        if let retired = retiredCutGenerations[target.id] {
            return retired == target.generation ? .retired : .stale
        }
        return target.id.rawValue < nextCutID ? .stale : .invalid
    }

    private func executeShutdown(requestID: CoreRequestID) {
        for cutID in cuts.keys.sorted() {
            guard let entry = cuts[cutID] else { continue }
            var cut: OpaquePointer? = entry.cut
            _ = inkpod_cut_destroy(&cut)
        }
        cuts.removeAll(keepingCapacity: false)
        for visualizationID in historyVisualizations.keys.sorted(by: {
            $0.rawValue < $1.rawValue
        }) {
            _ = releaseHistoryVisualizationEntry(visualizationID)
        }
        for previewID in colorChartPreviews.keys.sorted(by: {
            $0.rawValue < $1.rawValue
        }) {
            _ = releaseColorChartPreview(previewID)
        }
        for planID in cellPlans.keys.sorted(by: { $0.rawValue < $1.rawValue }) {
            guard let plan = cellPlans.removeValue(forKey: planID) else { continue }
            var raw: OpaquePointer? = plan.raw
            _ = inkpod_cell_creation_plan_release(&raw)
        }
        for clipboardID in clipboards.keys.sorted(by: { $0.rawValue < $1.rawValue }) {
            guard let ownedClipboard = clipboards.removeValue(forKey: clipboardID) else { continue }
            var clipboard: OpaquePointer? = ownedClipboard
            _ = inkpod_clipboard_release(&clipboard)
        }
        var destroyed: [CoreSessionID] = []
        for sessionID in sessions.keys.sorted() {
            guard let entry = sessions[sessionID] else { continue }
            if let transient = entry.activeTransient {
                _ = cancelTransient(transient, core: entry.core)
            }
            var core: OpaquePointer? = entry.core
            if CoreStatus(cValue: inkpod_core_destroy(&core)) == .ok, core == nil {
                destroyed.append(sessionID)
            }
        }
        sessions.removeAll(keepingCapacity: false)
        sessionByDocumentUUID.removeAll(keepingCapacity: false)

        var cancelledCount = 0
        for envelope in mailbox.drainAndStop() where envelope.requestID != requestID {
            if completions.complete(envelope.requestID, with: .failed(.cancelled)) {
                cancelledCount += 1
            }
        }
        completions.complete(
            requestID,
            with: .shutdown(
                CoreShutdownProjection(
                    ownerThreadID: ownerThreadID,
                    destroyedSessionIDs: destroyed,
                    cancelledRequestCount: cancelledCount
                )
            )
        )
    }
}
