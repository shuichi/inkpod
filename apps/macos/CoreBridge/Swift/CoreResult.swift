import Foundation

public final class CoreTask: @unchecked Sendable {
    public let requestID: CoreRequestID

    private let condition = NSCondition()
    private var outcome: CoreRequestOutcome?
    private var continuations: [CheckedContinuation<CoreRequestOutcome, Never>] = []
    private var completedCount = 0

    init(requestID: CoreRequestID) {
        self.requestID = requestID
    }

    public var completionCount: Int {
        condition.lock()
        let count = completedCount
        condition.unlock()
        return count
    }

    public func wait(timeout: TimeInterval) -> CoreRequestOutcome? {
        let deadline = Date(timeIntervalSinceNow: timeout)
        condition.lock()
        while outcome == nil, condition.wait(until: deadline) {}
        let completed = outcome
        condition.unlock()
        return completed
    }

    public func value() async -> CoreRequestOutcome {
        await withCheckedContinuation { continuation in
            condition.lock()
            if let outcome {
                condition.unlock()
                continuation.resume(returning: outcome)
            } else {
                continuations.append(continuation)
                condition.unlock()
            }
        }
    }

    @discardableResult
    func complete(_ outcome: CoreRequestOutcome) -> Bool {
        condition.lock()
        guard self.outcome == nil else {
            condition.unlock()
            return false
        }
        self.outcome = outcome
        completedCount += 1
        let waiters = continuations
        continuations.removeAll(keepingCapacity: false)
        condition.broadcast()
        condition.unlock()

        for continuation in waiters {
            continuation.resume(returning: outcome)
        }
        return true
    }
}

final class CoreCompletionRegistry: @unchecked Sendable {
    private let lock = NSLock()
    private var tasks: [CoreRequestID: CoreTask] = [:]

    func register(requestID: CoreRequestID) -> CoreTask {
        let task = CoreTask(requestID: requestID)
        lock.withLock {
            precondition(tasks[requestID] == nil, "request IDs must be unique")
            tasks[requestID] = task
        }
        return task
    }

    func isPending(_ requestID: CoreRequestID) -> Bool {
        lock.withLock { tasks[requestID] != nil }
    }

    @discardableResult
    func complete(_ requestID: CoreRequestID, with outcome: CoreRequestOutcome) -> Bool {
        let task = lock.withLock { tasks.removeValue(forKey: requestID) }
        return task?.complete(outcome) ?? false
    }
}
