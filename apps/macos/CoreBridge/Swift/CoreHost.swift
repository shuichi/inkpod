import Foundation

public final class CoreHost: @unchecked Sendable {
    private let requestLock = NSLock()
    private var nextRequestID: UInt64 = 1
    private let completions: CoreCompletionRegistry
    private let mailbox: CoreMailbox
    private let ownerThread: CoreOwnerThread

    public convenience init() {
        self.init(testConfiguration: CoreHostTestConfiguration())
    }

    init(testConfiguration: CoreHostTestConfiguration) {
        let completions = CoreCompletionRegistry()
        let mailbox = CoreMailbox(
            normalAdmissionFailureCount: testConfiguration.normalAdmissionFailureCount
        )
        self.completions = completions
        self.mailbox = mailbox
        ownerThread = CoreOwnerThread(
            mailbox: mailbox,
            completions: completions,
            testConfiguration: testConfiguration
        )
        ownerThread.start()
    }

    public func createSession(documentUUID: CoreDocumentUUID) -> CoreTask {
        submit(.createSession(documentUUID), lane: .normal)
    }

    public func prepareCellCreation(_ options: CoreCellCreationOptions) -> CoreTask {
        submit(.prepareCellCreation(options), lane: .normal)
    }

    public func commitCellCreation(
        plan: CoreCellCreationPlanID,
        documentUUIDs: [CoreDocumentUUID]
    ) -> CoreTask {
        submit(.commitCellCreation(plan, documentUUIDs), lane: .normal)
    }

    public func cancelCellCreation(_ plan: CoreCellCreationPlanID) -> CoreTask {
        submit(.cancelCellCreation(plan), lane: .control)
    }

    public func inspectSession(_ target: CoreSessionTarget) -> CoreTask {
        submit(.inspectSession(target), lane: .normal)
    }

    public func closeSession(_ target: CoreSessionTarget) -> CoreTask {
        submit(.closeSession(target), lane: .control)
    }

    public func createCut(
        cutUUID: CoreCutUUID,
        metadata: CoreCutMetadata,
        defaults: CoreCutDefaults,
        members: [CoreCutMember]
    ) -> CoreTask {
        submit(.createCut(cutUUID, metadata, defaults, members), lane: .normal)
    }

    public func inspectCut(_ target: CoreCutTarget) -> CoreTask {
        submit(.inspectCut(target), lane: .normal)
    }

    public func closeCut(_ target: CoreCutTarget) -> CoreTask {
        submit(.closeCut(target), lane: .control)
    }

    public func openCut(pathUTF8: [UInt8]) -> CoreTask {
        submit(.openCut(pathUTF8, false), lane: .normal)
    }

    public func openCutRecovery(pathUTF8: [UInt8]) -> CoreTask {
        submit(.openCut(pathUTF8, true), lane: .normal)
    }

    public func updateCut(
        target: CoreCutTarget,
        expectedRevision: UInt64,
        metadata: CoreCutMetadata,
        defaults: CoreCutDefaults
    ) -> CoreTask {
        submit(.updateCut(target, expectedRevision, metadata, defaults), lane: .normal)
    }

    public func cancelCutUpdate(_ target: CoreCutTarget) -> CoreTask {
        submit(.cancelCutUpdate(target), lane: .control)
    }

    public func editCutSequence(
        target: CoreCutTarget,
        expectedRevision: UInt64,
        operations: [CoreCutSequenceOperation]
    ) -> CoreTask {
        submit(.editCutSequence(target, expectedRevision, operations), lane: .normal)
    }

    public func cancelCutSequence(_ target: CoreCutTarget) -> CoreTask {
        submit(.cancelCutSequence(target), lane: .control)
    }

    public func undoCut(target: CoreCutTarget, expectedRevision: UInt64) -> CoreTask {
        submit(.undoCut(target, expectedRevision), lane: .normal)
    }

    public func redoCut(target: CoreCutTarget, expectedRevision: UInt64) -> CoreTask {
        submit(.redoCut(target, expectedRevision), lane: .normal)
    }

    public func saveCut(
        target: CoreCutTarget,
        expectedRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreTask {
        submit(.saveCut(target, expectedRevision, pathUTF8, false), lane: .normal)
    }

    public func autosaveCut(
        target: CoreCutTarget,
        expectedRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreTask {
        submit(.saveCut(target, expectedRevision, pathUTF8, true), lane: .normal)
    }

    public func createView(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.createView(target, expectedDocumentRevision), lane: .normal)
    }

    public func closeView(_ target: CoreViewTarget) -> CoreTask {
        submit(.closeView(target), lane: .control)
    }

    public func applyView(
        target: CoreViewTarget,
        command: CoreViewCommand,
        expectation: CoreCommandExpectation? = nil
    ) -> CoreTask {
        submit(.applyView(target, command, expectation), lane: .normal)
    }

    public func resolveDocumentPoints(
        target: CoreViewTarget,
        expectedDocumentRevision: UInt64,
        expectedViewRevision: UInt64,
        samples: [CorePointerSample]
    ) -> CoreTask {
        submit(
            .resolveDocumentPoints(
                target,
                expectedDocumentRevision,
                expectedViewRevision,
                samples
            ),
            lane: .inputBoundary
        )
    }

    public func applyDocument(
        target: CoreSessionTarget,
        command: CoreDocumentCommand,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.applyDocument(target, command, expectedDocumentRevision), lane: .normal)
    }

    public func editCell(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        command: CoreCellEditCommand
    ) -> CoreTask {
        submit(.editCell(target, expectedDocumentRevision, command), lane: .normal)
    }

    public func inspectTree(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64? = nil
    ) -> CoreTask {
        submit(.inspectTree(target, expectedDocumentRevision), lane: .normal)
    }

    public func setActiveNode(
        target: CoreSessionTarget,
        layerID: UInt64,
        planeID: UInt64,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(
            .setActiveNode(target, expectedDocumentRevision, layerID, planeID),
            lane: .normal
        )
    }

    public func editTree(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        command: CoreTreeEditCommand
    ) -> CoreTask {
        submit(.editTree(target, expectedDocumentRevision, command), lane: .normal)
    }

    public func beginPencilStroke(
        target: CoreViewTarget,
        samples: [CorePointerSample]
    ) -> CoreTask {
        submit(.beginRasterStroke(target, nil, samples), lane: .inputBoundary)
    }

    public func appendPencilStroke(
        target: CoreViewTarget,
        samples: [CorePointerSample]
    ) -> CoreTask {
        submit(
            .appendRasterStroke(target, samples),
            lane: .inputSample,
            cancelStrokeOnAdmissionFailure: target
        )
    }

    public func inspectPaint(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64? = nil
    ) -> CoreTask {
        submit(.inspectPaint(target, expectedDocumentRevision), lane: .normal)
    }

    public func updateEditor(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        update: CoreEditorUpdate
    ) -> CoreTask {
        submit(.updateEditor(target, expectation, update), lane: .normal)
    }

    public func beginRasterStroke(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        samples: [CorePointerSample]
    ) -> CoreTask {
        submit(.beginRasterStroke(target, expectation, samples), lane: .inputBoundary)
    }

    public func appendRasterStroke(
        target: CoreViewTarget,
        samples: [CorePointerSample]
    ) -> CoreTask {
        submit(
            .appendRasterStroke(target, samples),
            lane: .inputSample,
            cancelStrokeOnAdmissionFailure: target
        )
    }

    public func applyFill(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        gesture: CoreFillGesture
    ) -> CoreTask {
        submit(.applyFill(target, expectation, gesture), lane: .inputBoundary)
    }

    public func eyedropper(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        source: CoreEyedropperSource,
        devicePoint: CorePointerSample
    ) -> CoreTask {
        submit(.eyedropper(target, expectation, source, devicePoint), lane: .inputBoundary)
    }

    public func replacePalette(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        colors: [CoreColorValue]
    ) -> CoreTask {
        submit(.replacePalette(target, expectedDocumentRevision, colors), lane: .normal)
    }

    public func generatePalette(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        maximumColors: UInt32,
        quantizationBits: UInt32
    ) -> CoreTask {
        submit(
            .generatePalette(target, expectedDocumentRevision, maximumColors, quantizationBits),
            lane: .normal
        )
    }

    public func savePaletteFile(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreTask {
        submit(.savePaletteFile(target, expectedDocumentRevision, pathUTF8), lane: .normal)
    }

    public func loadPaletteFile(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreTask {
        submit(.loadPaletteFile(target, expectedDocumentRevision, pathUTF8), lane: .normal)
    }

    public func replaceColorChart(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        entries: [CoreColorChartEntry],
        locked: Bool
    ) -> CoreTask {
        submit(
            .replaceColorChart(target, expectedDocumentRevision, entries, locked),
            lane: .normal
        )
    }

    public func saveColorChartFile(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreTask {
        submit(.saveColorChartFile(target, expectedDocumentRevision, pathUTF8), lane: .normal)
    }

    public func loadColorChartFile(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreTask {
        submit(.loadColorChartFile(target, expectedDocumentRevision, pathUTF8), lane: .normal)
    }

    public func createColorChartPreview(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        maximumColors: UInt32,
        quantizationBits: UInt32
    ) -> CoreTask {
        submit(
            .createColorChartPreview(
                target,
                expectedDocumentRevision,
                maximumColors,
                quantizationBits
            ),
            lane: .normal
        )
    }

    public func applyColorChartPreview(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        preview: CoreColorChartPreviewID
    ) -> CoreTask {
        submit(
            .applyColorChartPreview(target, expectedDocumentRevision, preview),
            lane: .normal
        )
    }

    public func cancelColorChartPreview(_ preview: CoreColorChartPreviewID) -> CoreTask {
        submit(.cancelColorChartPreview(preview), lane: .control)
    }

    public func setColorCheck(
        target: CoreViewTarget,
        expectedViewRevision: UInt64,
        mode: CoreColorCheckMode
    ) -> CoreTask {
        submit(.setColorCheck(target, expectedViewRevision, mode), lane: .normal)
    }

    public func inspectLocator(
        target: CoreViewTarget,
        expectedViewRevision: UInt64,
        devicePoint: CorePointerSample,
        radius: UInt32 = 4
    ) -> CoreTask {
        submit(
            .inspectLocator(target, expectedViewRevision, devicePoint, radius),
            lane: .normal
        )
    }

    public func paintLocatorPixel(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        documentX: Int32,
        documentY: Int32
    ) -> CoreTask {
        submit(
            .paintLocatorPixel(target, expectation, documentX, documentY),
            lane: .inputBoundary
        )
    }

    public func previewColorReplace(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        request: CoreColorReplaceRequest
    ) -> CoreTask {
        submit(.previewColorReplace(target, expectation, request), lane: .normal)
    }

    public func applyColorReplace(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        request: CoreColorReplaceRequest
    ) -> CoreTask {
        submit(.applyColorReplace(target, expectation, request), lane: .inputBoundary)
    }

    public func selectOutputColorGuard(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        operation: CoreSelectionOperation
    ) -> CoreTask {
        submit(
            .selectOutputColorGuard(target, expectedDocumentRevision, operation),
            lane: .normal
        )
    }

    public func applySelection(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        samples: [CorePointerSample]
    ) -> CoreTask {
        submit(.applySelection(target, expectation, samples), lane: .inputBoundary)
    }

    public func selectionAdjust(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        operation: CoreSelectionAdjustOperation,
        pixels: UInt32
    ) -> CoreTask {
        submit(
            .selectionAdjust(target, expectedDocumentRevision, operation, pixels),
            lane: .normal
        )
    }

    public func clearSelection(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.clearSelection(target, expectedDocumentRevision), lane: .normal)
    }

    public func selectColor(
        target: CoreViewTarget,
        expectation: CorePaintExpectation,
        different: Bool,
        operation: CoreSelectionOperation
    ) -> CoreTask {
        submit(
            .selectColor(target, expectation, different, operation),
            lane: .normal
        )
    }

    public func selectionToLayer(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        nameUTF8: [UInt8]
    ) -> CoreTask {
        submit(.selectionToLayer(target, expectedDocumentRevision, nameUTF8), lane: .normal)
    }

    public func selectionFromLayer(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        layerID: UInt64,
        operation: CoreSelectionLayerOperation
    ) -> CoreTask {
        submit(
            .selectionFromLayer(
                target,
                expectedDocumentRevision,
                layerID,
                operation
            ),
            lane: .normal
        )
    }

    public func endStroke(target: CoreViewTarget) -> CoreTask {
        submit(
            .endStroke(target),
            lane: .inputBoundary,
            cancelStrokeOnAdmissionFailure: target
        )
    }

    public func cancelStroke(target: CoreViewTarget) -> CoreTask {
        submit(.cancelStroke(target), lane: .inputBoundary)
    }

    public func undo(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64? = nil
    ) -> CoreTask {
        submit(.undo(target, expectedDocumentRevision), lane: .normal)
    }

    public func redo(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64? = nil
    ) -> CoreTask {
        submit(.redo(target, expectedDocumentRevision), lane: .normal)
    }

    public func inspectHistory(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64? = nil
    ) -> CoreTask {
        submit(.inspectHistory(target, expectedDocumentRevision), lane: .normal)
    }

    public func jumpHistory(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        cursor: UInt64
    ) -> CoreTask {
        submit(.jumpHistory(target, expectedDocumentRevision, cursor), lane: .normal)
    }

    public func buildSnapshot(route: CoreSnapshotRoute) -> CoreTask {
        submit(.buildSnapshot(route), lane: .normal)
    }

    public func save(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8],
        allowCleanSave: Bool
    ) -> CoreTask {
        submit(
            .save(target, expectedDocumentRevision, pathUTF8, allowCleanSave),
            lane: .normal
        )
    }

    public func open(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreTask {
        submit(.open(target, expectedDocumentRevision, pathUTF8), lane: .normal)
    }

    public func autosave(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreTask {
        submit(.autosave(target, expectedDocumentRevision, pathUTF8), lane: .normal)
    }

    public func openRecovery(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8]
    ) -> CoreTask {
        submit(.openRecovery(target, expectedDocumentRevision, pathUTF8), lane: .normal)
    }

    public func revert(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.revert(target, expectedDocumentRevision), lane: .normal)
    }

    public func revertPartial(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.revertPartial(target, expectedDocumentRevision), lane: .normal)
    }

    public func importCommonRaster(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        format: CoreCommonRasterFormat,
        bytes: [UInt8],
        documentUUID: CoreDocumentUUID
    ) -> CoreTask {
        submit(
            .importCommonRaster(
                target,
                expectedDocumentRevision,
                format,
                bytes,
                documentUUID
            ),
            lane: .normal
        )
    }

    public func exportCommonRaster(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        format: CoreCommonRasterFormat,
        compositeWhite: Bool
    ) -> CoreTask {
        submit(
            .exportCommonRaster(target, expectedDocumentRevision, format, compositeWhite),
            lane: .normal
        )
    }

    public func compactionPlan(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.compactionPlan(target, expectedDocumentRevision), lane: .normal)
    }

    public func writeCompactedCopy(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8],
        token: CoreCompactionToken
    ) -> CoreTask {
        submit(
            .writeCompactedCopy(target, expectedDocumentRevision, pathUTF8, token),
            lane: .normal
        )
    }

    public func copyClipboard(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.copyClipboard(target, expectedDocumentRevision, false), lane: .normal)
    }

    public func cutClipboard(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.copyClipboard(target, expectedDocumentRevision, true), lane: .normal)
    }

    public func createClipboard(from raster: CoreClipboardRaster) -> CoreTask {
        submit(.createClipboard(raster), lane: .normal)
    }

    public func releaseClipboard(_ clipboard: CoreClipboardID) -> CoreTask {
        submit(.releaseClipboard(clipboard), lane: .control)
    }

    public func beginPaste(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        clipboard: CoreClipboardID,
        mode: CorePasteMode
    ) -> CoreTask {
        submit(
            .beginPaste(target, expectedDocumentRevision, clipboard, mode),
            lane: .normal
        )
    }

    public func transformFloatingPaste(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        transform: CoreFloatingTransform
    ) -> CoreTask {
        submit(
            .transformFloatingPaste(target, expectedDocumentRevision, transform),
            lane: .inputSample
        )
    }

    public func commitPaste(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.commitPaste(target, expectedDocumentRevision), lane: .normal)
    }

    public func cancelPaste(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.cancelPaste(target, expectedDocumentRevision), lane: .control)
    }

    public func beginHistoryVisualization(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(
            .beginHistoryVisualization(target, expectedDocumentRevision),
            lane: .normal
        )
    }

    public func stepHistoryVisualization(
        _ visualization: CoreHistoryVisualizationID,
        maximumEvents: UInt32
    ) -> CoreTask {
        submit(
            .stepHistoryVisualization(visualization, maximumEvents),
            lane: .normal
        )
    }

    public func historyVisualizationRows(
        _ visualization: CoreHistoryVisualizationID,
        range: Range<UInt64>
    ) -> CoreTask {
        submit(.historyVisualizationRows(visualization, range), lane: .normal)
    }

    public func releaseHistoryVisualization(
        _ visualization: CoreHistoryVisualizationID
    ) -> CoreTask {
        submit(.releaseHistoryVisualization(visualization), lane: .control)
    }

    public func inspectM8(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.inspectM8(target, expectedDocumentRevision), lane: .normal)
    }

    public func performM8(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        command: CoreM8Command
    ) -> CoreTask {
        let lane: CoreRequestLane
        switch command {
        case .cancelFilterPreview, .cancelGeometryPreview,
             .shootingFramePreviewCancel, .vanishingPointPreviewCancel:
            lane = .control
        default:
            lane = .normal
        }
        return submit(.performM8(target, expectedDocumentRevision, command), lane: lane)
    }

    public func inspectAnimation(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.inspectAnimation(target, expectedDocumentRevision), lane: .normal)
    }

    public func performAnimation(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        command: CoreAnimationCommand
    ) -> CoreTask {
        let lane: CoreRequestLane = switch command {
        case .motionStop:
            .control
        default:
            .normal
        }
        return submit(
            .performAnimation(target, expectedDocumentRevision, command),
            lane: lane
        )
    }

    public func exportInstructionRaster(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        format: CoreCommonRasterFormat,
        compositeWhite: Bool
    ) -> CoreTask {
        submit(
            .exportInstructionRaster(
                target,
                expectedDocumentRevision,
                format,
                compositeWhite
            ),
            lane: .normal
        )
    }

    public func previewBatch(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        graph: CoreBatchGraphDraft,
        scope: CoreBatchRunScope
    ) -> CoreTask {
        submit(.previewBatch(target, expectedDocumentRevision, graph, scope), lane: .normal)
    }

    public func executeBatch(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        graph: CoreBatchGraphDraft,
        options: CoreBatchRunOptions
    ) -> CoreTask {
        submit(.executeBatch(target, expectedDocumentRevision, graph, options), lane: .normal)
    }

    public func saveBatchGraph(_ graph: CoreBatchGraphDraft, pathUTF8: [UInt8]) -> CoreTask {
        submit(.saveBatchGraph(graph, pathUTF8), lane: .normal)
    }

    public func inspectBatchGraph(pathUTF8: [UInt8]) -> CoreTask {
        submit(.inspectBatchGraph(pathUTF8), lane: .normal)
    }

    public func previewSavedBatch(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8],
        operations: [CoreBatchOperation],
        scope: CoreBatchRunScope
    ) -> CoreTask {
        submit(
            .previewSavedBatch(target, expectedDocumentRevision, pathUTF8, operations, scope),
            lane: .normal
        )
    }

    public func executeSavedBatch(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        pathUTF8: [UInt8],
        operations: [CoreBatchOperation],
        options: CoreBatchRunOptions
    ) -> CoreTask {
        submit(
            .executeSavedBatch(target, expectedDocumentRevision, pathUTF8, operations, options),
            lane: .normal
        )
    }

    public func extractBatchColorPairs(
        target: CoreSessionTarget,
        expectedDocumentRevision: UInt64,
        oldSequenceIndex: UInt32,
        newSequenceIndex: UInt32
    ) -> CoreTask {
        submit(
            .extractBatchPairs(
                target,
                expectedDocumentRevision,
                oldSequenceIndex,
                newSequenceIndex
            ),
            lane: .normal
        )
    }

    public func batchProgress(request requestID: CoreRequestID) -> CoreBatchProgressProjection? {
        ownerThread.batchProgress(requestID)
    }

    public func cancel(request requestID: CoreRequestID) -> CoreTask {
        ownerThread.requestCancellation(requestID)
        return submit(.cancel(requestID), lane: .control)
    }

    public func shutdown() -> CoreTask {
        submit(.shutdown, lane: .control, isShutdown: true)
    }

    public func waitUntilStopped(timeout: TimeInterval) -> Bool {
        ownerThread.waitUntilFinished(timeout: timeout)
    }

    func beginTransientForTesting(_ target: CoreSessionTarget) -> CoreTask {
        submit(.beginTransientForTesting(target), lane: .normal)
    }

    func selectAllForTesting(
        _ target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        selectAll(target, expectedDocumentRevision: expectedDocumentRevision)
    }

    public func selectAll(
        _ target: CoreSessionTarget,
        expectedDocumentRevision: UInt64
    ) -> CoreTask {
        submit(.selectAll(target, expectedDocumentRevision), lane: .normal)
    }

    func setNormalProcessingEnabledForTesting(_ enabled: Bool) -> CoreTask {
        submit(.setNormalProcessingEnabledForTesting(enabled), lane: .control)
    }

    private func submit(
        _ request: CoreRequest,
        lane: CoreRequestLane,
        isShutdown: Bool = false,
        cancelStrokeOnAdmissionFailure: CoreViewTarget? = nil
    ) -> CoreTask {
        let requestID = allocateRequestID()
        let task = completions.register(requestID: requestID)
        let admission = mailbox.enqueue(
            CoreRequestEnvelope(requestID: requestID, request: request),
            lane: lane,
            isShutdown: isShutdown
        )
        switch admission {
        case .accepted:
            break
        case .queueFull:
            completions.complete(requestID, with: .failed(.queueFull))
            enqueueEmergencyStrokeCancel(cancelStrokeOnAdmissionFailure)
        case .allocationFailed:
            completions.complete(requestID, with: .failed(.allocationFailed))
            enqueueEmergencyStrokeCancel(cancelStrokeOnAdmissionFailure)
        case .hostStopped:
            completions.complete(
                requestID,
                with: isShutdown ? .noOp(nil) : .failed(.hostStopped)
            )
        case .shutdownAlreadyEnqueued:
            completions.complete(requestID, with: .noOp(nil))
        }
        return task
    }

    private func enqueueEmergencyStrokeCancel(_ target: CoreViewTarget?) {
        guard let target else { return }
        let requestID = allocateRequestID()
        let task = completions.register(requestID: requestID)
        let admission = mailbox.enqueue(
            CoreRequestEnvelope(requestID: requestID, request: .cancelStroke(target)),
            lane: .control
        )
        if admission != .accepted {
            completions.complete(requestID, with: .failed(.queueFull))
        }
        _ = task
    }

    private func allocateRequestID() -> CoreRequestID {
        requestLock.withLock {
            precondition(nextRequestID != 0 && nextRequestID < UInt64.max)
            let requestID = CoreRequestID(rawValue: nextRequestID)
            nextRequestID += 1
            return requestID
        }
    }
}
