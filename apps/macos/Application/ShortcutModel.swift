import AppKit
import Foundation
import InkpodCoreC

public enum ShortcutNamedKey: UInt32, Codable, CaseIterable, Sendable {
    case tab = 1
    case `return` = 2
    case escape = 3
    case space = 4
    case backspace = 5
    case delete = 6
    case left = 7
    case right = 8
    case up = 9
    case down = 10
    case home = 11
    case end = 12
    case pageUp = 13
    case pageDown = 14
    case f1 = 101
    case f2 = 102
    case f3 = 103
    case f4 = 104
    case f5 = 105
    case f6 = 106
    case f7 = 107
    case f8 = 108
    case f9 = 109
    case f10 = 110
    case f11 = 111
    case f12 = 112
    case f13 = 113
    case f14 = 114
    case f15 = 115
    case f16 = 116
    case f17 = 117
    case f18 = 118
    case f19 = 119
    case f20 = 120
    case f21 = 121
    case f22 = 122
    case f23 = 123
    case f24 = 124
}

public enum ShortcutKey: Hashable, Codable, Sendable {
    case unicodeScalar(UInt32)
    case named(ShortcutNamedKey)

    public static func character(_ character: Character) -> Self? {
        let scalars = String(character).unicodeScalars
        guard scalars.count == 1, let scalar = scalars.first else { return nil }
        return .unicodeScalar(scalar.value)
    }
}

public struct ShortcutModifiers: OptionSet, Hashable, Codable, Sendable {
    public let rawValue: UInt32

    public init(rawValue: UInt32) {
        self.rawValue = rawValue
    }

    public static let primary = Self(rawValue: 1 << 0)
    public static let shift = Self(rawValue: 1 << 1)
    public static let alternate = Self(rawValue: 1 << 2)
    public static let control = Self(rawValue: 1 << 3)
    public static let validMask: Self = [.primary, .shift, .alternate, .control]
}

public struct ShortcutStroke: Hashable, Codable, Sendable {
    public let key: ShortcutKey
    public let modifiers: ShortcutModifiers

    public init(key: ShortcutKey, modifiers: ShortcutModifiers = []) {
        self.key = key
        self.modifiers = modifiers
    }

    public static func character(
        _ character: Character,
        modifiers: ShortcutModifiers = []
    ) -> Self {
        precondition(ShortcutKey.character(character) != nil)
        return Self(key: ShortcutKey.character(character)!, modifiers: modifiers)
    }

    fileprivate var cValue: InkpodShortcutStrokeV2 {
        var value = InkpodShortcutStrokeV2()
        value.struct_size = UInt32(MemoryLayout<InkpodShortcutStrokeV2>.size)
        switch key {
        case let .unicodeScalar(scalar):
            value.key_kind = inkpod_bridge_shortcut_key_unicode_scalar()
            value.key_value = scalar
        case let .named(named):
            value.key_kind = inkpod_bridge_shortcut_key_named()
            value.key_value = named.rawValue
        }
        value.modifiers = modifiers.rawValue
        return value
    }

    fileprivate var isValid: Bool {
        guard modifiers.subtracting(.validMask).isEmpty else { return false }
        switch key {
        case let .unicodeScalar(value):
            return UnicodeScalar(value) != nil
        case .named:
            return true
        }
    }
}

public struct ShortcutSequence: Hashable, Codable, Sendable {
    public let strokes: [ShortcutStroke]

    public init?(_ strokes: [ShortcutStroke]) {
        guard (1 ... 4).contains(strokes.count), strokes.allSatisfy(\.isValid) else {
            return nil
        }
        self.strokes = strokes
    }

    fileprivate func cValue(command: InkpodCommandID) -> InkpodShortcutSequenceV2 {
        var value = InkpodShortcutSequenceV2()
        value.struct_size = UInt32(MemoryLayout<InkpodShortcutSequenceV2>.size)
        value.command_id = command.rawValue
        value.stroke_count = UInt32(strokes.count)
        for (index, stroke) in strokes.enumerated() {
            inkpod_bridge_shortcut_sequence_set_stroke(
                &value,
                UInt32(index),
                stroke.cValue
            )
        }
        return value
    }
}

public enum ShortcutMatch: Equatable, Sendable {
    case none
    case prefix
    case exact(InkpodCommandID)
    case invalid
}

public enum ShortcutResolver {
    public static func resolve(
        bindings: [InkpodCommandID: ShortcutSequence],
        entered: [ShortcutStroke]
    ) -> ShortcutMatch {
        guard (1 ... 4).contains(entered.count), entered.allSatisfy(\.isValid) else {
            return .invalid
        }
        let table = bindings.sorted { $0.key.rawValue < $1.key.rawValue }
            .map { $0.value.cValue(command: $0.key) }
        let input = entered.map(\.cValue)
        var match: InkpodShortcutMatch = 0
        var commandID: UInt32 = 0
        let status = table.withUnsafeBufferPointer { tableBuffer in
            input.withUnsafeBufferPointer { inputBuffer in
                inkpod_shortcut_sequence_resolve_v2(
                    tableBuffer.baseAddress,
                    UInt64(tableBuffer.count),
                    UInt64(MemoryLayout<InkpodShortcutSequenceV2>.stride),
                    inputBuffer.baseAddress,
                    UInt32(inputBuffer.count),
                    &match,
                    &commandID
                )
            }
        }
        guard CoreStatus(cValue: status) == .ok else { return .invalid }
        if match == inkpod_bridge_shortcut_match_prefix() {
            return .prefix
        }
        if match == inkpod_bridge_shortcut_match_exact(),
           let command = InkpodCommandID(rawValue: commandID)
        {
            return .exact(command)
        }
        return .none
    }
}

public enum ShortcutEditResult: Equatable, Sendable {
    case applied
    case noOp
    case invalid
    case prefixConflict
    case protectedStandard
    case persistenceFailure
}

public protocol ShortcutPersistence: AnyObject {
    func load() throws -> Data?
    func save(_ data: Data) throws
}

public final class UserDefaultsShortcutPersistence: ShortcutPersistence {
    private let defaults: UserDefaults
    private let key: String

    public init(defaults: UserDefaults = .standard, key: String = "shortcut-bindings-v2") {
        self.defaults = defaults
        self.key = key
    }

    public func load() throws -> Data? {
        defaults.data(forKey: key)
    }

    public func save(_ data: Data) throws {
        defaults.set(data, forKey: key)
    }
}

@MainActor
public final class ShortcutController: ObservableObject {
    private struct PersistedBindings: Codable {
        let version: UInt32
        let bindings: [InkpodCommandID: ShortcutSequence]
    }

    public static let timeout: TimeInterval = 1.5
    public static let defaults: [InkpodCommandID: ShortcutSequence] = [
        .appExit: ShortcutSequence([.character("Q", modifiers: .primary)])!,
        .fileNew: ShortcutSequence([.character("N", modifiers: .primary)])!,
        .fileOpen: ShortcutSequence([.character("O", modifiers: .primary)])!,
        .fileSave: ShortcutSequence([.character("S", modifiers: .primary)])!,
        .fileSaveAs: ShortcutSequence([.character("S", modifiers: [.primary, .shift])])!,
        .documentClose: ShortcutSequence([.character("W", modifiers: [.primary, .shift])])!,
        .undo: ShortcutSequence([.character("Z", modifiers: .primary)])!,
        .redo: ShortcutSequence([.character("Z", modifiers: [.primary, .shift])])!,
        .editCopy: ShortcutSequence([.character("C", modifiers: .primary)])!,
        .editPaste: ShortcutSequence([.character("V", modifiers: .primary)])!,
        .editCut: ShortcutSequence([.character("X", modifiers: .primary)])!,
        .zoomIn: ShortcutSequence([.character("=", modifiers: .primary)])!,
        .zoomOut: ShortcutSequence([.character("-", modifiers: .primary)])!,
        .fit: ShortcutSequence([.character("0", modifiers: .primary)])!,
        .oneToOne: ShortcutSequence([.character("1", modifiers: .primary)])!,
        .grid: ShortcutSequence([.character("'", modifiers: .primary)])!,
        .helpManual: ShortcutSequence([
            ShortcutStroke(key: .named(.f1)),
        ])!,
    ]

    @Published public private(set) var bindings: [InkpodCommandID: ShortcutSequence]
    @Published public private(set) var pendingStrokes: [ShortcutStroke] = []
    @Published public private(set) var captureCommand: InkpodCommandID?
    @Published public private(set) var capturedStrokes: [ShortcutStroke] = []
    @Published public private(set) var lastEditResult: ShortcutEditResult?
    private let persistence: ShortcutPersistence
    private var pendingDeadline: TimeInterval?

    public init(persistence: ShortcutPersistence = UserDefaultsShortcutPersistence()) {
        var initialBindings = Self.defaults
        var shouldPersistMigration = false
        if let data = try? persistence.load(),
           let persisted = try? JSONDecoder().decode(PersistedBindings.self, from: data),
           persisted.version == 2,
           Self.validate(persisted.bindings)
        {
            initialBindings = persisted.bindings
            for command in [InkpodCommandID.fileNew, .undo, .redo]
                where initialBindings[command] == nil
            {
                if let standard = Self.defaults[command] {
                    var candidate = initialBindings
                    if let conflicting = candidate.first(where: {
                        $0.key != command && $0.value == standard
                    })?.key {
                        candidate.removeValue(forKey: conflicting)
                    }
                    candidate[command] = standard
                    if Self.validate(candidate) {
                        initialBindings = candidate
                        shouldPersistMigration = true
                    }
                }
            }
        }
        self.persistence = persistence
        bindings = initialBindings
        if shouldPersistMigration {
            _ = persist(initialBindings)
        }
    }

    public func sequence(for command: InkpodCommandID) -> ShortcutSequence? {
        bindings[command]
    }

    @discardableResult
    public func rebind(
        command: InkpodCommandID,
        to sequence: ShortcutSequence
    ) -> ShortcutEditResult {
        guard !Self.isProtectedStandard(sequence, for: command) else {
            return .protectedStandard
        }
        if bindings[command] == sequence { return .noOp }
        for (candidateCommand, candidate) in bindings where candidateCommand != command {
            if Self.hasStrictPrefixConflict(sequence, candidate) {
                return .prefixConflict
            }
        }

        var replacement = bindings
        let oldSequence = replacement[command]
        if let exactOwner = replacement.first(where: {
            $0.key != command && $0.value == sequence
        })?.key {
            if let oldSequence {
                replacement[exactOwner] = oldSequence
            } else {
                replacement.removeValue(forKey: exactOwner)
            }
        }
        replacement[command] = sequence
        guard Self.validate(replacement) else { return .invalid }
        guard persist(replacement) else { return .persistenceFailure }
        bindings = replacement
        cancelPending()
        return .applied
    }

    @discardableResult
    public func reset() -> ShortcutEditResult {
        if bindings == Self.defaults { return .noOp }
        guard persist(Self.defaults) else { return .persistenceFailure }
        bindings = Self.defaults
        cancelPending()
        return .applied
    }

    public func consume(
        _ stroke: ShortcutStroke,
        now: TimeInterval = ProcessInfo.processInfo.systemUptime
    ) -> ShortcutMatch {
        if let deadline = pendingDeadline, now >= deadline {
            cancelPending()
        }
        let entered = pendingStrokes + [stroke]
        let result = ShortcutResolver.resolve(bindings: bindings, entered: entered)
        switch result {
        case .prefix:
            pendingStrokes = entered
            pendingDeadline = now + Self.timeout
        case .exact, .none, .invalid:
            cancelPending()
        }
        return result
    }

    public func expirePending(now: TimeInterval = ProcessInfo.processInfo.systemUptime) {
        guard let deadline = pendingDeadline, now >= deadline else { return }
        cancelPending()
    }

    public func cancelPending() {
        pendingStrokes = []
        pendingDeadline = nil
    }

    public func beginCapture(for command: InkpodCommandID) {
        captureCommand = command
        capturedStrokes = []
        lastEditResult = nil
        cancelPending()
    }

    public func appendCapturedStroke(_ stroke: ShortcutStroke) -> Bool {
        guard captureCommand != nil, capturedStrokes.count < 4, stroke.isValid else {
            return false
        }
        capturedStrokes.append(stroke)
        return true
    }

    @discardableResult
    public func commitCapture() -> ShortcutEditResult {
        guard let command = captureCommand,
              let sequence = ShortcutSequence(capturedStrokes)
        else {
            lastEditResult = .invalid
            return .invalid
        }
        let result = rebind(command: command, to: sequence)
        lastEditResult = result
        if result == .applied || result == .noOp {
            captureCommand = nil
            capturedStrokes = []
        }
        return result
    }

    public func cancelCapture() {
        captureCommand = nil
        capturedStrokes = []
        lastEditResult = nil
    }

    private func persist(_ replacement: [InkpodCommandID: ShortcutSequence]) -> Bool {
        do {
            let data = try JSONEncoder().encode(
                PersistedBindings(version: 2, bindings: replacement)
            )
            try persistence.save(data)
            return true
        } catch {
            return false
        }
    }

    private static func validate(_ bindings: [InkpodCommandID: ShortcutSequence]) -> Bool {
        let values = Array(bindings.values)
        for (index, left) in values.enumerated() {
            for right in values.dropFirst(index + 1) where conflicts(left, right) {
                return false
            }
        }
        return bindings.keys.allSatisfy { InkpodCommandID(rawValue: $0.rawValue) != nil }
    }

    private static func conflicts(_ left: ShortcutSequence, _ right: ShortcutSequence) -> Bool {
        left.strokes.starts(with: right.strokes) || right.strokes.starts(with: left.strokes)
    }

    private static func hasStrictPrefixConflict(
        _ left: ShortcutSequence,
        _ right: ShortcutSequence
    ) -> Bool {
        left != right && conflicts(left, right)
    }

    private static func isProtectedStandard(
        _ sequence: ShortcutSequence,
        for command: InkpodCommandID
    ) -> Bool {
        let reserved: Set<ShortcutSequence> = [
            ShortcutSequence([.character("A", modifiers: .primary)])!,
        ]
        if reserved.contains(sequence) { return true }
        let assigned: [ShortcutSequence: InkpodCommandID] = [
            ShortcutSequence([.character("Q", modifiers: .primary)])!: .appExit,
            ShortcutSequence([.character("N", modifiers: .primary)])!: .fileNew,
            ShortcutSequence([.character("O", modifiers: .primary)])!: .fileOpen,
            ShortcutSequence([.character("S", modifiers: .primary)])!: .fileSave,
            ShortcutSequence([.character("S", modifiers: [.primary, .shift])])!: .fileSaveAs,
            ShortcutSequence([.character("W", modifiers: [.primary, .shift])])!: .documentClose,
            ShortcutSequence([.character("Z", modifiers: .primary)])!: .undo,
            ShortcutSequence([.character("Z", modifiers: [.primary, .shift])])!: .redo,
            ShortcutSequence([.character("C", modifiers: .primary)])!: .editCopy,
            ShortcutSequence([.character("V", modifiers: .primary)])!: .editPaste,
            ShortcutSequence([.character("X", modifiers: .primary)])!: .editCut,
            ShortcutSequence([.character(",", modifiers: .primary)])!: .shortcutEdit,
        ]
        return assigned[sequence].map { $0 != command } ?? false
    }
}

extension ShortcutStroke: CustomStringConvertible {
    public var description: String {
        var parts: [String] = []
        if modifiers.contains(.control) { parts.append("⌃") }
        if modifiers.contains(.alternate) { parts.append("⌥") }
        if modifiers.contains(.shift) { parts.append("⇧") }
        if modifiers.contains(.primary) { parts.append("⌘") }
        switch key {
        case let .unicodeScalar(value):
            parts.append(UnicodeScalar(value).map(String.init) ?? "?")
        case let .named(key):
            parts.append(key.displayName)
        }
        return parts.joined()
    }
}

extension ShortcutSequence: CustomStringConvertible {
    public var description: String {
        strokes.map(\.description).joined(separator: " ")
    }
}

private extension ShortcutNamedKey {
    var displayName: String {
        switch self {
        case .tab: "⇥"
        case .return: "↩"
        case .escape: "⎋"
        case .space: "Space"
        case .backspace: "⌫"
        case .delete: "⌦"
        case .left: "←"
        case .right: "→"
        case .up: "↑"
        case .down: "↓"
        case .home: "Home"
        case .end: "End"
        case .pageUp: "Page Up"
        case .pageDown: "Page Down"
        default: "F\(rawValue - 100)"
        }
    }
}

public enum ShortcutInputGuard {
    public static func shouldHandle(
        isTextInput: Bool,
        hasMarkedText: Bool,
        stroke: ShortcutStroke
    ) -> Bool {
        if hasMarkedText { return false }
        if isTextInput { return false }
        return true
    }
}
