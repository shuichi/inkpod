import Foundation
import XCTest
@testable import InkpodCoreBridge

final class CoreM9IntegrationTests: XCTestCase {
    func testCutDefaultsCopyIntoCellPlanWithoutSharingCellHistory() throws {
        let host = CoreHost()
        let defaults = CoreCutDefaults(
            width: 320,
            height: 180,
            dpiXMilli: 96_000,
            dpiYMilli: 96_000,
            marginMilli: 4,
            safeFrameRatioMilli: 850,
            maximumCloseRatioMilli: 950,
            anchor: CoreFrameAnchor.center.rawValue,
            initialLayerKind: .grayscaleColoring,
            pixelFormat: .rgba16
        )
        XCTAssertNil(defaults.cellCreationOptions(count: 0))
        let options = try XCTUnwrap(defaults.cellCreationOptions(count: 2))
        let plan = try requireCellPlan(host.prepareCellCreation(options))
        XCTAssertEqual(plan.items.count, 2)
        XCTAssertTrue(plan.items.allSatisfy {
            $0.width == 320 && $0.height == 180
                && $0.initialLayerKind == .grayscaleColoring
                && $0.pixelFormat == .rgba16
        })
        let cells = try requireCells(host.commitCellCreation(
            plan: plan.id,
            documentUUIDs: [m9DocumentUUID(101), m9DocumentUUID(102)]
        ))
        let members = cells.enumerated().map { index, cell in
            CoreCutMember(
                displayNumber: UInt32(index + 1),
                cellID: cell.cellID,
                documentUUID: cell.documentUUID,
                relativePath: "cell-\(index + 1).inkpod"
            )
        }
        let cut = try requireCut(host.createCut(
            cutUUID: CoreCutUUID(high: 0x4D39, low: 100),
            metadata: CoreCutMetadata(cutName: "Defaults", durationFrames: 12),
            defaults: defaults,
            members: members
        ))
        let changedDefaults = CoreCutDefaults(width: 640, height: 360)
        let changed = try requireCutMutation(host.updateCut(
            target: cut.target,
            expectedRevision: cut.revision,
            metadata: cut.metadata,
            defaults: changedDefaults
        ))
        XCTAssertTrue(changed.applied)
        XCTAssertEqual(changed.cut.defaults, changedDefaults)
        for cell in cells {
            let inspected = try requireInspected(host.inspectSession(cell.target))
            XCTAssertEqual(inspected.documentWidth, 320)
            XCTAssertEqual(inspected.documentHeight, 180)
            XCTAssertEqual(inspected.documentRevision, cell.documentRevision)
            XCTAssertEqual(inspected.canUndo, cell.canUndo)
            XCTAssertEqual(inspected.isDirty, cell.isDirty)
        }
        _ = try outcome(host.closeCut(cut.target))
        for cell in cells { _ = try outcome(host.closeSession(cell.target)) }
        try shutdownM9(host)
    }

    func testCutHistoryCancelStaleSaveOpenAndRecoveryAreIndependent() throws {
        let host = CoreHost()
        let directory = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }

        let first = try requireCreated(host.createSession(documentUUID: m9DocumentUUID(1)))
        let second = try requireCreated(host.createSession(documentUUID: m9DocumentUUID(2)))
        let firstURL = directory.appendingPathComponent("cell1.inkpod")
        let secondURL = directory.appendingPathComponent("cell2.inkpod")
        _ = try requireFile(host.save(
            target: first.target,
            expectedDocumentRevision: first.documentRevision,
            pathUTF8: Array(firstURL.path.utf8),
            allowCleanSave: true
        ))
        _ = try requireFile(host.save(
            target: second.target,
            expectedDocumentRevision: second.documentRevision,
            pathUTF8: Array(secondURL.path.utf8),
            allowCleanSave: true
        ))

        let metadata = CoreCutMetadata(workTitle: "Pilot", cutName: "C001", durationFrames: 24)
        let defaults = CoreCutDefaults(width: 640, height: 480)
        let member1 = CoreCutMember(
            displayNumber: 1,
            cellID: first.cellID,
            documentUUID: first.documentUUID,
            relativePath: firstURL.lastPathComponent
        )
        let member2 = CoreCutMember(
            displayNumber: 2,
            cellID: second.cellID,
            documentUUID: second.documentUUID,
            relativePath: secondURL.lastPathComponent
        )
        let created = try requireCut(host.createCut(
            cutUUID: CoreCutUUID(high: 0x4D39, low: 1),
            metadata: metadata,
            defaults: defaults,
            members: [member1]
        ))
        XCTAssertTrue(created.isDirty)
        XCTAssertEqual(created.members, [member1])

        let cancelled = try requireCutMutation(host.cancelCutUpdate(created.target))
        XCTAssertFalse(cancelled.applied)
        XCTAssertEqual(cancelled.cut.revision, created.revision)
        let noOp = try requireCutMutation(host.updateCut(
            target: created.target,
            expectedRevision: created.revision,
            metadata: metadata,
            defaults: defaults
        ))
        XCTAssertFalse(noOp.applied)
        XCTAssertEqual(noOp.cut.revision, created.revision)
        XCTAssertEqual(
            try outcome(host.updateCut(
                target: created.target,
                expectedRevision: created.revision,
                metadata: CoreCutMetadata(cutName: "", durationFrames: 0),
                defaults: defaults
            )),
            .failed(.invalidRequest)
        )

        let inserted = try requireCutMutation(host.editCutSequence(
            target: created.target,
            expectedRevision: created.revision,
            operations: [.insert(member2, position: 1)]
        ))
        XCTAssertTrue(inserted.applied)
        XCTAssertEqual(inserted.cut.members.map(\.displayNumber), [1, 2])
        XCTAssertEqual(
            try outcome(host.editCutSequence(
                target: created.target,
                expectedRevision: created.revision,
                operations: [.remove(cellID: first.cellID, documentUUID: first.documentUUID)]
            )),
            .failed(.staleTarget)
        )

        let renumbered = try requireCutMutation(host.editCutSequence(
            target: created.target,
            expectedRevision: inserted.cut.revision,
            operations: [
                .moveBefore(
                    cellID: second.cellID,
                    documentUUID: second.documentUUID,
                    anchorCellID: first.cellID,
                    anchorDocumentUUID: first.documentUUID
                ),
                .renumber(position: 0, count: 2, first: 10, step: 10),
            ]
        ))
        XCTAssertEqual(renumbered.cut.members.map(\.displayNumber), [10, 20])
        XCTAssertEqual(renumbered.cut.members.map(\.cellID), [second.cellID, first.cellID])
        let undone = try requireCutMutation(host.undoCut(
            target: created.target,
            expectedRevision: renumbered.cut.revision
        ))
        XCTAssertEqual(undone.cut.members.map(\.cellID), [first.cellID, second.cellID])
        let redone = try requireCutMutation(host.redoCut(
            target: created.target,
            expectedRevision: undone.cut.revision
        ))
        XCTAssertEqual(redone.cut.members.map(\.displayNumber), [10, 20])

        let cutURL = directory.appendingPathComponent("pilot.inkcut")
        let saved = try requireCut(host.saveCut(
            target: created.target,
            expectedRevision: redone.cut.revision,
            pathUTF8: Array(cutURL.path.utf8)
        ))
        XCTAssertFalse(saved.isDirty)
        let changed = try requireCutMutation(host.updateCut(
            target: created.target,
            expectedRevision: saved.revision,
            metadata: CoreCutMetadata(workTitle: "Pilot", cutName: "C001A", durationFrames: 30),
            defaults: defaults
        ))
        let recoveryURL = directory.appendingPathComponent("pilot.recovery.inkcut")
        _ = try requireCut(host.autosaveCut(
            target: created.target,
            expectedRevision: changed.cut.revision,
            pathUTF8: Array(recoveryURL.path.utf8)
        ))
        _ = try outcome(host.closeCut(created.target))
        let reopened = try requireCut(host.openCut(pathUTF8: Array(cutURL.path.utf8)))
        XCTAssertEqual(reopened.metadata.cutName, "C001")
        XCTAssertFalse(reopened.isDirty)
        let recovered = try requireCut(host.openCutRecovery(pathUTF8: Array(recoveryURL.path.utf8)))
        XCTAssertEqual(recovered.metadata.cutName, "C001A")
        XCTAssertTrue(recovered.isDirty)
        XCTAssertNotEqual(reopened.target, recovered.target)
        try shutdownM9(host)
    }

    func testSequenceEndpointPlansNaturalOrderNoOpsAndStaleCommit() throws {
        let host = CoreHost()
        let created = try requireCreated(host.createSession(documentUUID: m9DocumentUUID(10)))
        let empty = try requirePlan(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .resolveStep(.next, .stop)
        ))
        XCTAssertEqual(empty.result, .empty)
        let emptyCommit = try requireAnimation(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .commitStep(empty)
        ))
        XCTAssertEqual(emptyCommit.session.documentRevision, created.documentRevision)

        let one = source(name: "cell10.png", uuid: created.documentUUID, generation: 1, red: 10)
        let installedOne = try requireAnimation(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .replaceSequence([one])
        ))
        XCTAssertEqual(installedOne.sequence.map(\.cellNumber), [10])
        let single = try requirePlan(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .resolveStep(.next, .wrap)
        ))
        XCTAssertEqual(single.result, .singleCell)

        let two = source(name: "cell2.png", uuid: m9DocumentUUID(11), generation: 2, red: 20)
        let installed = try requireAnimation(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .replaceSequence([one, two])
        ))
        XCTAssertEqual(installed.sequence.map(\.cellNumber), [2, 10])
        XCTAssertEqual(installed.activeSequenceIndex, 1)
        let stopped = try requirePlan(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .resolveStep(.next, .stop)
        ))
        XCTAssertEqual(stopped.result, .stopped)
        let wrapped = try requirePlan(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .resolveStep(.next, .wrap)
        ))
        XCTAssertEqual(wrapped.result, .wrapped)

        _ = try requireAnimation(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .replaceSequence([one])
        ))
        XCTAssertEqual(
            try outcome(host.performAnimation(
                target: created.target,
                expectedDocumentRevision: created.documentRevision,
                command: .commitStep(wrapped)
            )),
            .failed(.staleTarget)
        )
        try shutdownM9(host)
    }

    func testEncodedCutSequencePreservesCellUUIDAndGenerationAtomically() throws {
        let host = CoreHost()
        let plan = try requireCellPlan(host.prepareCellCreation(CoreCellCreationOptions(
            sizingMode: .imagePixels,
            width: 2,
            height: 2,
            dpiXMilli: 72_000,
            dpiYMilli: 72_000,
            marginMilli: 0,
            safeFrameRatioMilli: 900,
            maximumCloseRatioMilli: 1_000,
            anchor: .center,
            initialLayerKind: .raster,
            pixelFormat: .rgba8,
            count: 2
        )))
        let cells = try requireCells(host.commitCellCreation(
            plan: plan.id,
            documentUUIDs: [m9DocumentUUID(12), m9DocumentUUID(13)]
        ))
        let first = cells[0]
        let second = cells[1]
        let firstPNG = try requireRaster(host.exportCommonRaster(
            target: first.target,
            expectedDocumentRevision: first.documentRevision,
            format: .png,
            compositeWhite: false
        ))
        let secondPNG = try requireRaster(host.exportCommonRaster(
            target: second.target,
            expectedDocumentRevision: second.documentRevision,
            format: .png,
            compositeWhite: false
        ))
        let installed = try requireAnimation(host.performAnimation(
            target: first.target,
            expectedDocumentRevision: first.documentRevision,
            command: .importIdentifiedSequence([
                CoreIdentifiedNamedRaster(
                    raster: CoreNamedRaster(name: "cell10.png", format: .png, bytes: firstPNG.bytes),
                    documentUUID: first.documentUUID,
                    sourceGeneration: first.documentRevision
                ),
                CoreIdentifiedNamedRaster(
                    raster: CoreNamedRaster(name: "cell2.png", format: .png, bytes: secondPNG.bytes),
                    documentUUID: second.documentUUID,
                    sourceGeneration: second.documentRevision
                ),
            ])
        ))
        XCTAssertEqual(installed.sequence.map(\.cellNumber), [2, 10])
        XCTAssertEqual(installed.sequence.map(\.documentUUID), [second.documentUUID, first.documentUUID])
        XCTAssertEqual(
            installed.sequence.map(\.sourceGeneration),
            [second.documentRevision, first.documentRevision]
        )

        let failed = try outcome(host.performAnimation(
            target: first.target,
            expectedDocumentRevision: first.documentRevision,
            command: .importIdentifiedSequence([
                CoreIdentifiedNamedRaster(
                    raster: CoreNamedRaster(name: "cell3.png", format: .png, bytes: [0, 1, 2]),
                    documentUUID: m9DocumentUUID(14),
                    sourceGeneration: 1
                ),
            ])
        ))
        guard case .failed = failed else { throw M9TestFailure.unexpected(failed) }
        let after = try requireAnimation(host.inspectAnimation(
            target: first.target,
            expectedDocumentRevision: first.documentRevision
        ))
        XCTAssertEqual(after.sequence, installed.sequence)
        _ = try outcome(host.closeSession(first.target))
        _ = try outcome(host.closeSession(second.target))
        try shutdownM9(host)
    }

    func testLightTableBulkPreviewCancelRegisterDuplicateAndUndo() throws {
        let host = CoreHost()
        let created = try requireCreated(host.createSession(documentUUID: m9DocumentUUID(20)))
        let current = source(name: "cell2.png", uuid: created.documentUUID, generation: 1, red: 2)
        let previous = source(name: "cell1.png", uuid: m9DocumentUUID(21), generation: 2, red: 1)
        let next = source(name: "cell3.png", uuid: m9DocumentUUID(22), generation: 3, red: 3)
        let state = try requireAnimation(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .replaceSequence([next, current, previous])
        ))
        let setID = try XCTUnwrap(state.lightTableSets.first?.id)

        let zero = try requireBulk(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .previewLightTableBulk(
                setID: setID,
                direction: .both,
                neighborCount: 0,
                baseOpacityMilli: 800,
                distanceStepMilli: 100
            )
        ))
        XCTAssertTrue(zero.entries.isEmpty)
        XCTAssertEqual(zero.addCount, 0)

        let preview = try requireBulk(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .previewLightTableBulk(
                setID: setID,
                direction: .both,
                neighborCount: 1,
                baseOpacityMilli: 800,
                distanceStepMilli: 100
            )
        ))
        XCTAssertEqual(preview.entries.map(\.cellNumber), [3, 1])
        let unchanged = try requireAnimation(host.inspectAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision
        ))
        XCTAssertTrue(unchanged.lightTableSets.first?.items.isEmpty == true)

        let registered = try requireAnimationMutation(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .registerLightTableBulk(preview.request)
        ))
        XCTAssertTrue(registered.applied)
        XCTAssertEqual(registered.createdIDs.count, 2)
        XCTAssertEqual(
            registered.state.lightTableSets.first?.items.map(\.sourceDocumentUUID),
            [next.documentUUID, previous.documentUUID]
        )

        let duplicate = try requireBulk(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: registered.state.session.documentRevision,
            command: .previewLightTableBulk(
                setID: setID,
                direction: .both,
                neighborCount: 1,
                baseOpacityMilli: 800,
                distanceStepMilli: 100
            )
        ))
        XCTAssertEqual(duplicate.addCount, 0)
        XCTAssertEqual(duplicate.skipCount, 2)
        let undone = try requireSessionUpdate(host.undo(
            target: created.target,
            expectedDocumentRevision: registered.state.session.documentRevision
        ))
        XCTAssertTrue(undone.canRedo)
        let afterUndo = try requireAnimation(host.inspectAnimation(
            target: created.target,
            expectedDocumentRevision: undone.documentRevision
        ))
        XCTAssertTrue(afterUndo.lightTableSets.first?.items.isEmpty == true)
        XCTAssertEqual(
            try outcome(host.performAnimation(
                target: created.target,
                expectedDocumentRevision: registered.state.session.documentRevision,
                command: .registerLightTableBulk(preview.request)
            )),
            .failed(.staleTarget)
        )
        try shutdownM9(host)
    }

    func testMotionAndSubpaletteStayOnCoreOwnerAndCloseCancelsPlayback() throws {
        let host = CoreHost()
        let created = try requireCreated(host.createSession(documentUUID: m9DocumentUUID(30)))
        let cells = [
            source(name: "cell1.png", uuid: created.documentUUID, generation: 1, red: 20),
            source(name: "cell2.png", uuid: m9DocumentUUID(31), generation: 2, red: 40),
        ]
        _ = try requireAnimation(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .replaceSequence(cells)
        ))
        let sampled = try requireAnimationSample(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .setSubpalette(0)
        ), then: host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .sampleSubpalette(x: 0, y: 0)
        ))
        XCTAssertEqual(sampled, .rgba8(red: 20, green: 0, blue: 0))

        let started = try requireMotion(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .motionStart(fps: 24, loop: true, includeSelection: true, includeLightTable: true)
        ))
        XCTAssertTrue(started.includesSelection)
        XCTAssertTrue(started.includesLightTable)
        let paused = try requireMotion(host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .motionTogglePause
        ))
        XCTAssertTrue(paused.isPaused)
        let closed = try requireClosed(host.closeSession(created.target))
        XCTAssertTrue(closed.cancelledActiveTransient)
        try shutdownM9(host)
    }

    func testInvalidSequenceAndLightTableInputsAreAtomic() throws {
        let host = CoreHost()
        let created = try requireCreated(host.createSession(documentUUID: m9DocumentUUID(40)))
        let invalid = CoreRGBA8Source(
            name: "missing-number.png",
            documentUUID: created.documentUUID,
            sourceGeneration: 1,
            width: 2,
            height: 2,
            rgba8: [0, 0, 0, 255]
        )
        XCTAssertEqual(
            try outcome(host.performAnimation(
                target: created.target,
                expectedDocumentRevision: created.documentRevision,
                command: .replaceSequence([invalid])
            )),
            .failed(.invalidRequest)
        )
        let inspected = try requireAnimation(host.inspectAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision
        ))
        XCTAssertTrue(inspected.sequence.isEmpty)
        XCTAssertEqual(
            try outcome(host.performAnimation(
                target: created.target,
                expectedDocumentRevision: created.documentRevision,
                command: .editLightTable(.setGlobalOpacity(1_001))
            )),
            .failed(.invalidRequest)
        )
        let after = try requireAnimation(host.inspectAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision
        ))
        XCTAssertEqual(after, inspected)
        try shutdownM9(host)
    }
}

private enum M9TestFailure: Error { case unexpected(CoreRequestOutcome) }

private func outcome(_ task: CoreTask, timeout: TimeInterval = 20) throws -> CoreRequestOutcome {
    guard let value = task.wait(timeout: timeout) else {
        throw M9TestFailure.unexpected(.failed(.hostStopped))
    }
    return value
}

private func requireCreated(_ task: CoreTask) throws -> CoreSessionProjection {
    let value = try outcome(task)
    guard case let .created(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireInspected(_ task: CoreTask) throws -> CoreSessionProjection {
    let value = try outcome(task)
    guard case let .inspected(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireCellPlan(_ task: CoreTask) throws -> CoreCellCreationPlanProjection {
    let value = try outcome(task)
    guard case let .cellPlan(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireCells(_ task: CoreTask) throws -> [CoreSessionProjection] {
    let value = try outcome(task)
    guard case let .cellsCreated(projections) = value else { throw M9TestFailure.unexpected(value) }
    return projections
}

private func requireClosed(_ task: CoreTask) throws -> CoreSessionCloseProjection {
    let value = try outcome(task)
    guard case let .closed(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireFile(_ task: CoreTask) throws -> CoreFileProjection {
    let value = try outcome(task)
    guard case let .fileCompleted(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireRaster(_ task: CoreTask) throws -> CoreRasterExport {
    let value = try outcome(task)
    guard case let .rasterExported(projection) = value else {
        throw M9TestFailure.unexpected(value)
    }
    return projection
}

private func requireSessionUpdate(_ task: CoreTask) throws -> CoreSessionProjection {
    let value = try outcome(task)
    switch value {
    case let .documentUpdated(projection):
        return projection
    case let .history(projection):
        return projection.session
    default:
        throw M9TestFailure.unexpected(value)
    }
}

private func requireCut(_ task: CoreTask) throws -> CoreCutProjection {
    let value = try outcome(task)
    guard case let .cut(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireCutMutation(_ task: CoreTask) throws -> CoreCutMutationProjection {
    let value = try outcome(task)
    guard case let .cutMutation(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireAnimation(_ task: CoreTask) throws -> CoreAnimationProjection {
    let value = try outcome(task)
    guard case let .animation(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireAnimationMutation(_ task: CoreTask) throws -> CoreAnimationMutationProjection {
    let value = try outcome(task)
    guard case let .animationMutation(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requirePlan(_ task: CoreTask) throws -> CoreSequenceStepPlan {
    let value = try outcome(task)
    guard case let .sequenceStepPlan(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireBulk(_ task: CoreTask) throws -> CoreLightTableBulkPreview {
    let value = try outcome(task)
    guard case let .lightTableBulkPreview(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireMotion(_ task: CoreTask) throws -> CoreMotionProjection {
    let value = try outcome(task)
    guard case let .motion(projection) = value else { throw M9TestFailure.unexpected(value) }
    return projection
}

private func requireAnimationSample(_ first: CoreTask, then second: CoreTask) throws -> CoreColorValue {
    let firstOutcome = try outcome(first)
    guard case .animation = firstOutcome else { throw M9TestFailure.unexpected(firstOutcome) }
    let secondOutcome = try outcome(second)
    guard case let .animationSample(value) = secondOutcome else {
        throw M9TestFailure.unexpected(secondOutcome)
    }
    return value
}

private func shutdownM9(_ host: CoreHost) throws {
    let value = try outcome(host.shutdown())
    guard case .shutdown = value else { throw M9TestFailure.unexpected(value) }
}

private func temporaryDirectory() throws -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("inkpod-m9-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

private func m9DocumentUUID(_ low: UInt64) -> CoreDocumentUUID {
    CoreDocumentUUID(high: 0x4D39, low: low)
}

private func source(
    name: String,
    uuid: CoreDocumentUUID,
    generation: UInt64,
    red: UInt8
) -> CoreRGBA8Source {
    CoreRGBA8Source(
        name: name,
        documentUUID: uuid,
        sourceGeneration: generation,
        width: 2,
        height: 2,
        rgba8: [
            red, 0, 0, 255, red, 0, 0, 255,
            red, 0, 0, 255, red, 0, 0, 255,
        ]
    )
}
