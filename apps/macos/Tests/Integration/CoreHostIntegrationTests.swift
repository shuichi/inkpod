import Darwin
import Foundation
import XCTest
@testable import InkpodCoreBridge

final class CoreHostIntegrationTests: XCTestCase {
    func testSixtyFourSessionsShareOneOwnerThreadAndDestroyInStableOrder() throws {
        let host = CoreHost()
        let callerThreadID = currentTestThreadID()
        var sessions: [CoreSessionProjection] = []

        for index in 1...64 {
            let outcome = try requireOutcome(
                host.createSession(documentUUID: testUUID(index)).wait(timeout: 20)
            )
            sessions.append(try requireCreated(outcome))
        }

        XCTAssertEqual(Set(sessions.map(\.ownerThreadID)).count, 1)
        XCTAssertNotEqual(sessions[0].ownerThreadID, callerThreadID)
        XCTAssertEqual(sessions.map(\.target.id.rawValue), Array(1...64).map(UInt64.init))
        XCTAssertEqual(sessions.map(\.target.generation.rawValue), Array(1...64).map(UInt64.init))

        let overflow = try requireOutcome(
            host.createSession(documentUUID: testUUID(65)).wait(timeout: 5)
        )
        XCTAssertEqual(overflow, .failed(.sessionLimit))

        for session in sessions {
            let inspected = try requireInspected(
                try requireOutcome(host.inspectSession(session.target).wait(timeout: 5))
            )
            XCTAssertEqual(inspected.ownerThreadID, sessions[0].ownerThreadID)
            XCTAssertEqual(inspected.target, session.target)
        }

        let shutdown = try requireShutdown(
            try requireOutcome(host.shutdown().wait(timeout: 20))
        )
        XCTAssertEqual(shutdown.destroyedSessionIDs, sessions.map(\.target.id))
        XCTAssertTrue(host.waitUntilStopped(timeout: 5))
    }

    func testSuccessNoOpInvalidAndStaleNeverRetargetAnotherSession() throws {
        let host = CoreHost()
        let first = try requireCreated(
            try requireOutcome(host.createSession(documentUUID: testUUID(1)).wait(timeout: 10))
        )
        let second = try requireCreated(
            try requireOutcome(host.createSession(documentUUID: testUUID(2)).wait(timeout: 10))
        )

        let duplicate = try requireOutcome(
            host.createSession(documentUUID: testUUID(1)).wait(timeout: 5)
        )
        XCTAssertEqual(duplicate, .noOp(first))

        let invalid = CoreSessionTarget(
            id: CoreSessionID(rawValue: UInt64.max),
            generation: CoreSessionGeneration(rawValue: UInt64.max)
        )
        XCTAssertEqual(
            try requireOutcome(host.inspectSession(invalid).wait(timeout: 5)),
            .failed(.invalidTarget)
        )

        let stale = CoreSessionTarget(
            id: first.target.id,
            generation: CoreSessionGeneration(rawValue: first.target.generation.rawValue + 1)
        )
        XCTAssertEqual(
            try requireOutcome(host.inspectSession(stale).wait(timeout: 5)),
            .failed(.staleTarget)
        )
        XCTAssertEqual(
            try requireInspected(
                try requireOutcome(host.inspectSession(second.target).wait(timeout: 5))
            ).target,
            second.target
        )

        let closed = try requireClosed(
            try requireOutcome(host.closeSession(first.target).wait(timeout: 5))
        )
        XCTAssertEqual(closed.target, first.target)
        XCTAssertFalse(closed.cancelledActiveTransient)
        XCTAssertEqual(
            try requireOutcome(host.inspectSession(first.target).wait(timeout: 5)),
            .failed(.staleTarget)
        )
        XCTAssertEqual(
            try requireOutcome(host.closeSession(first.target).wait(timeout: 5)),
            .noOp(nil)
        )

        _ = try requireShutdown(try requireOutcome(host.shutdown().wait(timeout: 10)))
        XCTAssertTrue(host.waitUntilStopped(timeout: 5))
    }

    func testCommandExpectationsRejectStaleAndPreserveNoOpRevisions() throws {
        let host = CoreHost()
        let created = try requireCreated(
            try requireOutcome(host.createSession(documentUUID: testUUID(71)).wait(timeout: 10))
        )
        let initialExpectation = CoreCommandExpectation(
            documentRevision: created.documentRevision,
            viewRevision: created.viewRevision
        )

        let noOp = try requireOutcome(host.applyView(
            target: created.primaryView,
            command: .setGridVisible(false),
            expectation: initialExpectation
        ).wait(timeout: 5))
        XCTAssertEqual(noOp, .noOp(created))

        let changed = try requireOutcome(host.applyView(
            target: created.primaryView,
            command: .setGridVisible(true),
            expectation: initialExpectation
        ).wait(timeout: 5))
        guard case let .viewUpdated(updated) = changed else {
            return XCTFail("expected a view update, got \(changed)")
        }
        XCTAssertEqual(updated.documentRevision, created.documentRevision)
        XCTAssertGreaterThan(updated.viewRevision, created.viewRevision)

        XCTAssertEqual(
            try requireOutcome(host.applyView(
                target: created.primaryView,
                command: .setRulerVisible(true),
                expectation: initialExpectation
            ).wait(timeout: 5)),
            .failed(.staleTarget)
        )
        XCTAssertEqual(
            try requireOutcome(host.applyDocument(
                target: created.target,
                command: .setGrid(CoreGridDefinition(
                    originX: 0,
                    originY: 0,
                    spacingX: 0,
                    spacingY: 16,
                    subdivisions: 1
                )),
                expectedDocumentRevision: created.documentRevision
            ).wait(timeout: 5)),
            .failed(.invalidRequest)
        )

        _ = try requireShutdown(try requireOutcome(host.shutdown().wait(timeout: 10)))
        XCTAssertTrue(host.waitUntilStopped(timeout: 5))
    }

    func testCreateFailureRollsBackWithoutConsumingSessionIdentity() throws {
        let host = CoreHost(
            testConfiguration: CoreHostTestConfiguration(createABIMismatchCount: 1)
        )
        let failedTask = host.createSession(documentUUID: testUUID(1))
        XCTAssertEqual(
            try requireOutcome(failedTask.wait(timeout: 5)),
            .failed(.coreCreate(.incompatibleABI))
        )
        XCTAssertEqual(failedTask.completionCount, 1)

        let createdTask = host.createSession(documentUUID: testUUID(2))
        let created = try requireCreated(try requireOutcome(createdTask.wait(timeout: 10)))
        XCTAssertEqual(created.target.id.rawValue, 1)
        XCTAssertEqual(created.target.generation.rawValue, 1)
        XCTAssertEqual(createdTask.completionCount, 1)

        _ = try requireShutdown(try requireOutcome(host.shutdown().wait(timeout: 10)))
        XCTAssertTrue(host.waitUntilStopped(timeout: 5))
    }

    func testAdmissionAllocationFailureAndCancellationCompleteExactlyOnce() throws {
        let host = CoreHost(
            testConfiguration: CoreHostTestConfiguration(normalAdmissionFailureCount: 1)
        )
        let allocationFailure = host.createSession(documentUUID: testUUID(1))
        XCTAssertEqual(
            try requireOutcome(allocationFailure.wait(timeout: 5)),
            .failed(.allocationFailed)
        )
        XCTAssertEqual(allocationFailure.completionCount, 1)

        let created = try requireCreated(
            try requireOutcome(host.createSession(documentUUID: testUUID(2)).wait(timeout: 10))
        )
        try requireAcknowledged(
            try requireOutcome(host.setNormalProcessingEnabledForTesting(false).wait(timeout: 5))
        )

        let pending = host.inspectSession(created.target)
        let cancel = host.cancel(request: pending.requestID)
        XCTAssertEqual(
            try requireOutcome(pending.wait(timeout: 5)),
            .failed(.cancelled)
        )
        try requireAcknowledged(try requireOutcome(cancel.wait(timeout: 5)))

        try requireAcknowledged(
            try requireOutcome(host.setNormalProcessingEnabledForTesting(true).wait(timeout: 5))
        )
        XCTAssertEqual(pending.completionCount, 1)
        XCTAssertEqual(cancel.completionCount, 1)

        _ = try requireShutdown(try requireOutcome(host.shutdown().wait(timeout: 10)))
        XCTAssertTrue(host.waitUntilStopped(timeout: 5))
    }

    func testNormalSaturationPreservesControlReserveAndCloseCancelsActiveStroke() throws {
        let host = CoreHost()
        let session = try requireCreated(
            try requireOutcome(host.createSession(documentUUID: testUUID(1)).wait(timeout: 10))
        )
        try requireAcknowledged(
            try requireOutcome(host.beginTransientForTesting(session.target).wait(timeout: 5))
        )
        try requireAcknowledged(
            try requireOutcome(host.setNormalProcessingEnabledForTesting(false).wait(timeout: 5))
        )

        let pending = (0..<4_096).map { _ in host.inspectSession(session.target) }
        let overflow = host.inspectSession(session.target)
        XCTAssertEqual(
            try requireOutcome(overflow.wait(timeout: 5)),
            .failed(.queueFull)
        )

        let closed = try requireClosed(
            try requireOutcome(host.closeSession(session.target).wait(timeout: 10))
        )
        XCTAssertTrue(closed.cancelledActiveTransient)
        try requireAcknowledged(
            try requireOutcome(host.setNormalProcessingEnabledForTesting(true).wait(timeout: 5))
        )

        for task in pending {
            XCTAssertEqual(
                try requireOutcome(task.wait(timeout: 10)),
                .failed(.staleTarget)
            )
            XCTAssertEqual(task.completionCount, 1)
        }

        _ = try requireShutdown(try requireOutcome(host.shutdown().wait(timeout: 10)))
        XCTAssertTrue(host.waitUntilStopped(timeout: 5))
    }

    func testShutdownCancelsQueuedWorkRejectsLateInputAndIsIdempotent() throws {
        let host = CoreHost()
        var sessions: [CoreSessionProjection] = []
        for index in 1...4 {
            sessions.append(
                try requireCreated(
                    try requireOutcome(
                        host.createSession(documentUUID: testUUID(index)).wait(timeout: 10)
                    )
                )
            )
        }
        try requireAcknowledged(
            try requireOutcome(host.setNormalProcessingEnabledForTesting(false).wait(timeout: 5))
        )
        let queued = (0..<128).map { index in
            host.inspectSession(sessions[index % sessions.count].target)
        }

        let shutdownTask = host.shutdown()
        let lateInput = host.inspectSession(sessions[0].target)
        let repeatedShutdown = host.shutdown()
        let shutdown = try requireShutdown(
            try requireOutcome(shutdownTask.wait(timeout: 10))
        )
        XCTAssertEqual(shutdown.destroyedSessionIDs, sessions.map(\.target.id))

        for task in queued {
            XCTAssertEqual(
                try requireOutcome(task.wait(timeout: 5)),
                .failed(.cancelled)
            )
            XCTAssertEqual(task.completionCount, 1)
        }
        XCTAssertEqual(
            try requireOutcome(lateInput.wait(timeout: 5)),
            .failed(.hostStopped)
        )
        XCTAssertEqual(
            try requireOutcome(repeatedShutdown.wait(timeout: 5)),
            .noOp(nil)
        )
        XCTAssertEqual(shutdownTask.completionCount, 1)
        XCTAssertTrue(host.waitUntilStopped(timeout: 5))
    }

    func testAsyncContinuationResumesExactlyOnce() throws {
        let host = CoreHost()
        try requireAcknowledged(
            try requireOutcome(host.setNormalProcessingEnabledForTesting(false).wait(timeout: 5))
        )
        let task = host.createSession(documentUUID: testUUID(1))
        let box = AsyncOutcomeBox()
        Task.detached {
            box.complete(await task.value())
        }

        try requireAcknowledged(
            try requireOutcome(host.setNormalProcessingEnabledForTesting(true).wait(timeout: 5))
        )
        _ = try requireCreated(try requireOutcome(box.wait(timeout: 10)))
        XCTAssertEqual(task.completionCount, 1)

        _ = try requireShutdown(try requireOutcome(host.shutdown().wait(timeout: 10)))
        XCTAssertTrue(host.waitUntilStopped(timeout: 5))
    }
}

private enum IntegrationTestFailure: Error {
    case timedOut
    case unexpectedOutcome(CoreRequestOutcome)
}

private func requireOutcome(_ outcome: CoreRequestOutcome?) throws -> CoreRequestOutcome {
    guard let outcome else {
        XCTFail("Core task timed out")
        throw IntegrationTestFailure.timedOut
    }
    return outcome
}

private func requireCreated(_ outcome: CoreRequestOutcome) throws -> CoreSessionProjection {
    guard case let .created(session) = outcome else {
        XCTFail("expected created, got \(outcome)")
        throw IntegrationTestFailure.unexpectedOutcome(outcome)
    }
    return session
}

private func requireInspected(_ outcome: CoreRequestOutcome) throws -> CoreSessionProjection {
    guard case let .inspected(session) = outcome else {
        XCTFail("expected inspected, got \(outcome)")
        throw IntegrationTestFailure.unexpectedOutcome(outcome)
    }
    return session
}

private func requireClosed(_ outcome: CoreRequestOutcome) throws -> CoreSessionCloseProjection {
    guard case let .closed(projection) = outcome else {
        XCTFail("expected closed, got \(outcome)")
        throw IntegrationTestFailure.unexpectedOutcome(outcome)
    }
    return projection
}

private func requireShutdown(_ outcome: CoreRequestOutcome) throws -> CoreShutdownProjection {
    guard case let .shutdown(projection) = outcome else {
        XCTFail("expected shutdown, got \(outcome)")
        throw IntegrationTestFailure.unexpectedOutcome(outcome)
    }
    return projection
}

private func requireAcknowledged(_ outcome: CoreRequestOutcome) throws {
    guard outcome == .acknowledged else {
        XCTFail("expected acknowledged, got \(outcome)")
        throw IntegrationTestFailure.unexpectedOutcome(outcome)
    }
}

private func testUUID(_ value: Int) -> CoreDocumentUUID {
    CoreDocumentUUID(high: 0x4D31, low: UInt64(value))
}

private func currentTestThreadID() -> UInt64 {
    UInt64(pthread_mach_thread_np(pthread_self()))
}

private final class AsyncOutcomeBox: @unchecked Sendable {
    private let condition = NSCondition()
    private var outcome: CoreRequestOutcome?

    func complete(_ outcome: CoreRequestOutcome) {
        condition.lock()
        precondition(self.outcome == nil)
        self.outcome = outcome
        condition.broadcast()
        condition.unlock()
    }

    func wait(timeout: TimeInterval) -> CoreRequestOutcome? {
        let deadline = Date(timeIntervalSinceNow: timeout)
        condition.lock()
        while outcome == nil, condition.wait(until: deadline) {}
        let result = outcome
        condition.unlock()
        return result
    }
}
