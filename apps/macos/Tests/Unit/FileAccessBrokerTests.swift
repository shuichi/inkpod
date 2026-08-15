import Foundation
import Testing
@testable import InkpodCoreBridge

@Suite("M4 Sandbox file-authority contracts", .serialized)
struct FileAccessBrokerTests {
    @Test("security scope stop is balanced exactly once only after a successful start")
    func securityScopeBalance() {
        let url = URL(filePath: "/tmp/inkpod-m4-scope")
        var starts = 0
        var stops = 0
        let lease = SecurityScopedResourceLease(
            url: url,
            start: { _ in starts += 1; return true },
            stop: { _ in stops += 1 }
        )
        #expect(starts == 1)
        #expect(stops == 0)
        lease.close()
        lease.close()
        #expect(stops == 1)

        let unscoped = SecurityScopedResourceLease(
            url: url,
            start: { _ in false },
            stop: { _ in stops += 1 }
        )
        unscoped.close()
        #expect(stops == 1)
    }

    @Test("file identity reservations reject duplicates and stale commits")
    func fileIdentityReservation() {
        let registry = FileIdentityRegistry()
        let first = CoreSessionTarget(
            id: CoreSessionID(rawValue: 1),
            generation: CoreSessionGeneration(rawValue: 10)
        )
        let second = CoreSessionTarget(
            id: CoreSessionID(rawValue: 2),
            generation: CoreSessionGeneration(rawValue: 20)
        )
        let identity = FileIdentity(volume: 7, file: 11, canonicalPath: "/tmp/a.inkpod")

        let reservation = registry.reserve(identity, for: first)
        #expect(reservation != nil)
        #expect(registry.reserve(identity, for: second) == nil)
        let replacedIdentity = FileIdentity(
            volume: 7,
            file: 12,
            canonicalPath: "/tmp/a.inkpod"
        )
        #expect(registry.commit(reservation!, as: replacedIdentity))
        #expect(registry.owner(of: identity) == nil)
        #expect(registry.owner(of: replacedIdentity) == first)
        registry.release(session: second)
        #expect(registry.owner(of: replacedIdentity) == first)
        registry.release(session: first)
        #expect(registry.owner(of: replacedIdentity) == nil)
    }

    @Test("stale bookmarks are regenerated before their record is published")
    func staleBookmarkRegeneration() throws {
        let suite = "com.inkpod.tests.bookmarks.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let url = URL(filePath: "/tmp/inkpod-m4-bookmark.inkpod")
        var createCount = 0
        let codec = SecurityScopedBookmarkCodec(
            create: { candidate in
                createCount += 1
                return Data("bookmark-\(createCount)-\(candidate.path)".utf8)
            },
            resolve: { _ in (url, true) }
        )
        let store = FileBookmarkStore(defaults: defaults, key: "m4", codec: codec)
        try store.record(url: url, identity: .path(url.path))
        let resolved = try store.resolveMostRecent()
        #expect(resolved?.url == url)
        #expect(resolved?.regenerated == true)
        #expect(createCount == 2)
    }

    @Test("restore-previous preference is versioned, default-off, and persistent")
    func restorePreviousPreference() throws {
        let suite = "com.inkpod.tests.restore-previous.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let preference = PreviousDocumentRestorePreference(defaults: defaults, key: "m4")
        #expect(!preference.load())
        #expect(preference.save(true))
        #expect(preference.load())
        defaults.set(Data("malformed".utf8), forKey: "m4")
        #expect(!preference.load())
    }

    @Test("recovery metadata is versioned, bounded, and never silently discards malformed artifacts")
    func recoveryRegistryLifecycle() throws {
        let directory = FileManager.default.temporaryDirectory
            .appending(path: "inkpod-m4-recovery-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = RecoveryStore(directory: directory, maximumCount: 2)
        let firstArtifact = store.artifactURL(for: "first")
        try Data("native-one".utf8).write(to: firstArtifact)
        let target = CoreSessionTarget(
            id: CoreSessionID(rawValue: 7),
            generation: CoreSessionGeneration(rawValue: 11)
        )
        let documentUUID = CoreDocumentUUID(high: 13, low: 17)

        let first = try store.publish(
            artifactURL: firstArtifact,
            session: target,
            documentUUID: documentUUID,
            originalPath: "/tmp/original.inkpod",
            writtenAtMilliseconds: 23
        )
        #expect(first.metadataIsValid)
        #expect(first.documentUUID == documentUUID)
        #expect(first.originalPath == "/tmp/original.inkpod")

        let malformedArtifact = store.artifactURL(for: "malformed")
        try Data("native-two".utf8).write(to: malformedArtifact)
        try Data("not-a-plist".utf8).write(to: store.metadataURL(for: malformedArtifact))
        let candidates = store.candidates()
        #expect(candidates.count == 2)
        #expect(candidates.contains(first))
        let malformed = try #require(candidates.first { $0.artifactURL == malformedArtifact })
        #expect(!malformed.metadataIsValid)
        #expect(FileManager.default.fileExists(atPath: malformed.artifactURL.path))

        let overflowArtifact = store.artifactURL(for: "overflow")
        try Data("native-three".utf8).write(to: overflowArtifact)
        #expect(throws: RecoveryStoreError.capacityExceeded) {
            try store.publish(
                artifactURL: overflowArtifact,
                session: target,
                documentUUID: documentUUID,
                originalPath: nil,
                writtenAtMilliseconds: 29
            )
        }
        try store.discard(malformed)
        #expect(!FileManager.default.fileExists(atPath: malformed.artifactURL.path))
        #expect(!FileManager.default.fileExists(atPath: malformed.metadataURL.path))
    }

    @Test("atomic export failure leaves the previous destination bytes intact")
    func atomicWriterFailure() throws {
        let directory = FileManager.default.temporaryDirectory
            .appending(path: "inkpod-m4-writer-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let destination = directory.appending(path: "output.png")
        try Data("original".utf8).write(to: destination)
        let writer = AtomicFileWriter(failBeforeReplace: true)

        #expect(throws: (any Error).self) {
            try writer.write(Data("replacement".utf8), to: destination)
        }
        #expect(try Data(contentsOf: destination) == Data("original".utf8))
        #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path).count == 1)
    }

    @Test("drop and panel type classification accepts only the declared M4 formats")
    func supportedTypes() {
        #expect(FileTypeCatalog.classify(URL(filePath: "/tmp/a.inkpod")) == .native)
        #expect(FileTypeCatalog.classify(URL(filePath: "/tmp/a.png")) == .raster(.png))
        #expect(FileTypeCatalog.classify(URL(filePath: "/tmp/a.tiff")) == .raster(.tiff))
        #expect(FileTypeCatalog.classify(URL(filePath: "/tmp/a.tga")) == .raster(.tga))
        #expect(FileTypeCatalog.classify(URL(filePath: "/tmp/a.bmp")) == .raster(.bmp))
        #expect(FileTypeCatalog.classify(URL(filePath: "/tmp/a.jpeg")) == nil)
    }
}
