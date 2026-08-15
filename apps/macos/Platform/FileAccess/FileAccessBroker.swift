import AppKit
import Darwin
import Foundation
import UniformTypeIdentifiers

final class SecurityScopedResourceLease {
    let url: URL
    private let stopAccess: (URL) -> Void
    private var isActive: Bool
    private let lock = NSLock()

    var isAccessing: Bool { lock.withLock { isActive } }

    init(
        url: URL,
        start: (URL) -> Bool = { $0.startAccessingSecurityScopedResource() },
        stop: @escaping (URL) -> Void = { $0.stopAccessingSecurityScopedResource() }
    ) {
        self.url = url
        stopAccess = stop
        isActive = start(url)
    }

    func close() {
        let shouldStop = lock.withLock {
            guard isActive else { return false }
            isActive = false
            return true
        }
        if shouldStop { stopAccess(url) }
    }

    deinit {
        close()
    }
}

struct FileIdentity: Codable, Hashable, Sendable {
    let volume: UInt64?
    let file: UInt64?
    let canonicalPath: String

    init(volume: UInt64?, file: UInt64?, canonicalPath: String) {
        self.volume = volume
        self.file = file
        self.canonicalPath = canonicalPath
    }

    static func path(_ path: String) -> Self {
        Self(volume: nil, file: nil, canonicalPath: canonical(path))
    }

    static func resolve(_ url: URL) -> Self {
        let path = canonical(url.path)
        var metadata = stat()
        if lstat(path, &metadata) == 0 {
            return Self(
                volume: UInt64(metadata.st_dev),
                file: UInt64(metadata.st_ino),
                canonicalPath: path
            )
        }
        return .path(path)
    }

    private static func canonical(_ path: String) -> String {
        URL(filePath: path).standardizedFileURL.resolvingSymlinksInPath().path
    }
}

struct FileIdentityReservation: Equatable, Sendable {
    fileprivate let identity: FileIdentity
    fileprivate let session: CoreSessionTarget
    fileprivate let generation: UInt64
}

final class FileIdentityRegistry {
    private let lock = NSLock()
    private var owners: [FileIdentity: CoreSessionTarget] = [:]
    private var reservations: [FileIdentity: FileIdentityReservation] = [:]
    private var nextGeneration: UInt64 = 1

    func reserve(
        _ identity: FileIdentity,
        for session: CoreSessionTarget
    ) -> FileIdentityReservation? {
        lock.withLock {
            if let owner = owners[identity], owner != session { return nil }
            if let reserved = reservations[identity], reserved.session != session { return nil }
            guard nextGeneration != 0, nextGeneration < UInt64.max else { return nil }
            let reservation = FileIdentityReservation(
                identity: identity,
                session: session,
                generation: nextGeneration
            )
            nextGeneration += 1
            reservations[identity] = reservation
            return reservation
        }
    }

    func commit(
        _ reservation: FileIdentityReservation,
        as publishedIdentity: FileIdentity? = nil
    ) -> Bool {
        lock.withLock {
            guard reservations[reservation.identity] == reservation else { return false }
            let finalIdentity = publishedIdentity ?? reservation.identity
            if let owner = owners[finalIdentity], owner != reservation.session { return false }
            reservations.removeValue(forKey: reservation.identity)
            owners = owners.filter { $0.value != reservation.session }
            owners[finalIdentity] = reservation.session
            return true
        }
    }

    func cancel(_ reservation: FileIdentityReservation) {
        lock.withLock {
            if reservations[reservation.identity] == reservation {
                reservations.removeValue(forKey: reservation.identity)
            }
        }
    }

    func owner(of identity: FileIdentity) -> CoreSessionTarget? {
        lock.withLock { owners[identity] }
    }

    func release(session: CoreSessionTarget) {
        lock.withLock {
            owners = owners.filter { $0.value != session }
            reservations = reservations.filter { $0.value.session != session }
        }
    }
}

struct SecurityScopedBookmarkCodec {
    let create: (URL) throws -> Data
    let resolve: (Data) throws -> (url: URL, stale: Bool)

    init(
        create: @escaping (URL) throws -> Data = { url in
            try url.bookmarkData(
                options: .withSecurityScope,
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            )
        },
        resolve: @escaping (Data) throws -> (url: URL, stale: Bool) = { data in
            var stale = false
            let url = try URL(
                resolvingBookmarkData: data,
                options: .withSecurityScope,
                relativeTo: nil,
                bookmarkDataIsStale: &stale
            )
            return (url, stale)
        }
    ) {
        self.create = create
        self.resolve = resolve
    }
}

struct BookmarkResolution: Equatable {
    let url: URL
    let identity: FileIdentity
    let regenerated: Bool
}

final class FileBookmarkStore {
    private struct Record: Codable, Equatable {
        let bookmark: Data
        let identity: FileIdentity
    }

    private let defaults: UserDefaults
    private let key: String
    private let codec: SecurityScopedBookmarkCodec
    private let maximumCount: Int

    init(
        defaults: UserDefaults = .standard,
        key: String = "inkpod.macos.recent-bookmarks.v1",
        codec: SecurityScopedBookmarkCodec = SecurityScopedBookmarkCodec(),
        maximumCount: Int = 8
    ) {
        self.defaults = defaults
        self.key = key
        self.codec = codec
        self.maximumCount = maximumCount
    }

    func record(url: URL, identity: FileIdentity) throws {
        let bookmark = try codec.create(url)
        var records = readRecords().filter { $0.identity != identity }
        records.insert(Record(bookmark: bookmark, identity: identity), at: 0)
        publish(Array(records.prefix(maximumCount)))
    }

    func replace(urls: [URL]) throws {
        let records = try urls.prefix(maximumCount).map { url in
            Record(
                bookmark: try codec.create(url),
                identity: FileIdentity.resolve(url)
            )
        }
        publish(records)
    }

    func resolveMostRecent() throws -> BookmarkResolution? {
        guard var record = readRecords().first else { return nil }
        let resolved = try codec.resolve(record.bookmark)
        if resolved.stale {
            record = Record(
                bookmark: try codec.create(resolved.url),
                identity: FileIdentity.resolve(resolved.url)
            )
            var records = readRecords()
            records[0] = record
            publish(records)
        }
        return BookmarkResolution(
            url: resolved.url,
            identity: record.identity,
            regenerated: resolved.stale
        )
    }

    func resolvedRecentURLs() -> [URL] {
        var valid: [Record] = []
        var urls: [URL] = []
        for var record in readRecords() {
            guard let resolved = try? codec.resolve(record.bookmark) else { continue }
            if resolved.stale,
               let bookmark = try? codec.create(resolved.url)
            {
                record = Record(
                    bookmark: bookmark,
                    identity: FileIdentity.resolve(resolved.url)
                )
            }
            valid.append(record)
            urls.append(resolved.url)
        }
        if valid != readRecords() { publish(valid) }
        return urls
    }

    private func readRecords() -> [Record] {
        guard let data = defaults.data(forKey: key),
              let records = try? PropertyListDecoder().decode([Record].self, from: data)
        else {
            return []
        }
        return Array(records.prefix(maximumCount))
    }

    private func publish(_ records: [Record]) {
        guard let data = try? PropertyListEncoder().encode(records) else { return }
        defaults.set(data, forKey: key)
    }
}

private struct RecoveryMetadataRecord: Codable, Equatable {
    let version: UInt32
    let artifactFilename: String
    let sessionID: UInt64
    let sessionGeneration: UInt64
    let documentUUIDHigh: UInt64
    let documentUUIDLow: UInt64
    let originalPath: String?
    let writtenAtMilliseconds: UInt64
}

struct RecoveryCandidate: Identifiable, Equatable, Hashable, Sendable {
    var id: String { artifactURL.path }

    let artifactURL: URL
    let metadataURL: URL
    let documentUUID: CoreDocumentUUID?
    let originalPath: String?
    let writtenAtMilliseconds: UInt64?
    let metadataIsValid: Bool
}

enum RecoveryStoreError: Error, Equatable {
    case invalidArtifact
    case capacityExceeded
    case invalidMetadata
}

final class RecoveryStore: @unchecked Sendable {
    let directory: URL
    private let maximumCount: Int
    private let fileManager: FileManager

    init(
        directory: URL? = nil,
        maximumCount: Int = 64,
        fileManager: FileManager = .default
    ) {
        self.fileManager = fileManager
        self.maximumCount = max(1, maximumCount)
        if let directory {
            self.directory = directory.standardizedFileURL
        } else {
            let support = fileManager.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first ?? fileManager.temporaryDirectory
            self.directory = support
                .appending(path: "Inkpod/Recovery", directoryHint: .isDirectory)
                .standardizedFileURL
        }
    }

    func artifactURL(for identifier: String) -> URL {
        let safeIdentifier = identifier.unicodeScalars.map { scalar -> Character in
            CharacterSet.alphanumerics.contains(scalar) || scalar == "-" || scalar == "_"
                ? Character(String(scalar)) : "_"
        }
        return directory.appending(path: "\(String(safeIdentifier)).inkpod")
    }

    func metadataURL(for artifactURL: URL) -> URL {
        artifactURL.appendingPathExtension("recovery.plist")
    }

    @discardableResult
    func publish(
        artifactURL: URL,
        session: CoreSessionTarget,
        documentUUID: CoreDocumentUUID,
        originalPath: String?,
        writtenAtMilliseconds: UInt64
    ) throws -> RecoveryCandidate {
        try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        let artifactURL = artifactURL.standardizedFileURL
        guard owns(artifactURL), artifactURL.pathExtension.lowercased() == "inkpod",
              fileManager.fileExists(atPath: artifactURL.path), documentUUID.isValid,
              session.id.rawValue != 0, session.generation.rawValue != 0,
              writtenAtMilliseconds != 0,
              originalPath?.utf8.count ?? 0 <= 32_768
        else {
            throw RecoveryStoreError.invalidArtifact
        }
        guard hasCapacity(replacing: artifactURL) else {
            throw RecoveryStoreError.capacityExceeded
        }
        let record = RecoveryMetadataRecord(
            version: 1,
            artifactFilename: artifactURL.lastPathComponent,
            sessionID: session.id.rawValue,
            sessionGeneration: session.generation.rawValue,
            documentUUIDHigh: documentUUID.high,
            documentUUIDLow: documentUUID.low,
            originalPath: originalPath,
            writtenAtMilliseconds: writtenAtMilliseconds
        )
        let encoder = PropertyListEncoder()
        encoder.outputFormat = .binary
        let data = try encoder.encode(record)
        guard data.count <= 65_536 else { throw RecoveryStoreError.invalidMetadata }
        let sidecarURL = metadataURL(for: artifactURL)
        try AtomicFileWriter().write(data, to: sidecarURL)
        return RecoveryCandidate(
            artifactURL: artifactURL,
            metadataURL: sidecarURL,
            documentUUID: documentUUID,
            originalPath: originalPath,
            writtenAtMilliseconds: writtenAtMilliseconds,
            metadataIsValid: true
        )
    }

    func candidates() -> [RecoveryCandidate] {
        guard let enumerator = fileManager.enumerator(
            at: directory,
            includingPropertiesForKeys: [.contentModificationDateKey, .fileSizeKey],
            options: [.skipsHiddenFiles, .skipsSubdirectoryDescendants]
        ) else {
            return []
        }
        var result: [RecoveryCandidate] = []
        while let value = enumerator.nextObject() as? URL {
            guard value.pathExtension.lowercased() == "inkpod", owns(value) else { continue }
            result.append(candidate(for: value.standardizedFileURL))
            if result.count >= maximumCount { break }
        }
        return result.sorted {
            ($0.writtenAtMilliseconds ?? 0, $0.artifactURL.lastPathComponent)
                > ($1.writtenAtMilliseconds ?? 0, $1.artifactURL.lastPathComponent)
        }
    }

    func discard(_ candidate: RecoveryCandidate) throws {
        guard owns(candidate.artifactURL), owns(candidate.metadataURL),
              candidate.metadataURL == metadataURL(for: candidate.artifactURL)
        else {
            throw RecoveryStoreError.invalidArtifact
        }
        if fileManager.fileExists(atPath: candidate.artifactURL.path) {
            try fileManager.removeItem(at: candidate.artifactURL)
        }
        if fileManager.fileExists(atPath: candidate.metadataURL.path) {
            try fileManager.removeItem(at: candidate.metadataURL)
        }
    }

    func discardArtifact(at artifactURL: URL) throws {
        try discard(candidate(for: artifactURL.standardizedFileURL))
    }

    private func candidate(for artifactURL: URL) -> RecoveryCandidate {
        let sidecarURL = metadataURL(for: artifactURL)
        guard let values = try? sidecarURL.resourceValues(forKeys: [.fileSizeKey]),
              let size = values.fileSize, (1 ... 65_536).contains(size),
              let data = try? Data(contentsOf: sidecarURL),
              let record = try? PropertyListDecoder().decode(
                  RecoveryMetadataRecord.self,
                  from: data
              ),
              record.version == 1,
              record.artifactFilename == artifactURL.lastPathComponent,
              record.sessionID != 0, record.sessionGeneration != 0,
              record.documentUUIDHigh != 0 || record.documentUUIDLow != 0,
              record.writtenAtMilliseconds != 0,
              record.originalPath?.utf8.count ?? 0 <= 32_768
        else {
            let timestamp = (try? artifactURL.resourceValues(
                forKeys: [.contentModificationDateKey]
            ).contentModificationDate)?.timeIntervalSince1970
            return RecoveryCandidate(
                artifactURL: artifactURL,
                metadataURL: sidecarURL,
                documentUUID: nil,
                originalPath: nil,
                writtenAtMilliseconds: timestamp.map { UInt64(max(0, $0 * 1_000)) },
                metadataIsValid: false
            )
        }
        return RecoveryCandidate(
            artifactURL: artifactURL,
            metadataURL: sidecarURL,
            documentUUID: CoreDocumentUUID(
                high: record.documentUUIDHigh,
                low: record.documentUUIDLow
            ),
            originalPath: record.originalPath,
            writtenAtMilliseconds: record.writtenAtMilliseconds,
            metadataIsValid: true
        )
    }

    private func owns(_ url: URL) -> Bool {
        url.standardizedFileURL.deletingLastPathComponent() == directory
    }

    private func hasCapacity(replacing artifactURL: URL) -> Bool {
        guard let enumerator = fileManager.enumerator(
            at: directory,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles, .skipsSubdirectoryDescendants]
        ) else {
            return true
        }
        var otherCount = 0
        while let value = enumerator.nextObject() as? URL {
            guard value.pathExtension.lowercased() == "inkpod",
                  value.standardizedFileURL != artifactURL
            else {
                continue
            }
            otherCount += 1
            if otherCount >= maximumCount { return false }
        }
        return true
    }
}

struct PreviousDocumentRestoreRecord: Codable, Equatable {
    let version: UInt32
    let enabled: Bool
}

final class PreviousDocumentRestorePreference {
    private let defaults: UserDefaults
    private let key: String

    init(
        defaults: UserDefaults = .standard,
        key: String = "inkpod.macos.restore-previous.v1"
    ) {
        self.defaults = defaults
        self.key = key
    }

    func load() -> Bool {
        guard let data = defaults.data(forKey: key),
              let record = try? PropertyListDecoder().decode(
                  PreviousDocumentRestoreRecord.self,
                  from: data
              ),
              record.version == 1
        else {
            return false
        }
        return record.enabled
    }

    func save(_ enabled: Bool) -> Bool {
        guard let data = try? PropertyListEncoder().encode(
            PreviousDocumentRestoreRecord(version: 1, enabled: enabled)
        ) else {
            return false
        }
        defaults.set(data, forKey: key)
        return true
    }
}

enum AtomicFileWriterError: Error {
    case injectedFailure
    case replacementFailed(Int32)
}

struct AtomicFileWriter {
    let failBeforeReplace: Bool

    init(failBeforeReplace: Bool = false) {
        self.failBeforeReplace = failBeforeReplace
    }

    func write(_ data: Data, to destination: URL) throws {
        let directory = destination.deletingLastPathComponent()
        let temporary = directory.appending(
            path: ".\(destination.lastPathComponent).inkpod-\(UUID().uuidString).tmp"
        )
        defer { try? FileManager.default.removeItem(at: temporary) }
        try data.write(to: temporary, options: [])
        let handle = try FileHandle(forWritingTo: temporary)
        try handle.synchronize()
        try handle.close()
        if failBeforeReplace { throw AtomicFileWriterError.injectedFailure }
        guard rename(temporary.path, destination.path) == 0 else {
            throw AtomicFileWriterError.replacementFailed(errno)
        }
        let descriptor = open(directory.path, O_RDONLY)
        if descriptor >= 0 {
            _ = fsync(descriptor)
            _ = Darwin.close(descriptor)
        }
    }
}

enum FileTypeClassification: Equatable, Sendable {
    case native
    case raster(CoreCommonRasterFormat)
}

enum FileTypeCatalog {
    static let native = UTType(
        exportedAs: "com.inkpod.document",
        conformingTo: .data
    )
    static let tga = UTType(filenameExtension: "tga") ?? .data
    static let readableContentTypes: [UTType] = [native, .png, .tiff, tga, .bmp]
    static let rasterContentTypes: [UTType] = [.png, .tiff, tga, .bmp]

    static func classify(_ url: URL) -> FileTypeClassification? {
        switch url.pathExtension.lowercased() {
        case "inkpod": .native
        case "png": .raster(.png)
        case "tif", "tiff": .raster(.tiff)
        case "tga": .raster(.tga)
        case "bmp": .raster(.bmp)
        default: nil
        }
    }

    static func filenameExtension(for format: CoreCommonRasterFormat) -> String {
        switch format {
        case .png: "png"
        case .tiff: "tiff"
        case .tga: "tga"
        case .bmp: "bmp"
        }
    }
}

enum FileAccessBrokerError: Error {
    case coordinationFailed
    case accessorNotInvoked
}

final class FileAccessBroker: @unchecked Sendable {
    func coordinateReading<T>(
        _ url: URL,
        operation: (URL) throws -> T
    ) throws -> T {
        try coordinate(url, writing: false, operation: operation)
    }

    func coordinateReplacing<T>(
        _ url: URL,
        operation: (URL) throws -> T
    ) throws -> T {
        try coordinate(url, writing: true, operation: operation)
    }

    private func coordinate<T>(
        _ url: URL,
        writing: Bool,
        operation: (URL) throws -> T
    ) throws -> T {
        let lease = SecurityScopedResourceLease(url: url)
        defer { lease.close() }
        let coordinator = NSFileCoordinator(filePresenter: nil)
        var coordinationError: NSError?
        var result: Result<T, Error>?
        if writing {
            coordinator.coordinate(
                writingItemAt: url,
                options: .forReplacing,
                error: &coordinationError
            ) { coordinatedURL in
                result = Result { try operation(coordinatedURL) }
            }
        } else {
            coordinator.coordinate(
                readingItemAt: url,
                options: [],
                error: &coordinationError
            ) { coordinatedURL in
                result = Result { try operation(coordinatedURL) }
            }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw FileAccessBrokerError.accessorNotInvoked }
        return try result.get()
    }
}
