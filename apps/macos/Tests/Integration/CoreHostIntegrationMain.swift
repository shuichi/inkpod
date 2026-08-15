import Foundation
import InkpodCoreBridge

private enum IntegrationFailure: Error {
    case timedOut
    case unexpected(CoreRequestOutcome)
    case ownerThreadMismatch
    case destroyOrderMismatch
}

private func outcome(_ task: CoreTask) throws -> CoreRequestOutcome {
    guard let outcome = task.wait(timeout: 20) else {
        throw IntegrationFailure.timedOut
    }
    return outcome
}

@main
private enum CoreHostIntegrationMain {
    static func main() {
        do {
            let host = CoreHost()
            var sessions: [CoreSessionProjection] = []
            for index in 1...64 {
                let result = try outcome(
                    host.createSession(
                        documentUUID: CoreDocumentUUID(
                            high: 0x4D31_4845_4144,
                            low: UInt64(index)
                        )
                    )
                )
                guard case let .created(session) = result else {
                    throw IntegrationFailure.unexpected(result)
                }
                sessions.append(session)
            }
            guard Set(sessions.map(\.ownerThreadID)).count == 1 else {
                throw IntegrationFailure.ownerThreadMismatch
            }
            guard case let .inspected(inspected) = try outcome(
                host.inspectSession(sessions[31].target)
            ), inspected.target == sessions[31].target else {
                throw IntegrationFailure.ownerThreadMismatch
            }
            guard case let .shutdown(shutdown) = try outcome(host.shutdown()) else {
                throw IntegrationFailure.timedOut
            }
            guard shutdown.destroyedSessionIDs == sessions.map(\.target.id) else {
                throw IntegrationFailure.destroyOrderMismatch
            }
            guard host.waitUntilStopped(timeout: 5) else {
                throw IntegrationFailure.timedOut
            }
            print(
                "inkpod macOS CoreHost integration: "
                    + "64 sessions, one owner thread, ordered shutdown"
            )
        } catch {
            FileHandle.standardError.write(
                Data("inkpod macOS CoreHost integration failed: \(error)\n".utf8)
            )
            exit(EXIT_FAILURE)
        }
    }
}
