import Foundation
import XCTest
@testable import InkpodCoreBridge

final class CoreM8IntegrationTests: XCTestCase {
    func testM8DeviceCoordinatesResolveOnCoreAndRejectStaleView() throws {
        let host = CoreHost()
        let created = try requireCreated(host.createSession(documentUUID: m8UUID(5)))
        guard case let .viewUpdated(viewport) = try requireOutcome(host.applyView(
            target: created.primaryView,
            command: .viewportResized(width: 800, height: 600),
            expectation: .init(
                documentRevision: created.documentRevision,
                viewRevision: created.viewRevision
            )
        )) else {
            throw M8TestFailure.unexpected(.failed(.coreOperation(.panic)))
        }
        let samples = [CorePointerSample(
            deviceX: 12,
            deviceY: 24,
            pressure: 1
        )]
        guard case let .documentPoints(points) = try requireOutcome(
            host.resolveDocumentPoints(
                target: viewport.primaryView,
                expectedDocumentRevision: viewport.documentRevision,
                expectedViewRevision: viewport.viewRevision,
                samples: samples
            )
        ) else {
            throw M8TestFailure.unexpected(.failed(.coreOperation(.panic)))
        }
        XCTAssertEqual(points.count, 1)
        XCTAssertTrue(points[0].x.isFinite)
        XCTAssertTrue(points[0].y.isFinite)

        guard case .viewUpdated = try requireOutcome(host.applyView(
            target: viewport.primaryView,
            command: .flipHorizontal,
            expectation: .init(
                documentRevision: viewport.documentRevision,
                viewRevision: viewport.viewRevision
            )
        )) else {
            throw M8TestFailure.unexpected(.failed(.coreOperation(.panic)))
        }
        XCTAssertEqual(
            try requireOutcome(host.resolveDocumentPoints(
                target: viewport.primaryView,
                expectedDocumentRevision: viewport.documentRevision,
                expectedViewRevision: viewport.viewRevision,
                samples: samples
            )),
            .failed(.staleTarget)
        )
        try shutdown(host)
    }

    func testAdjustmentSnapshotPublishesOrderedLUTPassWithoutChangingFormat() throws {
        let host = CoreHost()
        let created = try requireCreated(host.createSession(documentUUID: m8UUID(4)))
        let tree = try requireTree(host.inspectTree(target: created.target))
        let plane = try XCTUnwrap(tree.layers.flatMap(\.planes).first { $0.kind == .color })
        let adjustmentOutcome = try requireOutcome(host.performM8(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .createAdjustment(
                .init(kind: .brightnessContrast, planeID: plane.id, parameters: [100, 50]),
                name: "Exposure"
            )
        ))
        guard case let .m8Mutation(adjusted) = adjustmentOutcome else {
            throw M8TestFailure.step("create adjustment", adjustmentOutcome)
        }
        let layerUpdate = try requireTreeUpdate(host.editTree(
            target: created.target,
            expectedDocumentRevision: adjusted.state.session.documentRevision,
            command: .createLayer(kind: .annotation, pixelFormat: .none, name: "Notes")
        ))
        let annotationLayer = try XCTUnwrap(
            layerUpdate.tree.layers.first { $0.id == layerUpdate.affectedObjectID }
        )
        let contentOutcome = try requireOutcome(host.performM8(
            target: created.target,
            expectedDocumentRevision: layerUpdate.tree.session.documentRevision,
            command: .annotation([.create(.init(
                kind: .text,
                layerID: annotationLayer.id,
                output: .normal,
                bounds: .init(x: 2, y: 2, width: 80, height: 24),
                fontFamily: "Helvetica",
                fontSizeMilli: 12_000,
                text: "Exposure"
            ))])
        ))
        guard case let .m8Mutation(content) = contentOutcome else {
            throw M8TestFailure.step("create ordered content", contentOutcome)
        }
        let route = CoreSnapshotRoute(
            session: created.target,
            view: content.state.session.primaryView,
            surface: CoreSurfaceTarget(
                id: CoreSurfaceID(rawValue: 804),
                generation: CoreSurfaceGeneration(rawValue: 1)
            )
        )
        let snapshot = try requireM8Snapshot(host.buildSnapshot(route: route))
        try snapshot.owner.withBorrowedRenderView { view in
            XCTAssertTrue(view.renderPasses.contains { $0.kind == 5 })
            XCTAssertEqual(view.adjustmentLUTs.first?.count, 768)
        }
        snapshot.owner.release()
        try shutdown(host)
    }

    func testFilterPreviewCancelAndApplyAreAtomicAndRejectStaleTargets() throws {
        let host = CoreHost()
        let created = try requireCreated(host.createSession(documentUUID: m8UUID(1)))
        let tree = try requireTree(host.inspectTree(target: created.target))
        let colorPlane = try XCTUnwrap(
            tree.layers.flatMap(\.planes).first { $0.kind == .color }
        )
        let geometry = CoreGeometryRequest(
            primitive: .rectangle,
            planeID: colorPlane.id,
            fillColor: .rgba8(red: 80, green: 120, blue: 160),
            options: .init(outline: false, fill: true),
            points: [.init(x: 4, y: 4), .init(x: 24, y: 24)]
        )
        let painted = try requireM8Mutation(host.performM8(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .applyGeometry(geometry)
        ))

        let filter = CoreFilterRequest(kind: .invert, planeID: colorPlane.id)
        let preview = try requireFilterPreview(host.performM8(
            target: created.target,
            expectedDocumentRevision: painted.state.session.documentRevision,
            command: .beginFilterPreview(filter)
        ))
        XCTAssertEqual(preview.session.documentRevision, painted.state.session.documentRevision)
        XCTAssertNotEqual(preview.baseChecksum, preview.previewChecksum)

        let updated = try requireFilterPreview(host.performM8(
            target: created.target,
            expectedDocumentRevision: preview.session.documentRevision,
            command: .updateFilterPreview(.init(kind: .blurWeak, planeID: colorPlane.id))
        ))
        XCTAssertEqual(updated.baseChecksum, preview.baseChecksum)
        XCTAssertGreaterThan(updated.previewRevision, preview.previewRevision)

        let cancelled = try requireM8State(host.performM8(
            target: created.target,
            expectedDocumentRevision: updated.session.documentRevision,
            command: .cancelFilterPreview
        ))
        XCTAssertEqual(cancelled.session.documentRevision, updated.session.documentRevision)
        XCTAssertFalse(cancelled.session.hasActiveTransient)

        _ = try requireFilterPreview(host.performM8(
            target: created.target,
            expectedDocumentRevision: cancelled.session.documentRevision,
            command: .beginFilterPreview(filter)
        ))
        let applied = try requireM8Mutation(host.performM8(
            target: created.target,
            expectedDocumentRevision: cancelled.session.documentRevision,
            command: .applyFilterPreview
        ))
        XCTAssertGreaterThan(
            applied.state.session.documentRevision,
            cancelled.session.documentRevision
        )
        XCTAssertTrue(applied.state.session.canUndo)

        XCTAssertEqual(
            try requireOutcome(host.performM8(
                target: created.target,
                expectedDocumentRevision: cancelled.session.documentRevision,
                command: .applyLastFilter(planeID: colorPlane.id)
            )),
            .failed(.staleTarget)
        )
        XCTAssertEqual(
            try requireOutcome(host.performM8(
                target: created.target,
                expectedDocumentRevision: applied.state.session.documentRevision,
                command: .beginFilterPreview(.init(kind: .invert, planeID: 0))
            )),
            .failed(.invalidRequest)
        )
        try shutdown(host)
    }

    func testVectorGeometryPreviewCancelCommitAndSelectionUseCoreTopology() throws {
        let host = CoreHost()
        let created = try requireCreated(host.createSession(documentUUID: m8UUID(2)))
        let layerUpdate = try requireTreeUpdate(host.editTree(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .createLayer(kind: .vectorColoring, pixelFormat: .rgba8, name: "Vector")
        ))
        let vectorLayer = try XCTUnwrap(
            layerUpdate.tree.layers.first { $0.id == layerUpdate.affectedObjectID }
        )
        let vectorPlane = try XCTUnwrap(
            vectorLayer.planes.first { $0.kind == .vectorMainLine }
        )
        let request = CoreGeometryRequest(
            primitive: .line,
            planeID: vectorPlane.id,
            baseRevision: layerUpdate.tree.session.documentRevision,
            outlineColor: .rgba8(red: 10, green: 20, blue: 30),
            outlineWidth: 3,
            points: [.init(x: 3, y: 5), .init(x: 35, y: 25)]
        )
        let preview = try requireGeometryPreview(host.performM8(
            target: created.target,
            expectedDocumentRevision: layerUpdate.tree.session.documentRevision,
            command: .beginGeometryPreview(request)
        ))
        XCTAssertEqual(preview.baseRevision, layerUpdate.tree.session.documentRevision)
        let cancelled = try requireM8State(host.performM8(
            target: created.target,
            expectedDocumentRevision: preview.session.documentRevision,
            command: .cancelGeometryPreview
        ))
        XCTAssertEqual(cancelled.session.documentRevision, preview.session.documentRevision)

        _ = try requireGeometryPreview(host.performM8(
            target: created.target,
            expectedDocumentRevision: cancelled.session.documentRevision,
            command: .beginGeometryPreview(request)
        ))
        let committed = try requireM8Mutation(host.performM8(
            target: created.target,
            expectedDocumentRevision: cancelled.session.documentRevision,
            command: .commitGeometryPreview
        ))
        XCTAssertEqual(committed.createdIDs.count, 1)
        XCTAssertGreaterThan(committed.state.session.documentRevision, cancelled.session.documentRevision)

        let selection = try requireVectorSelection(host.performM8(
            target: created.target,
            expectedDocumentRevision: committed.state.session.documentRevision,
            command: .vector(.select(
                mode: .touching,
                bounds: .init(x: 0, y: 0, width: 40, height: 40)
            ))
        ))
        XCTAssertEqual(selection.ranges.map(\.pathID), committed.createdIDs)
        XCTAssertEqual(selection.session.documentRevision, committed.state.session.documentRevision)
        try shutdown(host)
    }

    func testAnnotationFrameAndVanishingPointPersistAndInstructionExportDiffers() throws {
        let host = CoreHost()
        let created = try requireCreated(host.createSession(documentUUID: m8UUID(3)))
        let annotationTree = try requireTreeUpdate(host.editTree(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .createLayer(kind: .annotation, pixelFormat: .none, name: "Instructions")
        ))
        let annotationLayer = try XCTUnwrap(
            annotationTree.tree.layers.first { $0.id == annotationTree.affectedObjectID }
        )
        let annotated = try requireM8Mutation(host.performM8(
            target: created.target,
            expectedDocumentRevision: annotationTree.tree.session.documentRevision,
            command: .annotation([.create(.init(
                kind: .text,
                layerID: annotationLayer.id,
                output: .instruction,
                bounds: .init(x: 8, y: 10, width: 70, height: 24),
                fontFamily: "Helvetica",
                fontSizeMilli: 14_000,
                color: .rgba8(red: 220, green: 30, blue: 30),
                text: "Paint hold"
            ))])
        ))
        XCTAssertEqual(annotated.createdIDs.count, 1)

        let frame = CoreShootingFrame(
            centerX: 320,
            centerY: 240,
            width: 560,
            height: 400,
            rotationDegrees: 7.5
        )
        let framed = try requireM8Mutation(host.performM8(
            target: created.target,
            expectedDocumentRevision: annotated.state.session.documentRevision,
            command: .shootingFrameCreate(frame, preview: false)
        ))
        XCTAssertEqual(framed.createdIDs.count, 1)

        let vpTree = try requireTreeUpdate(host.editTree(
            target: created.target,
            expectedDocumentRevision: framed.state.session.documentRevision,
            command: .createLayer(kind: .vanishingPoint, pixelFormat: .none, name: "Perspective")
        ))
        let vpLayer = try XCTUnwrap(
            vpTree.tree.layers.first { $0.id == vpTree.affectedObjectID }
        )
        let guided = try requireM8Mutation(host.performM8(
            target: created.target,
            expectedDocumentRevision: vpTree.tree.session.documentRevision,
            command: .vanishingPointCreate(.init(
                layerID: vpLayer.id,
                xMilli: 320_000,
                yMilli: 180_000,
                intervalMilliDegrees: 15_000
            ), preview: false)
        ))
        XCTAssertEqual(guided.createdIDs.count, 1)
        guard case .viewUpdated = try requireOutcome(host.applyView(
            target: guided.state.session.primaryView,
            command: .viewportResized(width: 640, height: 480)
        )) else {
            throw M8TestFailure.unexpected(.failed(.coreOperation(.panic)))
        }

        let route = CoreSnapshotRoute(
            session: created.target,
            view: guided.state.session.primaryView,
            surface: CoreSurfaceTarget(
                id: CoreSurfaceID(rawValue: 803),
                generation: CoreSurfaceGeneration(rawValue: 1)
            )
        )
        let snapshot = try requireM8Snapshot(host.buildSnapshot(route: route))
        try snapshot.owner.withBorrowedRenderView { view in
            XCTAssertEqual(view.annotations.count, 1)
            XCTAssertEqual(view.annotations.first?.text, "Paint hold")
            XCTAssertEqual(view.shootingFrames.count, 1)
            XCTAssertEqual(view.vanishingPoints.count, 1)
            XCTAssertFalse(view.radialGuides.isEmpty)
            XCTAssertTrue(view.renderPasses.contains { $0.kind == 6 })
        }
        snapshot.owner.release()

        let normal = try requireRaster(host.exportCommonRaster(
            target: created.target,
            expectedDocumentRevision: guided.state.session.documentRevision,
            format: .png,
            compositeWhite: false
        ))
        let instruction = try requireRaster(host.exportInstructionRaster(
            target: created.target,
            expectedDocumentRevision: guided.state.session.documentRevision,
            format: .png,
            compositeWhite: false
        ))
        XCTAssertNotEqual(normal.bytes, instruction.bytes)

        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("inkpod-m8-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = Array(directory.appendingPathComponent("m8.inkpod").path.utf8)
        let saved = try requireFile(host.save(
            target: created.target,
            expectedDocumentRevision: guided.state.session.documentRevision,
            pathUTF8: path,
            allowCleanSave: true
        ))
        let reopened = try requireFile(host.open(
            target: created.target,
            expectedDocumentRevision: saved.session.documentRevision,
            pathUTF8: path
        ))
        let state = try requireM8State(host.inspectM8(
            target: created.target,
            expectedDocumentRevision: reopened.session.documentRevision
        ))
        XCTAssertEqual(state.shootingFrame?.id, framed.createdIDs.first)
        XCTAssertEqual(state.vanishingPoints.map(\.id), guided.createdIDs)
        try shutdown(host)
    }
}

private enum M8TestFailure: Error {
    case unexpected(CoreRequestOutcome)
    case step(String, CoreRequestOutcome)
}

private func requireOutcome(_ task: CoreTask, timeout: TimeInterval = 20) throws -> CoreRequestOutcome {
    guard let outcome = task.wait(timeout: timeout) else { throw M8TestFailure.unexpected(.failed(.hostStopped)) }
    return outcome
}

private func requireCreated(_ task: CoreTask) throws -> CoreSessionProjection {
    guard case let .created(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func requireTree(_ task: CoreTask) throws -> CoreTreeProjection {
    guard case let .tree(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func requireTreeUpdate(_ task: CoreTask) throws -> CoreTreeMutationProjection {
    guard case let .treeUpdated(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func requireM8State(_ task: CoreTask) throws -> CoreM8Projection {
    guard case let .m8State(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func requireM8Mutation(_ task: CoreTask) throws -> CoreM8MutationProjection {
    guard case let .m8Mutation(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func requireFilterPreview(_ task: CoreTask) throws -> CoreFilterPreviewProjection {
    guard case let .filterPreview(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func requireGeometryPreview(_ task: CoreTask) throws -> CoreGeometryPreviewProjection {
    guard case let .geometryPreview(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func requireVectorSelection(_ task: CoreTask) throws -> CoreVectorSelectionProjection {
    guard case let .vectorSelection(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func requireRaster(_ task: CoreTask) throws -> CoreRasterExport {
    guard case let .rasterExported(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func requireM8Snapshot(_ task: CoreTask) throws -> CoreSnapshotEnvelope {
    guard case let .snapshot(value) = try requireOutcome(task) else {
        throw M8TestFailure.unexpected(try requireOutcome(task))
    }
    return value
}

private func requireFile(_ task: CoreTask) throws -> CoreFileProjection {
    guard case let .fileCompleted(value) = try requireOutcome(task) else { throw M8TestFailure.unexpected(try requireOutcome(task)) }
    return value
}

private func shutdown(_ host: CoreHost) throws {
    guard case .shutdown = try requireOutcome(host.shutdown()) else {
        throw M8TestFailure.unexpected(try requireOutcome(host.shutdown()))
    }
}

private func m8UUID(_ value: UInt64) -> CoreDocumentUUID {
    CoreDocumentUUID(high: 0x4D38, low: value)
}
