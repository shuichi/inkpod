import Foundation
import XCTest
@testable import InkpodCoreBridge

final class CoreFileClipboardIntegrationTests: XCTestCase {
    func testCoordinatedNativeSaveAndStagedOpenUseTheV15PathAPI() throws {
        let host = CoreHost()
        let original = try created(
            host.createSession(documentUUID: documentUUID(0x400)).wait(timeout: 10)
        )
        let dirty = try drawOnePixel(host: host, session: original)
        let directory = FileManager.default.temporaryDirectory
            .appending(path: "inkpod-m4-coordinated-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let destination = directory.appending(path: "coordinated.inkpod")
        let broker = FileAccessBroker()

        let savedOutcome = try broker.coordinateReplacing(destination) { coordinatedURL in
            try outcome(host.save(
                target: dirty.target,
                expectedDocumentRevision: dirty.documentRevision,
                pathUTF8: Array(coordinatedURL.path.utf8),
                allowCleanSave: true
            ).wait(timeout: 20))
        }
        let saved = try fileProjection(savedOutcome, operation: .save)
        XCTAssertFalse(saved.session.isDirty)
        XCTAssertTrue(FileManager.default.fileExists(atPath: destination.path))

        let closeOutcome = try outcome(
            host.closeSession(saved.session.target).wait(timeout: 10)
        )
        guard case .closed = closeOutcome else {
            throw M4IntegrationFailure.unexpected(closeOutcome)
        }
        let replacement = try created(
            host.createSession(documentUUID: documentUUID(0x499)).wait(timeout: 10)
        )
        let openedOutcome = try broker.coordinateReading(destination) { coordinatedURL in
            try outcome(host.open(
                target: replacement.target,
                expectedDocumentRevision: replacement.documentRevision,
                pathUTF8: Array(coordinatedURL.path.utf8)
            ).wait(timeout: 20))
        }
        let opened = try fileProjection(openedOutcome, operation: .open)
        XCTAssertEqual(opened.session.documentUUID, original.documentUUID)
        XCTAssertFalse(opened.session.isDirty)
        try shutdown(host)
    }

    func testNativeSaveAutosaveRecoveryRevertAndFailureAreAtomic() throws {
        let host = CoreHost()
        let session = try created(
            host.createSession(documentUUID: documentUUID(0x401)).wait(timeout: 10)
        )
        let edited = try drawOnePixel(host: host, session: session)
        XCTAssertTrue(edited.isDirty)

        let directory = FileManager.default.temporaryDirectory
            .appending(path: "inkpod-m4-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let normal = directory.appending(path: "normal.inkpod")
        let recovery = directory.appending(path: "recovery.inkpod")

        let saved = try fileProjection(
            host.save(
                target: edited.target,
                expectedDocumentRevision: edited.documentRevision,
                pathUTF8: Array(normal.path.utf8),
                allowCleanSave: true
            ).wait(timeout: 20),
            operation: .save
        )
        XCTAssertFalse(saved.session.isDirty)
        XCTAssertTrue(FileManager.default.fileExists(atPath: normal.path))

        XCTAssertEqual(
            try outcome(host.save(
                target: saved.session.target,
                expectedDocumentRevision: saved.session.documentRevision,
                pathUTF8: Array(normal.path.utf8),
                allowCleanSave: false
            ).wait(timeout: 10)),
            .noOp(saved.session)
        )

        let dirty = try drawOnePixel(host: host, session: saved.session, x: 3, y: 3)
        let autosaved = try fileProjection(
            host.autosave(
                target: dirty.target,
                expectedDocumentRevision: dirty.documentRevision,
                pathUTF8: Array(recovery.path.utf8)
            ).wait(timeout: 20),
            operation: .autosave
        )
        XCTAssertTrue(autosaved.session.isDirty)
        XCTAssertFalse(autosaved.session.isRecovered)

        let reverted = try fileProjection(
            host.revert(
                target: dirty.target,
                expectedDocumentRevision: dirty.documentRevision
            ).wait(timeout: 20),
            operation: .revert
        )
        XCTAssertFalse(reverted.session.isDirty)

        let recovered = try fileProjection(
            host.openRecovery(
                target: reverted.session.target,
                expectedDocumentRevision: reverted.session.documentRevision,
                pathUTF8: Array(recovery.path.utf8)
            ).wait(timeout: 20),
            operation: .openRecovery
        )
        XCTAssertTrue(recovered.session.isDirty)
        XCTAssertTrue(recovered.session.isRecovered)

        let beforeFailure = try inspected(host, recovered.session.target)
        let missingParent = directory.appending(path: "missing/failure.inkpod")
        XCTAssertEqual(
            try outcome(host.save(
                target: beforeFailure.target,
                expectedDocumentRevision: beforeFailure.documentRevision,
                pathUTF8: Array(missingParent.path.utf8),
                allowCleanSave: true
            ).wait(timeout: 20)),
            .failed(.coreOperation(.ioError))
        )
        XCTAssertEqual(try inspected(host, beforeFailure.target), beforeFailure)

        XCTAssertEqual(
            try outcome(host.revert(
                target: beforeFailure.target,
                expectedDocumentRevision: beforeFailure.documentRevision &+ 1
            ).wait(timeout: 10)),
            .failed(.staleTarget)
        )
        try shutdown(host)
    }

    func testCommonRasterExportImportAndInvalidInputPreserveTheLiveDocument() throws {
        let host = CoreHost()
        let session = try created(
            host.createSession(documentUUID: documentUUID(0x402)).wait(timeout: 10)
        )
        let edited = try drawOnePixel(host: host, session: session, x: 8, y: 9)

        let exported = try exportedRaster(
            host.exportCommonRaster(
                target: edited.target,
                expectedDocumentRevision: edited.documentRevision,
                format: .png,
                compositeWhite: false
            ).wait(timeout: 20)
        )
        XCTAssertEqual(exported.format, .png)
        XCTAssertFalse(exported.bytes.isEmpty)
        XCTAssertEqual(try inspected(host, edited.target), edited)

        let importedUUID = documentUUID(0x403)
        let imported = try fileProjection(
            host.importCommonRaster(
                target: edited.target,
                expectedDocumentRevision: edited.documentRevision,
                format: .png,
                bytes: exported.bytes,
                documentUUID: importedUUID
            ).wait(timeout: 20),
            operation: .importRaster
        )
        XCTAssertEqual(imported.session.documentUUID, importedUUID)
        XCTAssertFalse(imported.session.isDirty)

        let beforeInvalid = imported.session
        XCTAssertEqual(
            try outcome(host.importCommonRaster(
                target: beforeInvalid.target,
                expectedDocumentRevision: beforeInvalid.documentRevision,
                format: .png,
                bytes: [],
                documentUUID: documentUUID(0x404)
            ).wait(timeout: 10)),
            .failed(.invalidRequest)
        )
        XCTAssertEqual(try inspected(host, beforeInvalid.target), beforeInvalid)
        try shutdown(host)
    }

    func testTypedClipboardApplyCancelStaleAndExactlyOnceRelease() throws {
        let host = CoreHost()
        let session = try created(
            host.createSession(documentUUID: documentUUID(0x405)).wait(timeout: 10)
        )
        let edited = try drawOnePixel(host: host, session: session, x: 5, y: 6)
        let selected = try documentUpdated(host.selectAllForTesting(
            edited.target,
            expectedDocumentRevision: edited.documentRevision
        ).wait(timeout: 10))
        let copied = try clipboard(
            host.copyClipboard(
                target: selected.target,
                expectedDocumentRevision: selected.documentRevision
            ).wait(timeout: 10)
        )
        XCTAssertGreaterThan(copied.raster.width, 0)
        XCTAssertEqual(
            copied.raster.rgba8.count,
            Int(copied.raster.rowStrideBytes) * Int(copied.raster.height)
        )

        let started = try pasteStarted(host.beginPaste(
            target: selected.target,
            expectedDocumentRevision: selected.documentRevision,
            clipboard: copied.id,
            mode: .compatible
        ).wait(timeout: 10))
        XCTAssertTrue(started.hasActiveTransient)
        let cancelled = try pasteCancelled(host.cancelPaste(
            target: selected.target,
            expectedDocumentRevision: selected.documentRevision
        ).wait(timeout: 10))
        XCTAssertEqual(cancelled.documentRevision, selected.documentRevision)
        XCTAssertFalse(cancelled.hasActiveTransient)

        let external = try clipboard(host.createClipboard(from: CoreClipboardRaster(
            originX: 12,
            originY: 13,
            width: 1,
            height: 1,
            rowStrideBytes: 4,
            rgba8: [0xCC, 0x44, 0x22, 0xFF]
        )).wait(timeout: 10))
        _ = try pasteStarted(host.beginPaste(
            target: selected.target,
            expectedDocumentRevision: selected.documentRevision,
            clipboard: external.id,
            mode: .activePlaneConverted
        ).wait(timeout: 10))
        let committed = try documentUpdated(host.commitPaste(
            target: selected.target,
            expectedDocumentRevision: selected.documentRevision
        ).wait(timeout: 10))
        XCTAssertGreaterThan(committed.documentRevision, selected.documentRevision)

        XCTAssertEqual(
            try outcome(host.beginPaste(
                target: selected.target,
                expectedDocumentRevision: selected.documentRevision,
                clipboard: copied.id,
                mode: .compatible
            ).wait(timeout: 10)),
            .failed(.staleTarget)
        )
        XCTAssertEqual(
            try outcome(host.releaseClipboard(copied.id).wait(timeout: 10)),
            .acknowledged
        )
        XCTAssertEqual(
            try outcome(host.releaseClipboard(copied.id).wait(timeout: 10)),
            .noOp(nil)
        )
        XCTAssertEqual(
            try outcome(host.releaseClipboard(external.id).wait(timeout: 10)),
            .acknowledged
        )
        try shutdown(host)
    }

    func testM7FloatingTransformUsesLatestPreviewFiveAnchorsAndOneUndoUnit() throws {
        let host = CoreHost()
        let session = try created(
            host.createSession(documentUUID: documentUUID(0x407)).wait(timeout: 10)
        )
        let external = try clipboard(host.createClipboard(from: CoreClipboardRaster(
            originX: 12,
            originY: 13,
            width: 2,
            height: 2,
            rowStrideBytes: 8,
            rgba8: [
                0xCC, 0x44, 0x22, 0xFF, 0xCC, 0x44, 0x22, 0xFF,
                0xCC, 0x44, 0x22, 0xFF, 0xCC, 0x44, 0x22, 0xFF,
            ]
        )).wait(timeout: 10))
        defer {
            _ = try? outcome(host.releaseClipboard(external.id).wait(timeout: 10))
            try? shutdown(host)
        }

        _ = try pasteStarted(host.beginPaste(
            target: session.target,
            expectedDocumentRevision: session.documentRevision,
            clipboard: external.id,
            mode: .activePlaneConverted
        ).wait(timeout: 10))
        for anchor in CoreFloatingAnchor.allCases {
            let preview = try floatingTransformed(host.transformFloatingPaste(
                target: session.target,
                expectedDocumentRevision: session.documentRevision,
                transform: CoreFloatingTransform(
                    anchor: anchor,
                    targetX: 40 + Double(anchor.rawValue),
                    targetY: 50 + Double(anchor.rawValue),
                    scaleX: 1,
                    scaleY: 1,
                    rotationDegrees: 0
                )
            ).wait(timeout: 10))
            XCTAssertEqual(preview.documentRevision, session.documentRevision)
            XCTAssertTrue(preview.hasActiveTransient)
        }
        XCTAssertEqual(
            try outcome(host.transformFloatingPaste(
                target: session.target,
                expectedDocumentRevision: session.documentRevision,
                transform: CoreFloatingTransform(
                    anchor: .topLeft,
                    targetX: 0,
                    targetY: 0,
                    scaleX: 0,
                    scaleY: 1,
                    rotationDegrees: 0
                )
            ).wait(timeout: 10)),
            .failed(.invalidRequest)
        )
        XCTAssertEqual(
            try outcome(host.transformFloatingPaste(
                target: session.target,
                expectedDocumentRevision: session.documentRevision + 1,
                transform: .identity
            ).wait(timeout: 10)),
            .failed(.staleTarget)
        )
        let cancelled = try pasteCancelled(host.cancelPaste(
            target: session.target,
            expectedDocumentRevision: session.documentRevision
        ).wait(timeout: 10))
        XCTAssertEqual(cancelled.documentRevision, session.documentRevision)

        _ = try pasteStarted(host.beginPaste(
            target: session.target,
            expectedDocumentRevision: session.documentRevision,
            clipboard: external.id,
            mode: .activePlaneConverted
        ).wait(timeout: 10))
        _ = try floatingTransformed(host.transformFloatingPaste(
            target: session.target,
            expectedDocumentRevision: session.documentRevision,
            transform: CoreFloatingTransform(
                anchor: .center,
                targetX: 10_000,
                targetY: 10_000,
                scaleX: 2,
                scaleY: 3,
                rotationDegrees: 45
            )
        ).wait(timeout: 10))
        _ = try floatingTransformed(host.transformFloatingPaste(
            target: session.target,
            expectedDocumentRevision: session.documentRevision,
            transform: CoreFloatingTransform(
                anchor: .center,
                targetX: 100,
                targetY: 100,
                scaleX: 2,
                scaleY: 3,
                rotationDegrees: 45
            )
        ).wait(timeout: 10))
        let committed = try documentUpdated(host.commitPaste(
            target: session.target,
            expectedDocumentRevision: session.documentRevision
        ).wait(timeout: 10))
        XCTAssertEqual(committed.documentRevision, session.documentRevision + 1)
        let undone = try documentUpdated(host.undo(
            target: session.target,
            expectedDocumentRevision: committed.documentRevision
        ).wait(timeout: 10))
        XCTAssertTrue(undone.canRedo)
        let redone = try documentUpdated(host.redo(
            target: session.target,
            expectedDocumentRevision: undone.documentRevision
        ).wait(timeout: 10))
        XCTAssertFalse(redone.canRedo)
    }

    func testQueuedFileRequestCanBeCancelledWithoutMutation() throws {
        let host = CoreHost()
        let session = try created(
            host.createSession(documentUUID: documentUUID(0x406)).wait(timeout: 10)
        )
        guard case .acknowledged = try outcome(
            host.setNormalProcessingEnabledForTesting(false).wait(timeout: 5)
        ) else {
            return XCTFail("normal lane did not pause")
        }
        let path = FileManager.default.temporaryDirectory
            .appending(path: "cancelled-\(UUID().uuidString).inkpod")
        let pending = host.save(
            target: session.target,
            expectedDocumentRevision: session.documentRevision,
            pathUTF8: Array(path.path.utf8),
            allowCleanSave: true
        )
        _ = try outcome(host.cancel(request: pending.requestID).wait(timeout: 5))
        XCTAssertEqual(try outcome(pending.wait(timeout: 5)), .failed(.cancelled))
        XCTAssertFalse(FileManager.default.fileExists(atPath: path.path))
        _ = try outcome(host.setNormalProcessingEnabledForTesting(true).wait(timeout: 5))
        XCTAssertEqual(try inspected(host, session.target), session)
        try shutdown(host)
    }
}

private enum M4IntegrationFailure: Error {
    case timedOut
    case unexpected(CoreRequestOutcome)
}

private func outcome(_ value: CoreRequestOutcome?) throws -> CoreRequestOutcome {
    guard let value else { throw M4IntegrationFailure.timedOut }
    return value
}

private func created(_ value: CoreRequestOutcome?) throws -> CoreSessionProjection {
    guard case let .created(projection) = try outcome(value) else {
        throw M4IntegrationFailure.unexpected(try outcome(value))
    }
    return projection
}

private func inspected(_ host: CoreHost, _ target: CoreSessionTarget) throws
    -> CoreSessionProjection
{
    let value = try outcome(
        host.inspectSession(target).wait(timeout: 10)
    )
    guard case let .inspected(projection) = value else {
        throw M4IntegrationFailure.unexpected(value)
    }
    return projection
}

private func fileProjection(
    _ value: CoreRequestOutcome?,
    operation: CoreFileOperation
) throws -> CoreFileProjection {
    guard case let .fileCompleted(projection) = try outcome(value),
          projection.operation == operation
    else {
        throw M4IntegrationFailure.unexpected(try outcome(value))
    }
    return projection
}

private func exportedRaster(_ value: CoreRequestOutcome?) throws -> CoreRasterExport {
    guard case let .rasterExported(exported) = try outcome(value) else {
        throw M4IntegrationFailure.unexpected(try outcome(value))
    }
    return exported
}

private func clipboard(_ value: CoreRequestOutcome?) throws -> CoreClipboardProjection {
    guard case let .clipboardCopied(projection) = try outcome(value) else {
        throw M4IntegrationFailure.unexpected(try outcome(value))
    }
    return projection
}

private func pasteStarted(_ value: CoreRequestOutcome?) throws -> CoreSessionProjection {
    guard case let .pasteStarted(projection) = try outcome(value) else {
        throw M4IntegrationFailure.unexpected(try outcome(value))
    }
    return projection
}

private func pasteCancelled(_ value: CoreRequestOutcome?) throws -> CoreSessionProjection {
    guard case let .pasteCancelled(projection) = try outcome(value) else {
        throw M4IntegrationFailure.unexpected(try outcome(value))
    }
    return projection
}

private func floatingTransformed(_ value: CoreRequestOutcome?) throws -> CoreSessionProjection {
    guard case let .floatingTransformed(projection) = try outcome(value) else {
        throw M4IntegrationFailure.unexpected(try outcome(value))
    }
    return projection
}

private func documentUpdated(_ value: CoreRequestOutcome?) throws -> CoreSessionProjection {
    guard case let .documentUpdated(projection) = try outcome(value) else {
        throw M4IntegrationFailure.unexpected(try outcome(value))
    }
    return projection
}

private func drawOnePixel(
    host: CoreHost,
    session: CoreSessionProjection,
    x: Float = 1,
    y: Float = 1
) throws -> CoreSessionProjection {
    let sample = CorePointerSample(deviceX: x, deviceY: y, pressure: 1)
    let begun = try outcome(
        host.beginPencilStroke(target: session.primaryView, samples: [sample])
            .wait(timeout: 10)
    )
    guard case .acknowledged = begun else {
        throw M4IntegrationFailure.unexpected(begun)
    }
    return try documentUpdated(
        host.endStroke(target: session.primaryView).wait(timeout: 10)
    )
}

private func shutdown(_ host: CoreHost) throws {
    let stopped = try outcome(host.shutdown().wait(timeout: 20))
    guard case .shutdown = stopped else { throw M4IntegrationFailure.unexpected(stopped) }
    XCTAssertTrue(host.waitUntilStopped(timeout: 5))
}

private func documentUUID(_ value: UInt64) -> CoreDocumentUUID {
    CoreDocumentUUID(high: 0x4D34, low: value)
}
