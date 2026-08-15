import AppKit
import Foundation
import Testing
@testable import InkpodCoreBridge

@Suite("M4 typed and standard pasteboard contracts", .serialized)
@MainActor
struct ClipboardBrokerTests {
    @Test("one pasteboard item publishes private, PNG, and TIFF representations")
    func privateAndStandardRepresentations() async throws {
        let host = CoreHost()
        let pasteboard = NSPasteboard.withUniqueName()
        let broker = ClipboardBroker(coreHost: host, pasteboard: pasteboard)
        let session = try #require(created(await host.createSession(
            documentUUID: CoreDocumentUUID(high: 0x4D34, low: 0x510)
        ).value()))
        let sample = CorePointerSample(deviceX: 4, deviceY: 4, pressure: 1)
        #expect(await host.beginPencilStroke(
            target: session.primaryView,
            samples: [sample]
        ).value() == .acknowledged)
        let committed = try #require(documentUpdated(await host.endStroke(
            target: session.primaryView
        ).value()))
        let selected = try #require(documentUpdated(await host.selectAllForTesting(
            committed.target,
            expectedDocumentRevision: committed.documentRevision
        ).value()))
        let copied = try #require(clipboard(await host.copyClipboard(
            target: selected.target,
            expectedDocumentRevision: selected.documentRevision
        ).value()))

        #expect(await broker.publish(copied))
        #expect(pasteboard.pasteboardItems?.count == 1)
        #expect(pasteboard.data(forType: ClipboardBroker.privateType) != nil)
        #expect(pasteboard.data(forType: .png) != nil)
        #expect(pasteboard.data(forType: .tiff) != nil)
        #expect(await broker.clipboardForPaste() == copied.id)

        await broker.shutdown()
        _ = await host.closeSession(selected.target).value()
        _ = await host.shutdown().value()
        #expect(host.waitUntilStopped(timeout: 5))
    }

    @Test("unsupported external pasteboard content is rejected without a Core mutation")
    func unsupportedExternalRepresentation() async throws {
        let host = CoreHost()
        let pasteboard = NSPasteboard.withUniqueName()
        let broker = ClipboardBroker(coreHost: host, pasteboard: pasteboard)
        pasteboard.clearContents()
        pasteboard.setString("not an image", forType: .string)
        #expect(!(broker.hasPasteableRepresentation()))
        #expect(await broker.clipboardForPaste() == nil)
        await broker.shutdown()
        _ = await host.shutdown().value()
        #expect(host.waitUntilStopped(timeout: 5))
    }
}

private func created(_ outcome: CoreRequestOutcome) -> CoreSessionProjection? {
    guard case let .created(projection) = outcome else { return nil }
    return projection
}

private func documentUpdated(_ outcome: CoreRequestOutcome) -> CoreSessionProjection? {
    guard case let .documentUpdated(projection) = outcome else { return nil }
    return projection
}

private func clipboard(_ outcome: CoreRequestOutcome) -> CoreClipboardProjection? {
    guard case let .clipboardCopied(projection) = outcome else { return nil }
    return projection
}
