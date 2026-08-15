import Foundation
import XCTest
@testable import InkpodCoreBridge

final class CoreM10IntegrationTests: XCTestCase {
    func testPreviewAndDryRunUseNaturalOrderWithoutMutatingLiveDocument() throws {
        let host = CoreHost()
        let directory = try makeM10Directory()
        defer { try? FileManager.default.removeItem(at: directory) }

        let current = try m10Created(host, 1)
        for (number, low) in [(10, UInt64(10)), (2, UInt64(2))] {
            let source = try m10Created(host, low)
            let url = directory.appendingPathComponent("cell\(number).inkpod")
            _ = try m10File(host.save(
                target: source.target,
                expectedDocumentRevision: source.documentRevision,
                pathUTF8: Array(url.path.utf8),
                allowCleanSave: true
            ))
        }
        let output = directory.appendingPathComponent("output", isDirectory: true)
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        let graph = CoreBatchGraphDraft(
            name: "Natural order",
            inputs: [.folder(directory.path)],
            operations: [.invertColorPlane()],
            output: CoreBatchOutputSettings(
                policy: .newSave,
                folder: output.path,
                basename: "result"
            )
        )

        let preview = try m10Preview(host.previewBatch(
            target: current.target,
            expectedDocumentRevision: current.documentRevision,
            graph: graph,
            scope: .all
        ))
        XCTAssertEqual(preview.items.map(\.inputName), ["cell2.inkpod", "cell10.inkpod"])

        let report = try m10Report(host.executeBatch(
            target: current.target,
            expectedDocumentRevision: current.documentRevision,
            graph: graph,
            options: CoreBatchRunOptions(scope: .all, dryRun: true, previewConfirmed: true)
        ))
        XCTAssertEqual(report.items.map(\.outcome), [.dryRun, .dryRun])
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: output.path).isEmpty)
        let inspected = try m10Inspected(host.inspectSession(current.target))
        XCTAssertEqual(inspected.documentRevision, current.documentRevision)
        XCTAssertEqual(inspected.isDirty, current.isDirty)
        try m10Shutdown(host)
    }

    func testInvalidConfigureEachRunAndStaleRevisionNeverStartAJob() throws {
        let host = CoreHost()
        let current = try m10Created(host, 20)
        var unresolved = CoreBatchOperation.invertColorPlane()
        unresolved.configureEachRun = true
        let graph = CoreBatchGraphDraft(
            name: "Unresolved",
            inputs: [.currentSequence()],
            operations: [unresolved]
        )

        XCTAssertEqual(
            try m10Outcome(host.executeBatch(
                target: current.target,
                expectedDocumentRevision: current.documentRevision,
                graph: graph,
                options: CoreBatchRunOptions(scope: .current, dryRun: true)
            )),
            .failed(.invalidRequest)
        )
        var resolved = unresolved
        resolved.configureEachRun = false
        XCTAssertEqual(
            try m10Outcome(host.previewBatch(
                target: current.target,
                expectedDocumentRevision: current.documentRevision + 1,
                graph: CoreBatchGraphDraft(
                    name: "Stale",
                    inputs: [.currentSequence()],
                    operations: [resolved]
                ),
                scope: .current
            )),
            .failed(.staleTarget)
        )
        let inspected = try m10Inspected(host.inspectSession(current.target))
        XCTAssertEqual(inspected.documentRevision, current.documentRevision)
        try m10Shutdown(host)
    }

    func testQueuedCancellationIsExactlyOnceAndWritesNothing() throws {
        let host = CoreHost()
        let directory = try makeM10Directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let current = try m10Created(host, 30)
        try m10Acknowledged(host.setNormalProcessingEnabledForTesting(false))
        let task = host.executeBatch(
            target: current.target,
            expectedDocumentRevision: current.documentRevision,
            graph: CoreBatchGraphDraft(
                name: "Cancelled",
                inputs: [.currentSequence()],
                operations: [.invertColorPlane()],
                output: CoreBatchOutputSettings(
                    policy: .newSave,
                    folder: directory.path,
                    basename: "cancelled"
                )
            ),
            options: CoreBatchRunOptions(scope: .all, previewConfirmed: true)
        )
        let cancel = host.cancel(request: task.requestID)
        XCTAssertEqual(try m10Outcome(task), .failed(.cancelled))
        try m10Acknowledged(cancel)
        try m10Acknowledged(host.setNormalProcessingEnabledForTesting(true))
        XCTAssertEqual(task.completionCount, 1)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
        try m10Shutdown(host)
    }

    func testBatchSetSaveLoadFailureAndAllOperationKindsCrossTheTypedBridge() throws {
        let host = CoreHost()
        let directory = try makeM10Directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let current = try m10Created(host, 40)
        let setURL = directory.appendingPathComponent("all.inkbatch")
        let graph = CoreBatchGraphDraft(
            name: "All operations",
            inputs: [.currentSequence()],
            operations: CoreBatchOperationKind.allCases.map(CoreBatchOperation.example)
        )
        try m10Acknowledged(host.saveBatchGraph(graph, pathUTF8: Array(setURL.path.utf8)))
        let summary = try m10GraphSummary(host.inspectBatchGraph(pathUTF8: Array(setURL.path.utf8)))
        XCTAssertEqual(summary.operationKinds, CoreBatchOperationKind.allCases)
        XCTAssertEqual(summary.operationCount, UInt64(CoreBatchOperationKind.allCases.count))
        XCTAssertEqual(summary.operations.map(\.kind), CoreBatchOperationKind.allCases)

        let missing = directory.appendingPathComponent("missing.inkbatch")
        guard case .failed(.coreOperation(.ioError)) = try m10Outcome(
            host.inspectBatchGraph(pathUTF8: Array(missing.path.utf8))
        ) else {
            return XCTFail("missing set must report an I/O failure")
        }
        let inspected = try m10Inspected(host.inspectSession(current.target))
        XCTAssertEqual(inspected.documentRevision, current.documentRevision)
        try m10Shutdown(host)
    }

    func testLoadedConfigureEachRunRequiresAndUsesAnImmutableRunCopy() throws {
        let host = CoreHost()
        let directory = try makeM10Directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let current = try m10Created(host, 50)
        var operation = CoreBatchOperation.invertColorPlane()
        operation.configureEachRun = true
        let setURL = directory.appendingPathComponent("per-run.inkbatch")
        try m10Acknowledged(host.saveBatchGraph(
            CoreBatchGraphDraft(
                name: "Per run",
                inputs: [.currentSequence()],
                operations: [operation]
            ),
            pathUTF8: Array(setURL.path.utf8)
        ))
        let summary = try m10GraphSummary(host.inspectBatchGraph(
            pathUTF8: Array(setURL.path.utf8)
        ))
        XCTAssertTrue(summary.operations[0].configureEachRun)
        XCTAssertEqual(
            try m10Outcome(host.previewSavedBatch(
                target: current.target,
                expectedDocumentRevision: current.documentRevision,
                pathUTF8: Array(setURL.path.utf8),
                operations: summary.operations,
                scope: .current
            )),
            .failed(.invalidRequest)
        )
        var resolved = summary.operations
        resolved[0].configureEachRun = false
        _ = try m10Preview(host.previewSavedBatch(
            target: current.target,
            expectedDocumentRevision: current.documentRevision,
            pathUTF8: Array(setURL.path.utf8),
            operations: resolved,
            scope: .current
        ))
        let inspected = try m10Inspected(host.inspectSession(current.target))
        XCTAssertEqual(inspected.documentRevision, current.documentRevision)
        try m10Shutdown(host)
    }

    func testEmptyFolderAndExistingOutputAreAtomicInvalidAndReportedFailure() throws {
        let host = CoreHost()
        let directory = try makeM10Directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let current = try m10Created(host, 60)
        let input = directory.appendingPathComponent("input", isDirectory: true)
        let output = directory.appendingPathComponent("output", isDirectory: true)
        try FileManager.default.createDirectory(at: input, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        let graph = CoreBatchGraphDraft(
            name: "Empty",
            inputs: [.folder(input.path)],
            operations: [.invertColorPlane()],
            output: CoreBatchOutputSettings(policy: .newSave, folder: output.path)
        )
        XCTAssertEqual(
            try m10Outcome(host.executeBatch(
                target: current.target,
                expectedDocumentRevision: current.documentRevision,
                graph: graph,
                options: CoreBatchRunOptions(scope: .all, previewConfirmed: true)
            )),
            .failed(.invalidRequest)
        )
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: output.path).isEmpty)

        let source = try m10Created(host, 61)
        let sourceURL = input.appendingPathComponent("cell1.inkpod")
        _ = try m10File(host.save(
            target: source.target,
            expectedDocumentRevision: source.documentRevision,
            pathUTF8: Array(sourceURL.path.utf8),
            allowCleanSave: true
        ))
        let collision = output.appendingPathComponent("cell_0001.inkpod")
        let original = Data("existing-output".utf8)
        try original.write(to: collision)
        let collisionOutcome = try m10Outcome(host.executeBatch(
            target: current.target,
            expectedDocumentRevision: current.documentRevision,
            graph: graph,
            options: CoreBatchRunOptions(scope: .all, previewConfirmed: true)
        ))
        guard case let .batchReport(failed) = collisionOutcome else {
            return XCTFail("collision returned \(collisionOutcome); graph ready=\(graph.isRunReady)")
        }
        XCTAssertEqual(failed.failureCount, 1)
        XCTAssertEqual(failed.items.first?.outcome, .failed)
        XCTAssertEqual(try Data(contentsOf: collision), original)
        XCTAssertEqual(
            try m10Inspected(host.inspectSession(current.target)).documentRevision,
            current.documentRevision
        )
        try m10Shutdown(host)
    }

    func testAdmissionAndSaveFailuresDoNotPublishFiles() throws {
        let directory = try makeM10Directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let graph = CoreBatchGraphDraft(
            name: "Failure",
            inputs: [.currentSequence()],
            operations: [.invertColorPlane()]
        )
        let host = CoreHost(testConfiguration: CoreHostTestConfiguration(
            normalAdmissionFailureCount: 1
        ))
        let destination = directory.appendingPathComponent("allocation.inkbatch")
        XCTAssertEqual(
            try m10Outcome(host.saveBatchGraph(graph, pathUTF8: Array(destination.path.utf8))),
            .failed(.allocationFailed)
        )
        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))

        let missingParent = directory.appendingPathComponent("missing/failure.inkbatch")
        guard case .failed(.coreOperation(.ioError)) = try m10Outcome(
            host.saveBatchGraph(graph, pathUTF8: Array(missingParent.path.utf8))
        ) else {
            return XCTFail("save into a missing directory must report an I/O failure")
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: missingParent.path))
        try m10Shutdown(host)
    }
}

private enum M10TestFailure: Error { case unexpected(CoreRequestOutcome) }

private func m10Outcome(_ task: CoreTask, timeout: TimeInterval = 30) throws -> CoreRequestOutcome {
    guard let value = task.wait(timeout: timeout) else {
        throw M10TestFailure.unexpected(.failed(.hostStopped))
    }
    return value
}

private func m10Created(_ host: CoreHost, _ low: UInt64) throws -> CoreSessionProjection {
    let value = try m10Outcome(host.createSession(
        documentUUID: CoreDocumentUUID(high: 0x4D3130, low: low)
    ))
    guard case let .created(projection) = value else { throw M10TestFailure.unexpected(value) }
    return projection
}

private func m10Inspected(_ task: CoreTask) throws -> CoreSessionProjection {
    let value = try m10Outcome(task)
    guard case let .inspected(projection) = value else { throw M10TestFailure.unexpected(value) }
    return projection
}

private func m10File(_ task: CoreTask) throws -> CoreFileProjection {
    let value = try m10Outcome(task)
    guard case let .fileCompleted(projection) = value else { throw M10TestFailure.unexpected(value) }
    return projection
}

private func m10Preview(_ task: CoreTask) throws -> CoreBatchPreviewProjection {
    let value = try m10Outcome(task)
    guard case let .batchPreview(projection) = value else { throw M10TestFailure.unexpected(value) }
    return projection
}

private func m10Report(_ task: CoreTask) throws -> CoreBatchReportProjection {
    let value = try m10Outcome(task)
    guard case let .batchReport(projection) = value else { throw M10TestFailure.unexpected(value) }
    return projection
}

private func m10GraphSummary(_ task: CoreTask) throws -> CoreBatchGraphSummary {
    let value = try m10Outcome(task)
    guard case let .batchGraph(projection) = value else { throw M10TestFailure.unexpected(value) }
    return projection
}

private func m10Acknowledged(_ task: CoreTask) throws {
    let value = try m10Outcome(task)
    guard case .acknowledged = value else { throw M10TestFailure.unexpected(value) }
}

private func m10Shutdown(_ host: CoreHost) throws {
    let value = try m10Outcome(host.shutdown())
    guard case .shutdown = value else { throw M10TestFailure.unexpected(value) }
}

private func makeM10Directory() throws -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("inkpod-m10-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}
