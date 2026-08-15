import Foundation
@testable import InkpodCoreBridge
import Testing

@Suite("M10 batch domain", .serialized)
struct BatchDomainTests {
    @Test("operation editing is ordered, bounded, and no-op stable")
    func orderedOperationEditing() {
        var draft = BatchWindowDraft()
        #expect(draft.add(.invertColorPlane()) == .applied)
        #expect(draft.add(.example(.mirror)) == .applied)
        let before = draft.operations
        #expect(draft.moveOperation(from: 0, to: 0) == .noOp)
        #expect(draft.operations == before)
        #expect(draft.moveOperation(from: 0, to: 1) == .applied)
        #expect(draft.operations.map(\.kind) == [.mirror, .filter])
        #expect(draft.removeOperation(at: 9) == .invalid)
    }

    @Test("all concrete batch commands map to an implemented operation or workflow")
    func concreteCommandCoverage() {
        #expect(BatchCommandCatalog.operationCommands.count == 24)
        #expect(Set(BatchCommandCatalog.operationCommands.values.map(\.kind))
            == Set(CoreBatchOperationKind.allCases))
        #expect(BatchCommandCatalog.surfaceCommands.count == 50)
    }

    @Test("security scoped lease closes exactly once on reject replace close and shutdown")
    func securityScopeLeaseExactlyOnce() {
        var starts = 0
        var stops = 0
        let url = URL(filePath: "/tmp/inkpod-batch-scope")
        let broker = BatchFolderBroker(
            startAccess: { _ in starts += 1; return true },
            stopAccess: { _ in stops += 1 }
        )
        let first = broker.acquire(url)
        #expect(first != nil)
        first?.close()
        first?.close()
        #expect((starts, stops) == (1, 1))
        let replacement = broker.acquire(url)
        #expect(replacement != nil)
        replacement?.close()
        #expect((starts, stops) == (2, 2))

        let denied = BatchFolderBroker(
            startAccess: { _ in false },
            stopAccess: { _ in stops += 1 }
        )
        #expect(denied.acquire(url) == nil)
        #expect(stops == 2)
    }

    @Test("expired Batch folder bookmark is regenerated before reuse")
    func batchBookmarkExpiry() throws {
        let suite = "com.inkpod.tests.batch-bookmarks.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let folder = URL(filePath: "/tmp/inkpod-batch-bookmark", directoryHint: .isDirectory)
        var creates = 0
        let store = FileBookmarkStore(
            defaults: defaults,
            key: "m10",
            codec: SecurityScopedBookmarkCodec(
                create: { url in
                    creates += 1
                    return Data("batch-\(creates)-\(url.path)".utf8)
                },
                resolve: { _ in (folder, true) }
            )
        )
        try store.record(url: folder, identity: .path(folder.path))
        let resolved = try store.resolveMostRecent()
        #expect(resolved?.url == folder)
        #expect(resolved?.regenerated == true)
        #expect(creates == 2)
    }

    @Test("window close registry cancels a queued job and releases leases after completion")
    @MainActor
    func windowCloseDuringJob() async throws {
        let host = CoreHost()
        let projection = try #require(await batchCreatedSession(host, low: 101))
        guard case .acknowledged = await host
            .setNormalProcessingEnabledForTesting(false).value()
        else { Issue.record("failed to pause normal processing"); return }
        let task = host.executeBatch(
            target: projection.target,
            expectedDocumentRevision: projection.documentRevision,
            graph: CoreBatchGraphDraft(
                name: "Close",
                inputs: [.currentSequence()],
                operations: [.invertColorPlane()]
            ),
            options: CoreBatchRunOptions(scope: .current, dryRun: true)
        )
        var stopped = 0
        let lease = SecurityScopedResourceLease(
            url: URL(filePath: "/tmp/m10-close"),
            start: { _ in true },
            stop: { _ in stopped += 1 }
        )
        let registry = BatchJobRegistry()
        registry.retain(lease)
        #expect(registry.start(task))
        registry.close(using: host)
        #expect(stopped == 0)
        #expect(await task.value() == .failed(.cancelled))
        registry.complete(task)
        await registry.waitUntilStopped()
        #expect(stopped == 1)
        guard case .acknowledged = await host
            .setNormalProcessingEnabledForTesting(true).value()
        else { Issue.record("failed to resume normal processing"); return }
        guard case .shutdown = await host.shutdown().value() else {
            Issue.record("host shutdown failed"); return
        }
    }

    @Test("application shutdown cancels and joins a queued Batch job")
    @MainActor
    func shutdownRace() async throws {
        let host = CoreHost()
        let projection = try #require(await batchCreatedSession(host, low: 102))
        guard case .acknowledged = await host
            .setNormalProcessingEnabledForTesting(false).value()
        else { Issue.record("failed to pause normal processing"); return }
        let task = host.executeBatch(
            target: projection.target,
            expectedDocumentRevision: projection.documentRevision,
            graph: CoreBatchGraphDraft(
                name: "Shutdown",
                inputs: [.currentSequence()],
                operations: [.invertColorPlane()]
            ),
            options: CoreBatchRunOptions(scope: .current, dryRun: true)
        )
        let registry = BatchJobRegistry()
        #expect(registry.start(task))
        registry.close(using: host)
        let shutdown = host.shutdown()
        #expect(await task.value() == .failed(.cancelled))
        registry.complete(task)
        guard case .shutdown = await shutdown.value() else {
            Issue.record("host shutdown failed"); return
        }
        #expect(host.waitUntilStopped(timeout: 5))
    }
}

@MainActor
private func batchCreatedSession(
    _ host: CoreHost,
    low: UInt64
) async -> CoreSessionProjection? {
    guard case let .created(projection) = await host.createSession(
        documentUUID: CoreDocumentUUID(high: 0x4D3130, low: low)
    ).value() else { return nil }
    return projection
}
