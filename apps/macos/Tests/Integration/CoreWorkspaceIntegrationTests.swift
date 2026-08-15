import Foundation
import XCTest
@testable import InkpodCoreBridge

final class CoreWorkspaceIntegrationTests: XCTestCase {
    func testThreeCellPlanCommitsAllOrNoneWithoutConsumingIdentityOnFailure() throws {
        let host = CoreHost()
        let options = CoreCellCreationOptions(
            sizingMode: .imagePixels,
            width: 96,
            height: 64,
            dpiXMilli: 144_000,
            dpiYMilli: 144_000,
            marginMilli: 100,
            safeFrameRatioMilli: 900,
            maximumCloseRatioMilli: 750,
            anchor: .center,
            initialLayerKind: .binaryColoring,
            pixelFormat: .rgba16,
            count: 3
        )

        let plan = try requireCellPlan(
            try requireWorkspaceOutcome(host.prepareCellCreation(options).wait(timeout: 5))
        )
        XCTAssertEqual(plan.items.count, 3)
        XCTAssertEqual(plan.items.map(\.width), [96, 96, 96])
        XCTAssertEqual(plan.items.map(\.height), [64, 64, 64])
        XCTAssertEqual(plan.items.map(\.pixelFormat), [.rgba16, .rgba16, .rgba16])

        let invalidUUIDs = [workspaceUUID(1), CoreDocumentUUID(high: 0, low: 0), workspaceUUID(3)]
        XCTAssertEqual(
            try requireWorkspaceOutcome(
                host.commitCellCreation(plan: plan.id, documentUUIDs: invalidUUIDs)
                    .wait(timeout: 10)
            ),
            .failed(.invalidRequest)
        )

        let created = try requireCellsCreated(
            try requireWorkspaceOutcome(
                host.commitCellCreation(
                    plan: plan.id,
                    documentUUIDs: [workspaceUUID(1), workspaceUUID(2), workspaceUUID(3)]
                ).wait(timeout: 20)
            )
        )
        XCTAssertEqual(created.count, 3)
        XCTAssertEqual(created.map(\.target.id.rawValue), [1, 2, 3])
        XCTAssertEqual(Set(created.map(\.ownerThreadID)).count, 1)
        XCTAssertEqual(created.map(\.documentWidth), [96, 96, 96])
        XCTAssertEqual(created.map(\.documentHeight), [64, 64, 64])

        XCTAssertEqual(
            try requireWorkspaceOutcome(host.cancelCellCreation(plan.id).wait(timeout: 5)),
            .noOp(nil)
        )
        _ = try requireWorkspaceShutdown(host)
    }

    func testCellPlanInvalidAndCancelLeaveCoreRegistryUnchanged() throws {
        let host = CoreHost()
        let invalid = CoreCellCreationOptions(
            sizingMode: .imagePixels,
            width: 0,
            height: 64,
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
        XCTAssertEqual(
            try requireWorkspaceOutcome(host.prepareCellCreation(invalid).wait(timeout: 5)),
            .failed(.invalidRequest)
        )

        let plan = try requireCellPlan(
            try requireWorkspaceOutcome(
                host.prepareCellCreation(.defaultSingleCell).wait(timeout: 5)
            )
        )
        XCTAssertEqual(
            try requireWorkspaceOutcome(host.cancelCellCreation(plan.id).wait(timeout: 5)),
            .acknowledged
        )
        XCTAssertEqual(
            try requireWorkspaceOutcome(
                host.commitCellCreation(plan: plan.id, documentUUIDs: [workspaceUUID(9)])
                    .wait(timeout: 5)
            ),
            .failed(.staleTarget)
        )
        let created = try requireCreatedWorkspaceSession(
            try requireWorkspaceOutcome(host.createSession(documentUUID: workspaceUUID(10)).wait(timeout: 10))
        )
        XCTAssertEqual(created.target.id.rawValue, 1)
        _ = try requireWorkspaceShutdown(host)
    }

    func testSecondaryViewHasIndependentRevisionAndBecomesStaleAfterClose() throws {
        let host = CoreHost()
        let session = try requireCreatedWorkspaceSession(
            try requireWorkspaceOutcome(host.createSession(documentUUID: workspaceUUID(20)).wait(timeout: 10))
        )
        let secondary = try requireViewCreated(
            try requireWorkspaceOutcome(
                host.createView(
                    target: session.target,
                    expectedDocumentRevision: session.documentRevision
                ).wait(timeout: 5)
            )
        )
        XCTAssertEqual(secondary.session.target, session.target)
        XCTAssertNotEqual(secondary.target, session.primaryView)
        XCTAssertEqual(secondary.session.documentRevision, session.documentRevision)

        let changed = try requireLogicalViewUpdated(
            try requireWorkspaceOutcome(
                host.applyView(
                    target: secondary.target,
                    command: .panBy(deviceDX: 7, deviceDY: -3),
                    expectation: CoreCommandExpectation(
                        documentRevision: session.documentRevision,
                        viewRevision: secondary.viewRevision
                    )
                ).wait(timeout: 5)
            )
        )
        XCTAssertGreaterThan(changed.viewRevision, secondary.viewRevision)
        XCTAssertEqual(changed.session.documentRevision, session.documentRevision)

        let primary = try requireInspectedWorkspaceSession(
            try requireWorkspaceOutcome(host.inspectSession(session.target).wait(timeout: 5))
        )
        XCTAssertEqual(primary.documentRevision, session.documentRevision)
        XCTAssertEqual(primary.viewRevision, session.viewRevision)

        let route = CoreSnapshotRoute(
            session: session.target,
            view: secondary.target,
            surface: CoreSurfaceTarget(
                id: CoreSurfaceID(rawValue: 71),
                generation: CoreSurfaceGeneration(rawValue: 91)
            )
        )
        let snapshot = try requireWorkspaceSnapshot(
            try requireWorkspaceOutcome(host.buildSnapshot(route: route).wait(timeout: 5))
        )
        XCTAssertEqual(snapshot.route, route)
        XCTAssertEqual(snapshot.viewRevision, changed.viewRevision)
        snapshot.owner.release()

        XCTAssertEqual(
            try requireWorkspaceOutcome(host.closeView(secondary.target).wait(timeout: 5)),
            .viewClosed(secondary.target)
        )
        XCTAssertEqual(
            try requireWorkspaceOutcome(
                host.applyView(target: secondary.target, command: .setGridVisible(true))
                    .wait(timeout: 5)
            ),
            .failed(.staleTarget)
        )
        _ = try requireWorkspaceShutdown(host)
    }

    func testLayerPlaneTreeUsesStableTargetsAndRejectsStaleOrInvalidEdits() throws {
        let host = CoreHost()
        let session = try requireCreatedWorkspaceSession(
            try requireWorkspaceOutcome(host.createSession(documentUUID: workspaceUUID(30)).wait(timeout: 10))
        )
        let initial = try requireTree(
            try requireWorkspaceOutcome(
                host.inspectTree(
                    target: session.target,
                    expectedDocumentRevision: session.documentRevision
                ).wait(timeout: 5)
            )
        )
        XCTAssertEqual(initial.layers.count, 1)
        XCTAssertGreaterThanOrEqual(initial.layers[0].planes.count, 2)
        let main = try XCTUnwrap(initial.layers[0].planes.first { $0.kind == .mainLine })
        let color = try XCTUnwrap(initial.layers[0].planes.first { $0.kind == .color })

        XCTAssertEqual(initial.activePlaneID, main.id)

        let colorTarget = try requireTreeUpdated(
            try requireWorkspaceOutcome(
                host.setActiveNode(
                    target: session.target,
                    layerID: initial.layers[0].id,
                    planeID: color.id,
                    expectedDocumentRevision: initial.session.documentRevision
                ).wait(timeout: 5)
            )
        )
        XCTAssertEqual(colorTarget.tree.activePlaneID, color.id)
        XCTAssertEqual(colorTarget.tree.session.documentRevision, session.documentRevision)

        let mainTarget = try requireTreeUpdated(
            try requireWorkspaceOutcome(
                host.setActiveNode(
                    target: session.target,
                    layerID: initial.layers[0].id,
                    planeID: main.id,
                    expectedDocumentRevision: initial.session.documentRevision
                ).wait(timeout: 5)
            )
        )
        XCTAssertEqual(mainTarget.tree.activePlaneID, main.id)

        let createdLayer = try requireTreeUpdated(
            try requireWorkspaceOutcome(
                host.editTree(
                    target: session.target,
                    expectedDocumentRevision: initial.session.documentRevision,
                    command: .createLayer(
                        kind: .raster,
                        pixelFormat: .rgba8,
                        name: "Paint"
                    )
                ).wait(timeout: 5)
            )
        )
        XCTAssertNotNil(createdLayer.affectedObjectID)
        XCTAssertEqual(createdLayer.tree.layers.count, 2)
        let paint = try XCTUnwrap(
            createdLayer.tree.layers.first { $0.id == createdLayer.affectedObjectID }
        )

        let noOp = try requireWorkspaceOutcome(
            host.editTree(
                target: session.target,
                expectedDocumentRevision: createdLayer.tree.session.documentRevision,
                command: .setLayerProperties(
                    id: paint.id,
                    visible: paint.isVisible,
                    editable: paint.isEditable,
                    opacityMilli: paint.opacityMilli,
                    name: paint.name
                )
            ).wait(timeout: 5)
        )
        guard case .noOp = noOp else { return XCTFail("expected semantic no-op, got \(noOp)") }

        XCTAssertEqual(
            try requireWorkspaceOutcome(
                host.editTree(
                    target: session.target,
                    expectedDocumentRevision: initial.session.documentRevision,
                    command: .deleteLayer(id: paint.id)
                ).wait(timeout: 5)
            ),
            .failed(.staleTarget)
        )
        XCTAssertEqual(
            try requireWorkspaceOutcome(
                host.editTree(
                    target: session.target,
                    expectedDocumentRevision: createdLayer.tree.session.documentRevision,
                    command: .createPlane(
                        parentLayerID: paint.id,
                        kind: .selection,
                        pixelFormat: .rgba8,
                        name: "Invalid"
                    )
                ).wait(timeout: 5)
            ),
            .failed(.coreOperation(.invalidArgument))
        )
        _ = try requireWorkspaceShutdown(host)
    }
}

private enum WorkspaceIntegrationFailure: Error {
    case timedOut
    case unexpected(CoreRequestOutcome)
}

private func requireWorkspaceOutcome(_ outcome: CoreRequestOutcome?) throws -> CoreRequestOutcome {
    guard let outcome else {
        XCTFail("Core task timed out")
        throw WorkspaceIntegrationFailure.timedOut
    }
    return outcome
}

private func requireCellPlan(_ outcome: CoreRequestOutcome) throws -> CoreCellCreationPlanProjection {
    guard case let .cellPlan(plan) = outcome else {
        XCTFail("expected cell plan, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return plan
}

private func requireCellsCreated(_ outcome: CoreRequestOutcome) throws -> [CoreSessionProjection] {
    guard case let .cellsCreated(sessions) = outcome else {
        XCTFail("expected cells created, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return sessions
}

private func requireCreatedWorkspaceSession(_ outcome: CoreRequestOutcome) throws -> CoreSessionProjection {
    guard case let .created(session) = outcome else {
        XCTFail("expected created, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return session
}

private func requireInspectedWorkspaceSession(_ outcome: CoreRequestOutcome) throws -> CoreSessionProjection {
    guard case let .inspected(session) = outcome else {
        XCTFail("expected inspected, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return session
}

private func requireViewCreated(_ outcome: CoreRequestOutcome) throws -> CoreLogicalViewProjection {
    guard case let .viewCreated(view) = outcome else {
        XCTFail("expected view created, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return view
}

private func requireLogicalViewUpdated(_ outcome: CoreRequestOutcome) throws -> CoreLogicalViewProjection {
    guard case let .logicalViewUpdated(view) = outcome else {
        XCTFail("expected logical view updated, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return view
}

private func requireWorkspaceSnapshot(_ outcome: CoreRequestOutcome) throws -> CoreSnapshotEnvelope {
    guard case let .snapshot(snapshot) = outcome else {
        XCTFail("expected snapshot, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return snapshot
}

private func requireTree(_ outcome: CoreRequestOutcome) throws -> CoreTreeProjection {
    guard case let .tree(tree) = outcome else {
        XCTFail("expected tree, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return tree
}

private func requireTreeUpdated(_ outcome: CoreRequestOutcome) throws -> CoreTreeMutationProjection {
    guard case let .treeUpdated(update) = outcome else {
        XCTFail("expected tree update, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return update
}

private func requireWorkspaceShutdown(_ host: CoreHost) throws -> CoreShutdownProjection {
    let outcome = try requireWorkspaceOutcome(host.shutdown().wait(timeout: 20))
    guard case let .shutdown(shutdown) = outcome else {
        XCTFail("expected shutdown, got \(outcome)")
        throw WorkspaceIntegrationFailure.unexpected(outcome)
    }
    return shutdown
}

private func workspaceUUID(_ value: UInt64) -> CoreDocumentUUID {
    CoreDocumentUUID(high: 0x4D35, low: value)
}
