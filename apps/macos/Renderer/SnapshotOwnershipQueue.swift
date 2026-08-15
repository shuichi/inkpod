import Foundation

enum SnapshotSubmissionResult: Equatable {
    case accepted
    case replacedPending
    case rejectedStaleRoute
    case rejectedHidden
    case rejectedCapacity
    case rejectedStopped
}

enum SnapshotRetentionResult: Equatable {
    case retained
    case rejectedStaleRoute
    case rejectedStopped
}

final class SnapshotOwnershipQueue: @unchecked Sendable {
    private struct SurfaceState {
        var binding: CoreSnapshotRoute
        var visible = true
        var pending: CoreSnapshotEnvelope?
        var retained: CoreSnapshotEnvelope?
    }

    private let lock = NSLock()
    private let capacity: Int
    private var surfaces: [CoreSurfaceID: SurfaceState] = [:]
    private var stopped = false

    init(capacity: Int = 64) {
        precondition(capacity > 0)
        self.capacity = capacity
    }

    func registerSurface(
        _ surface: CoreSurfaceTarget,
        binding: CoreSnapshotRoute
    ) -> Bool {
        lock.withLock {
            guard !stopped,
                  surface.id.rawValue != 0,
                  surface.generation.rawValue != 0,
                  binding.surface == surface,
                  binding.session == binding.view.session,
                  surfaces[surface.id] == nil
            else {
                return false
            }
            surfaces[surface.id] = SurfaceState(binding: binding)
            return true
        }
    }

    func submit(_ envelope: CoreSnapshotEnvelope) -> SnapshotSubmissionResult {
        lock.withLock {
            guard !stopped else {
                envelope.owner.release()
                return .rejectedStopped
            }
            guard var state = surfaces[envelope.route.surface.id],
                  state.binding == envelope.route
            else {
                envelope.owner.release()
                return .rejectedStaleRoute
            }
            guard state.visible else {
                envelope.owner.release()
                return .rejectedHidden
            }
            if state.pending != nil {
                release(&state.pending)
                state.pending = envelope
                surfaces[envelope.route.surface.id] = state
                return .replacedPending
            }
            let pendingCount = surfaces.values.reduce(into: 0) { count, surface in
                if surface.pending != nil {
                    count += 1
                }
            }
            guard pendingCount < capacity else {
                envelope.owner.release()
                return .rejectedCapacity
            }
            state.pending = envelope
            surfaces[envelope.route.surface.id] = state
            return .accepted
        }
    }

    func takeNext() -> CoreSnapshotEnvelope? {
        lock.withLock {
            guard !stopped else { return nil }
            let surfaceID = surfaces.keys.sorted { $0.rawValue < $1.rawValue }.first {
                surfaces[$0]?.pending != nil
            }
            guard let surfaceID, var state = surfaces[surfaceID] else {
                return nil
            }
            let envelope = state.pending
            state.pending = nil
            surfaces[surfaceID] = state
            return envelope
        }
    }

    func retainRendered(_ envelope: CoreSnapshotEnvelope) -> SnapshotRetentionResult {
        lock.withLock {
            guard !stopped else {
                envelope.owner.release()
                return .rejectedStopped
            }
            guard var state = surfaces[envelope.route.surface.id],
                  state.binding == envelope.route,
                  state.visible
            else {
                envelope.owner.release()
                return .rejectedStaleRoute
            }
            release(&state.retained)
            state.retained = envelope
            surfaces[envelope.route.surface.id] = state
            return .retained
        }
    }

    func retainedSnapshot(for surface: CoreSurfaceTarget) -> CoreSnapshotEnvelope? {
        lock.withLock {
            guard !stopped,
                  let state = surfaces[surface.id],
                  state.binding.surface == surface
            else {
                return nil
            }
            return state.retained
        }
    }

    func setSurfaceVisible(_ surface: CoreSurfaceTarget, visible: Bool) {
        lock.withLock {
            guard !stopped,
                  var state = surfaces[surface.id],
                  state.binding.surface == surface
            else {
                return
            }
            state.visible = visible
            if !visible {
                release(&state.pending)
            }
            surfaces[surface.id] = state
        }
    }

    func closeSurface(_ surface: CoreSurfaceTarget) {
        lock.withLock {
            guard var state = surfaces[surface.id], state.binding.surface == surface else {
                return
            }
            surfaces.removeValue(forKey: surface.id)
            release(&state.pending)
            release(&state.retained)
        }
    }

    func shutdown() {
        lock.withLock {
            guard !stopped else { return }
            stopped = true
            for var state in surfaces.values {
                release(&state.pending)
                release(&state.retained)
            }
            surfaces.removeAll(keepingCapacity: false)
        }
    }

    private func release(_ envelope: inout CoreSnapshotEnvelope?) {
        envelope?.owner.release()
        envelope = nil
    }
}
