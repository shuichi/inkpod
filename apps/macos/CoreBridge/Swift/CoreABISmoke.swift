import Darwin
import Foundation
import InkpodCoreC

public struct CoreABISmokeResult: Equatable, Sendable {
    public let abiVersion: UInt32
    public let snapshotRevision: UInt64
    public let callerThreadID: UInt64
    public let ownerThreadID: UInt64
    public let snapshotReleaseThreadID: UInt64
    public let coreOwnerWasNullAfterDestroy: Bool
    public let snapshotReleaseCount: Int
}

public struct ABIMismatchProbeResult: Equatable, Sendable {
    public let status: CoreStatus
    public let diagnostic: String
    public let failureThreadID: UInt64
    public let diagnosticThreadID: UInt64
}

public struct InvalidCreateProbeResult: Equatable, Sendable {
    public let shortStructStatus: CoreStatus
    public let nullConfigStatus: CoreStatus
    public let ownerStayedNull: Bool
}

public struct WrongThreadProbeResult: Equatable, Sendable {
    public let status: CoreStatus
    public let snapshotOwnerStayedNull: Bool
    public let ownerThreadCouldDestroy: Bool
}

public struct DoubleSnapshotReleaseProbeResult: Equatable, Sendable {
    public let first: CoreStatus
    public let second: CoreStatus
    public let ffiReleaseCount: Int
}

public enum CoreABISmoke {
    public static func run() throws -> CoreABISmokeResult {
        let callerThreadID = currentThreadID()
        return try runOnDedicatedThread {
            let ownerThreadID = currentThreadID()
            let abiVersion = inkpod_abi_version()
            guard abiVersion == inkpod_bridge_abi_version() else {
                throw failure("inkpod_abi_version", status: .incompatibleABI)
            }

            var core = try createCore()
            defer {
                if core != nil {
                    _ = inkpod_core_destroy(&core)
                }
            }
            var replay = InkpodReplayContract()
            replay.struct_size = UInt32(MemoryLayout<InkpodReplayContract>.size)
            try requireOK(
                inkpod_core_get_replay_contract(core, &replay),
                operation: "inkpod_core_get_replay_contract"
            )
            guard replay.replay_epoch == 23, replay.procedure_format_version == 26 else {
                throw CoreBridgeFailure(
                    operation: "replay contract",
                    status: .incompatibleABI,
                    diagnostic: "unexpected replay contract"
                )
            }

            var options = InkpodSnapshotOptions()
            options.struct_size = UInt32(MemoryLayout<InkpodSnapshotOptions>.size)
            options.feature_flags = inkpod_bridge_feature_none()
            var rawSnapshot: OpaquePointer?
            try requireOK(
                inkpod_core_build_snapshot(core, &options, &rawSnapshot),
                operation: "inkpod_core_build_snapshot"
            )
            guard let rawSnapshot else {
                throw CoreBridgeFailure(
                    operation: "inkpod_core_build_snapshot",
                    status: .panic,
                    diagnostic: "success did not publish a snapshot owner"
                )
            }
            let snapshot = OwnedSnapshot(raw: rawSnapshot)
            var view = InkpodSnapshotView()
            view.struct_size = UInt32(MemoryLayout<InkpodSnapshotView>.size)
            try snapshot.withBorrowed { borrowed in
                try requireOK(
                    inkpod_snapshot_get_view(borrowed, &view),
                    operation: "inkpod_snapshot_get_view"
                )
            }

            let release = releaseOnAnotherThread(snapshot)
            guard release.status == .ok else {
                throw CoreBridgeFailure(
                    operation: "inkpod_snapshot_release",
                    status: release.status,
                    diagnostic: "snapshot release failed"
                )
            }

            try requireOK(inkpod_core_destroy(&core), operation: "inkpod_core_destroy")
            return CoreABISmokeResult(
                abiVersion: abiVersion,
                snapshotRevision: view.revision,
                callerThreadID: callerThreadID,
                ownerThreadID: ownerThreadID,
                snapshotReleaseThreadID: release.threadID,
                coreOwnerWasNullAfterDestroy: core == nil,
                snapshotReleaseCount: snapshot.ffiReleaseCount
            )
        }
    }

    public static func probeABIMismatch() throws -> ABIMismatchProbeResult {
        try runOnDedicatedThread {
            var config = coreConfig()
            config.abi_version = inkpod_bridge_abi_version() &+ 1
            var core: OpaquePointer?
            let failureThreadID = currentThreadID()
            let status = CoreStatus(cValue: inkpod_core_create(&config, &core))
            let diagnostic = copyDiagnostic()
            let diagnosticThreadID = currentThreadID()
            if core != nil {
                _ = inkpod_core_destroy(&core)
            }
            return ABIMismatchProbeResult(
                status: status,
                diagnostic: diagnostic,
                failureThreadID: failureThreadID,
                diagnosticThreadID: diagnosticThreadID
            )
        }
    }

    public static func probeInvalidCreateInputs() throws -> InvalidCreateProbeResult {
        try runOnDedicatedThread {
            var shortConfig = coreConfig()
            shortConfig.struct_size = UInt32(MemoryLayout<UInt32>.size)
            var core: OpaquePointer?
            let shortStatus = CoreStatus(cValue: inkpod_core_create(&shortConfig, &core))
            let nullStatus = CoreStatus(cValue: inkpod_core_create(nil, &core))
            let stayedNull = core == nil
            if core != nil {
                _ = inkpod_core_destroy(&core)
            }
            return InvalidCreateProbeResult(
                shortStructStatus: shortStatus,
                nullConfigStatus: nullStatus,
                ownerStayedNull: stayedNull
            )
        }
    }

    public static func probeWrongThread() throws -> WrongThreadProbeResult {
        try runOnDedicatedThread {
            var core = try createCore()
            defer {
                if core != nil {
                    _ = inkpod_core_destroy(&core)
                }
            }
            let probe = BorrowedCoreProbe(raw: core!)
            let resultBox = BlockingResultBox<WrongThreadCallResult>()
            let thread = Thread {
                var options = InkpodSnapshotOptions()
                options.struct_size = UInt32(MemoryLayout<InkpodSnapshotOptions>.size)
                var snapshot: OpaquePointer?
                let status = CoreStatus(
                    cValue: inkpod_core_build_snapshot(probe.raw, &options, &snapshot)
                )
                if snapshot != nil {
                    _ = inkpod_snapshot_release(&snapshot)
                }
                resultBox.complete(
                    .success(
                        WrongThreadCallResult(
                            status: status,
                            snapshotOwnerStayedNull: snapshot == nil
                        )
                    )
                )
            }
            thread.name = "inkpod.macos.abi-wrong-thread"
            thread.start()
            let callResult = try resultBox.wait()
            let destroyStatus = CoreStatus(cValue: inkpod_core_destroy(&core))
            return WrongThreadProbeResult(
                status: callResult.status,
                snapshotOwnerStayedNull: callResult.snapshotOwnerStayedNull,
                ownerThreadCouldDestroy: destroyStatus == .ok && core == nil
            )
        }
    }

    public static func probeDoubleSnapshotRelease() throws -> DoubleSnapshotReleaseProbeResult {
        try runOnDedicatedThread {
            var core = try createCore()
            defer {
                if core != nil {
                    _ = inkpod_core_destroy(&core)
                }
            }
            var options = InkpodSnapshotOptions()
            options.struct_size = UInt32(MemoryLayout<InkpodSnapshotOptions>.size)
            var rawSnapshot: OpaquePointer?
            try requireOK(
                inkpod_core_build_snapshot(core, &options, &rawSnapshot),
                operation: "inkpod_core_build_snapshot"
            )
            guard let rawSnapshot else {
                throw CoreBridgeFailure(
                    operation: "inkpod_core_build_snapshot",
                    status: .panic,
                    diagnostic: "success did not publish a snapshot owner"
                )
            }
            let snapshot = OwnedSnapshot(raw: rawSnapshot)
            let first = snapshot.release()
            let second = snapshot.release()
            try requireOK(inkpod_core_destroy(&core), operation: "inkpod_core_destroy")
            return DoubleSnapshotReleaseProbeResult(
                first: first,
                second: second,
                ffiReleaseCount: snapshot.ffiReleaseCount
            )
        }
    }
}

private struct WrongThreadCallResult: Sendable {
    let status: CoreStatus
    let snapshotOwnerStayedNull: Bool
}

private final class BorrowedCoreProbe: @unchecked Sendable {
    let raw: OpaquePointer

    init(raw: OpaquePointer) {
        self.raw = raw
    }
}

private final class OwnedSnapshot: @unchecked Sendable {
    private let lock = NSLock()
    private var raw: OpaquePointer?
    private var releaseCalls = 0

    init(raw: OpaquePointer) {
        self.raw = raw
    }

    var ffiReleaseCount: Int {
        lock.withLock { releaseCalls }
    }

    func withBorrowed<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        try lock.withLock {
            guard let raw else {
                throw CoreBridgeFailure(
                    operation: "snapshot borrow",
                    status: .invalidState,
                    diagnostic: "snapshot was already released"
                )
            }
            return try body(raw)
        }
    }

    @discardableResult
    func release() -> CoreStatus {
        lock.withLock {
            guard let raw else {
                return .ok
            }
            var owner: OpaquePointer? = raw
            releaseCalls += 1
            let status = CoreStatus(cValue: inkpod_snapshot_release(&owner))
            self.raw = owner
            return status
        }
    }

    deinit {
        _ = release()
    }
}

private final class BlockingResultBox<Value>: @unchecked Sendable {
    private let condition = NSCondition()
    private var result: Result<Value, any Error>?

    func complete(_ result: Result<Value, any Error>) {
        condition.lock()
        precondition(self.result == nil, "completion must be delivered exactly once")
        self.result = result
        condition.broadcast()
        condition.unlock()
    }

    func wait() throws -> Value {
        condition.lock()
        while result == nil {
            condition.wait()
        }
        let completed = result!
        condition.unlock()
        return try completed.get()
    }
}

private final class ThrowingOperation<Value>: @unchecked Sendable {
    let body: () throws -> Value

    init(_ body: @escaping () throws -> Value) {
        self.body = body
    }
}

private func runOnDedicatedThread<Value>(
    _ body: @escaping () throws -> Value
) throws -> Value {
    let operation = ThrowingOperation(body)
    let resultBox = BlockingResultBox<Value>()
    let thread = Thread {
        let result = Result { try operation.body() }
        resultBox.complete(result)
    }
    thread.name = "inkpod.macos.abi-owner"
    thread.start()
    return try resultBox.wait()
}

private func coreConfig() -> InkpodCoreConfig {
    var config = InkpodCoreConfig()
    config.struct_size = UInt32(MemoryLayout<InkpodCoreConfig>.size)
    config.abi_version = inkpod_bridge_abi_version()
    config.feature_flags = inkpod_bridge_feature_none()
    return config
}

private func createCore() throws -> OpaquePointer? {
    var config = coreConfig()
    var core: OpaquePointer?
    try requireOK(
        inkpod_core_create(&config, &core),
        operation: "inkpod_core_create"
    )
    guard core != nil else {
        throw CoreBridgeFailure(
            operation: "inkpod_core_create",
            status: .panic,
            diagnostic: "success did not publish a Core owner"
        )
    }
    return core
}

private func requireOK(_ rawStatus: UInt32, operation: String) throws {
    let status = CoreStatus(cValue: rawStatus)
    guard status == .ok else {
        throw failure(operation, status: status)
    }
}

private func failure(_ operation: String, status: CoreStatus) -> CoreBridgeFailure {
    CoreBridgeFailure(operation: operation, status: status, diagnostic: copyDiagnostic())
}

private func copyDiagnostic() -> String {
    var required: UInt64 = 0
    guard inkpod_error_message_size(&required) == inkpod_bridge_status_ok(), required > 0 else {
        return ""
    }
    guard required <= 65_536 else {
        return "diagnostic exceeds bridge limit"
    }
    var bytes = [UInt8](repeating: 0, count: Int(required))
    var written: UInt64 = 0
    let status = bytes.withUnsafeMutableBufferPointer { buffer in
        inkpod_error_message_copy(buffer.baseAddress, UInt64(buffer.count), &written)
    }
    guard status == inkpod_bridge_status_ok(), written < UInt64(bytes.count) else {
        return ""
    }
    return String(decoding: bytes.prefix(Int(written)), as: UTF8.self)
}

private func currentThreadID() -> UInt64 {
    UInt64(pthread_mach_thread_np(pthread_self()))
}

private func releaseOnAnotherThread(
    _ snapshot: OwnedSnapshot
) -> (status: CoreStatus, threadID: UInt64) {
    struct ReleaseResult: Sendable {
        let status: CoreStatus
        let threadID: UInt64
    }
    let resultBox = BlockingResultBox<ReleaseResult>()
    let thread = Thread {
        resultBox.complete(
            .success(
                ReleaseResult(
                    status: snapshot.release(),
                    threadID: currentThreadID()
                )
            )
        )
    }
    thread.name = "inkpod.macos.snapshot-release"
    thread.start()
    let result = try! resultBox.wait()
    return (result.status, result.threadID)
}
