import XCTest
@testable import InkpodCoreBridge

final class CanvasCoreIntegrationTests: XCTestCase {
    func testViewUpdatesAreViewOnlyAndRejectInvalidOrStaleTargets() throws {
        let host = CoreHost()
        let created = try requireCreated(
            host.createSession(documentUUID: .init(high: 0xA201, low: 1))
        )

        let resized = try requireViewUpdated(
            host.applyView(
                target: created.primaryView,
                command: .viewportResized(width: 800, height: 600)
            )
        )
        XCTAssertEqual(resized.documentRevision, created.documentRevision)
        XCTAssertGreaterThan(resized.viewRevision, created.viewRevision)

        guard case let .noOp(unchanged?) = requireOutcome(
            host.applyView(
                target: created.primaryView,
                command: .viewportResized(width: 800, height: 600)
            )
        )
        else {
            throw TestFailure.unexpectedOutcome
        }
        XCTAssertEqual(unchanged.documentRevision, created.documentRevision)
        XCTAssertEqual(unchanged.viewRevision, resized.viewRevision)

        XCTAssertEqual(
            requireOutcome(
                host.applyView(
                    target: created.primaryView,
                    command: .zoomAt(factor: .nan, deviceX: 10, deviceY: 10)
                )
            ),
            .failed(.coreOperation(.invalidArgument))
        )

        _ = requireOutcome(host.closeSession(created.target))
        XCTAssertEqual(
            requireOutcome(
                host.applyView(
                    target: created.primaryView,
                    command: .panBy(deviceDX: 1, deviceDY: 1)
                )
            ),
            .failed(.staleTarget)
        )
        shutdown(host)
    }

    func testStrokeSuccessCancelInvalidAndUndoAreAtomic() throws {
        let host = CoreHost()
        let created = try requireCreated(
            host.createSession(documentUUID: .init(high: 0xA202, low: 1))
        )
        _ = try requireViewUpdated(
            host.applyView(
                target: created.primaryView,
                command: .viewportResized(width: 1_024, height: 1_024)
            )
        )
        let before = try requireInspected(host.inspectSession(created.target))

        XCTAssertEqual(
            requireOutcome(
                host.beginPencilStroke(
                    target: created.primaryView,
                    samples: [.init(deviceX: 8, deviceY: 8, pressure: 1)]
                )
            ),
            .acknowledged
        )
        XCTAssertEqual(
            requireOutcome(host.cancelStroke(target: created.primaryView)),
            .acknowledged
        )
        let cancelled = try requireInspected(host.inspectSession(created.target))
        XCTAssertEqual(cancelled.documentRevision, before.documentRevision)
        XCTAssertFalse(cancelled.canUndo)
        XCTAssertFalse(cancelled.hasActiveTransient)

        let rapidBegin = host.beginPencilStroke(
            target: created.primaryView,
            samples: [.init(deviceX: 9, deviceY: 9, pressure: 1)]
        )
        let rapidCancel = host.cancelStroke(target: created.primaryView)
        XCTAssertEqual(requireOutcome(rapidBegin), .acknowledged)
        XCTAssertEqual(requireOutcome(rapidCancel), .acknowledged)
        let rapidlyCancelled = try requireInspected(host.inspectSession(created.target))
        XCTAssertEqual(rapidlyCancelled.documentRevision, before.documentRevision)
        XCTAssertFalse(rapidlyCancelled.hasActiveTransient)

        XCTAssertEqual(
            requireOutcome(
                host.beginPencilStroke(
                    target: created.primaryView,
                    samples: [.init(deviceX: -.infinity, deviceY: 1, pressure: 1)]
                )
            ),
            .failed(.invalidRequest)
        )
        let afterInvalid = try requireInspected(host.inspectSession(created.target))
        XCTAssertEqual(afterInvalid.documentRevision, before.documentRevision)
        XCTAssertFalse(afterInvalid.canUndo)

        XCTAssertEqual(
            requireOutcome(
                host.beginPencilStroke(
                    target: created.primaryView,
                    samples: [.init(deviceX: 12, deviceY: 12, pressure: 1)]
                )
            ),
            .acknowledged
        )
        XCTAssertEqual(
            requireOutcome(
                host.appendPencilStroke(
                    target: created.primaryView,
                    samples: [.init(deviceX: .nan, deviceY: 13, pressure: 1)]
                )
            ),
            .failed(.invalidRequest)
        )
        let afterInvalidAppend = try requireInspected(host.inspectSession(created.target))
        XCTAssertEqual(afterInvalidAppend.documentRevision, before.documentRevision)
        XCTAssertFalse(afterInvalidAppend.canUndo)
        XCTAssertFalse(afterInvalidAppend.hasActiveTransient)

        XCTAssertEqual(
            requireOutcome(
                host.beginPencilStroke(
                    target: created.primaryView,
                    samples: [.init(deviceX: 16, deviceY: 16, pressure: 0.5)]
                )
            ),
            .acknowledged
        )
        XCTAssertEqual(
            requireOutcome(
                host.appendPencilStroke(
                    target: created.primaryView,
                    samples: [
                        .init(deviceX: 17, deviceY: 17, pressure: 0.75),
                        .init(deviceX: 18, deviceY: 18, pressure: 1),
                    ]
                )
            ),
            .acknowledged
        )
        let committed = try requireDocumentUpdated(
            host.endStroke(target: created.primaryView)
        )
        XCTAssertEqual(committed.documentRevision, before.documentRevision + 1)
        XCTAssertTrue(committed.canUndo)
        XCTAssertFalse(committed.hasActiveTransient)

        let undone = try requireDocumentUpdated(host.undo(target: created.target))
        XCTAssertFalse(undone.canUndo)
        XCTAssertTrue(undone.canRedo)
        XCTAssertNotEqual(undone.documentRevision, committed.documentRevision)
        shutdown(host)
    }

    func testSnapshotQueueReleasesRejectReplaceCloseAndShutdownExactlyOnce() throws {
        let host = CoreHost()
        let created = try requireCreated(
            host.createSession(documentUUID: .init(high: 0xA203, low: 1))
        )
        let route = CoreSnapshotRoute(
            session: created.target,
            view: created.primaryView,
            surface: .init(id: .init(rawValue: 71), generation: .init(rawValue: 3))
        )
        let queue = SnapshotOwnershipQueue(capacity: 2)
        XCTAssertTrue(queue.registerSurface(route.surface, binding: route))

        let rejected = try requireSnapshot(
            host.buildSnapshot(
                route: .init(
                    session: route.session,
                    view: route.view,
                    surface: .init(id: route.surface.id, generation: .init(rawValue: 2))
                )
            )
        )
        XCTAssertEqual(queue.submit(rejected), .rejectedStaleRoute)
        XCTAssertEqual(rejected.owner.ffiReleaseCount, 1)

        let first = try requireSnapshot(host.buildSnapshot(route: route))
        let replacement = try requireSnapshot(host.buildSnapshot(route: route))
        XCTAssertEqual(queue.submit(first), .accepted)
        XCTAssertEqual(queue.submit(replacement), .replacedPending)
        XCTAssertEqual(first.owner.ffiReleaseCount, 1)
        XCTAssertEqual(replacement.owner.ffiReleaseCount, 0)

        let retained = try XCTUnwrap(queue.takeNext())
        XCTAssertEqual(queue.retainRendered(retained), .retained)
        XCTAssertEqual(replacement.owner.ffiReleaseCount, 0)
        queue.setSurfaceVisible(route.surface, visible: false)
        let hidden = try requireSnapshot(host.buildSnapshot(route: route))
        XCTAssertEqual(queue.submit(hidden), .rejectedHidden)
        XCTAssertEqual(hidden.owner.ffiReleaseCount, 1)

        queue.closeSurface(route.surface)
        XCTAssertEqual(replacement.owner.ffiReleaseCount, 1)
        queue.shutdown()
        XCTAssertEqual(replacement.owner.ffiReleaseCount, 1)
        shutdown(host)
    }

    func testBackingPixelNormalizationUsesHalfOpenBoundsAndInputFallbacks() {
        XCTAssertEqual(
            CanvasInputNormalizer.localDevicePoint(
                backingPoint: .init(x: 199, y: -99),
                backingBounds: .init(x: 0, y: -100, width: 200, height: 100),
                isFlipped: true
            ),
            .init(x: 199, y: 99)
        )
        XCTAssertEqual(
            CanvasInputNormalizer.localDevicePoint(
                backingPoint: .init(x: 199, y: 99),
                backingBounds: .init(x: 0, y: 0, width: 200, height: 100),
                isFlipped: false
            ),
            .init(x: 199, y: 99)
        )
        XCTAssertNil(
            CanvasInputNormalizer.localDevicePoint(
                backingPoint: .init(x: 200, y: -99),
                backingBounds: .init(x: 0, y: -100, width: 200, height: 100),
                isFlipped: true
            )
        )
        XCTAssertEqual(
            CanvasInputNormalizer.sample(
                deviceX: 199,
                deviceY: 99,
                drawableWidth: 200,
                drawableHeight: 100,
                pressure: nil,
                tilt: nil
            ),
            .init(deviceX: 199, deviceY: 99, pressure: 1, tiltX: 0, tiltY: 0)
        )
        XCTAssertNil(
            CanvasInputNormalizer.sample(
                deviceX: 200,
                deviceY: 99,
                drawableWidth: 200,
                drawableHeight: 100,
                pressure: 0.4,
                tilt: .init(x: 0.2, y: -0.2)
            )
        )
        XCTAssertNil(
            CanvasInputNormalizer.sample(
                deviceX: 1,
                deviceY: .nan,
                drawableWidth: 200,
                drawableHeight: 100,
                pressure: nil,
                tilt: nil
            )
        )
    }

    func testStrokeBoundariesUseReservedOrderedInputCapacity() {
        let mailbox = CoreMailbox(normalAdmissionFailureCount: 0)
        let session = CoreSessionTarget(
            id: .init(rawValue: 1),
            generation: .init(rawValue: 1)
        )
        let view = CoreViewTarget(
            session: session,
            id: .init(rawValue: 1),
            generation: .init(rawValue: 1)
        )
        let sample = CorePointerSample(deviceX: 1, deviceY: 1, pressure: 1)

        for rawID in 1 ... CoreMailbox.inputSampleCapacity {
            XCTAssertEqual(
                mailbox.enqueue(
                    .init(
                        requestID: .init(rawValue: UInt64(rawID)),
                        request: .appendRasterStroke(view, [sample])
                    ),
                    lane: .inputSample
                ),
                .accepted
            )
        }
        XCTAssertEqual(
            mailbox.enqueue(
                .init(
                    requestID: .init(rawValue: 50_000),
                    request: .appendRasterStroke(view, [sample])
                ),
                lane: .inputSample
            ),
            .queueFull
        )
        XCTAssertEqual(
            mailbox.enqueue(
                .init(
                    requestID: .init(rawValue: 50_001),
                    request: .endStroke(view)
                ),
                lane: .inputBoundary
            ),
            .accepted
        )
        XCTAssertEqual(
            mailbox.drainAndStop().count,
            CoreMailbox.inputSampleCapacity + 1
        )
    }

    private func requireOutcome(
        _ task: CoreTask,
        file: StaticString = #filePath,
        line: UInt = #line
    ) -> CoreRequestOutcome {
        guard let outcome = task.wait(timeout: 10) else {
            XCTFail("Core task timed out", file: file, line: line)
            return .failed(.cancelled)
        }
        return outcome
    }

    private func requireCreated(_ task: CoreTask) throws -> CoreSessionProjection {
        guard case let .created(projection) = requireOutcome(task) else {
            throw TestFailure.unexpectedOutcome
        }
        return projection
    }

    private func requireInspected(_ task: CoreTask) throws -> CoreSessionProjection {
        guard case let .inspected(projection) = requireOutcome(task) else {
            throw TestFailure.unexpectedOutcome
        }
        return projection
    }

    private func requireViewUpdated(_ task: CoreTask) throws -> CoreSessionProjection {
        guard case let .viewUpdated(projection) = requireOutcome(task) else {
            throw TestFailure.unexpectedOutcome
        }
        return projection
    }

    private func requireDocumentUpdated(_ task: CoreTask) throws -> CoreSessionProjection {
        guard case let .documentUpdated(projection) = requireOutcome(task) else {
            throw TestFailure.unexpectedOutcome
        }
        return projection
    }

    private func requireSnapshot(_ task: CoreTask) throws -> CoreSnapshotEnvelope {
        guard case let .snapshot(envelope) = requireOutcome(task) else {
            throw TestFailure.unexpectedOutcome
        }
        return envelope
    }

    private func shutdown(_ host: CoreHost) {
        _ = requireOutcome(host.shutdown())
        XCTAssertTrue(host.waitUntilStopped(timeout: 10))
    }
}

private enum TestFailure: Error {
    case unexpectedOutcome
}
