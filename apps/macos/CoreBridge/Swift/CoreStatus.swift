import Foundation
import InkpodCoreC

private enum CStatus {
    static let ok = inkpod_bridge_status_ok()
    static let invalidArgument = inkpod_bridge_status_invalid_argument()
    static let incompatibleABI = inkpod_bridge_status_incompatible_abi()
    static let bufferTooSmall = inkpod_bridge_status_buffer_too_small()
    static let unsupported = inkpod_bridge_status_unsupported()
    static let panic = inkpod_bridge_status_panic()
    static let wrongThread = inkpod_bridge_status_wrong_thread()
    static let ioError = inkpod_bridge_status_io_error()
    static let invalidState = inkpod_bridge_status_invalid_state()
    static let noDocument = inkpod_bridge_status_no_document()
    static let cancelled = inkpod_bridge_status_cancelled()
    static let fillOverflow = inkpod_bridge_status_fill_overflow()
    static let unsavedChanges = inkpod_bridge_status_unsaved_changes()
}

public enum CoreStatus: Equatable, Sendable {
    case ok
    case invalidArgument
    case incompatibleABI
    case bufferTooSmall
    case unsupported
    case panic
    case wrongThread
    case ioError
    case invalidState
    case noDocument
    case cancelled
    case fillOverflow
    case unsavedChanges
    case unknown(UInt32)

    public init(cValue: UInt32) {
        switch cValue {
        case CStatus.ok:
            self = .ok
        case CStatus.invalidArgument:
            self = .invalidArgument
        case CStatus.incompatibleABI:
            self = .incompatibleABI
        case CStatus.bufferTooSmall:
            self = .bufferTooSmall
        case CStatus.unsupported:
            self = .unsupported
        case CStatus.panic:
            self = .panic
        case CStatus.wrongThread:
            self = .wrongThread
        case CStatus.ioError:
            self = .ioError
        case CStatus.invalidState:
            self = .invalidState
        case CStatus.noDocument:
            self = .noDocument
        case CStatus.cancelled:
            self = .cancelled
        case CStatus.fillOverflow:
            self = .fillOverflow
        case CStatus.unsavedChanges:
            self = .unsavedChanges
        default:
            self = .unknown(cValue)
        }
    }
}

public struct CoreBridgeFailure: Error, Equatable, Sendable {
    public let operation: String
    public let status: CoreStatus
    public let diagnostic: String
}
