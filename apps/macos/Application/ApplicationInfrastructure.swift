import AppKit
import Foundation

public enum AppLanguageSelection: String, Codable, CaseIterable, Sendable {
    case system
    case japanese
    case english
}

@MainActor
public final class SequenceEndpointPolicyController: ObservableObject {
    private struct PersistedPolicy: Codable {
        let version: UInt32
        let policy: CoreSequenceEndpointPolicy
    }

    @Published public private(set) var policy: CoreSequenceEndpointPolicy
    private let defaults: UserDefaults
    private let key: String

    public init(
        defaults: UserDefaults = .standard,
        key: String = "sequence-endpoint-policy-v1"
    ) {
        self.defaults = defaults
        self.key = key
        if let data = defaults.data(forKey: key), data.count <= 4_096,
           let record = try? JSONDecoder().decode(PersistedPolicy.self, from: data),
           record.version == 1
        {
            policy = record.policy
        } else {
            policy = .stop
        }
    }

    @discardableResult
    public func set(_ value: CoreSequenceEndpointPolicy) -> Bool {
        guard value != policy,
              let data = try? JSONEncoder().encode(
                  PersistedPolicy(version: 1, policy: value)
              ),
              data.count <= 4_096
        else {
            return false
        }
        defaults.set(data, forKey: key)
        policy = value
        return true
    }

    @discardableResult
    public func toggle() -> Bool {
        set(policy == .stop ? .wrap : .stop)
    }
}

@MainActor
public final class AppLanguageController: ObservableObject {
    private struct PersistedLanguage: Codable {
        let version: UInt32
        let selection: AppLanguageSelection
    }

    @Published public private(set) var selection: AppLanguageSelection
    public let activeLanguageCode: String
    private let defaults: UserDefaults
    private let key: String

    public init(defaults: UserDefaults = .standard, key: String = "language-selection-v1") {
        self.defaults = defaults
        self.key = key
        let initialSelection: AppLanguageSelection
        if let data = defaults.data(forKey: key),
           let value = try? JSONDecoder().decode(PersistedLanguage.self, from: data),
           value.version == 1
        {
            initialSelection = value.selection
        } else {
            initialSelection = .system
        }
        selection = initialSelection
        activeLanguageCode = Self.languageCode(for: initialSelection)
    }

    public var effectiveLanguageCode: String {
        activeLanguageCode
    }

    private static func languageCode(for selection: AppLanguageSelection) -> String {
        switch selection {
        case .japanese: "ja"
        case .english: "en"
        case .system:
            Locale.preferredLanguages.first?.lowercased().hasPrefix("ja") == true ? "ja" : "en"
        }
    }

    @discardableResult
    public func select(_ value: AppLanguageSelection) -> Bool {
        if value == selection { return false }
        guard let data = try? JSONEncoder().encode(
            PersistedLanguage(version: 1, selection: value)
        ) else {
            return false
        }
        defaults.set(data, forKey: key)
        selection = value
        return true
    }

    public func text(_ key: String) -> String {
        String(
            localized: String.LocalizationValue(key),
            bundle: .main,
            locale: Locale(identifier: effectiveLanguageCode)
        )
    }

    public func commandLabel(_ command: InkpodCommandID) -> String {
        text(CommandCatalog.descriptor(for: command).localizationKey)
    }
}

@MainActor
final class HelpPresenter {
    struct AboutPanelContent: Equatable {
        let description: String
    }

    static func aboutPanelContent(localizer: AppLanguageController) -> AboutPanelContent {
        AboutPanelContent(
            description: localizer.text("about.description")
        )
    }

    func present(_ command: InkpodCommandID, localizer: AppLanguageController) -> Bool {
        switch command {
        case .helpAbout:
            let content = Self.aboutPanelContent(localizer: localizer)
            NSApp.orderFrontStandardAboutPanel(options: [
                .credits: NSAttributedString(string: content.description),
            ])
            return true
        case .helpManual:
            show(
                title: localizer.commandLabel(.helpManual),
                body: localizer.text("help.manual.body")
            )
            return true
        case .helpFileFormat:
            show(
                title: localizer.commandLabel(.helpFileFormat),
                body: localizer.text("help.file.format.body")
            )
            return true
        case .helpAcknowledgements:
            show(
                title: localizer.commandLabel(.helpAcknowledgements),
                body: localizer.text("help.acknowledgements.body")
            )
            return true
        case .helpWebPage:
            guard let url = URL(string: "https://shuichi.github.io/inkpod/") else {
                return false
            }
            return NSWorkspace.shared.open(url)
        default:
            return false
        }
    }

    private func show(title: String, body: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = body
        alert.alertStyle = .informational
        alert.addButton(withTitle: "OK")
        if let window = NSApp.keyWindow {
            alert.beginSheetModal(for: window)
        } else {
            alert.runModal()
        }
    }
}

@MainActor
enum ShortcutEventNormalizer {
    static func stroke(from event: NSEvent) -> ShortcutStroke? {
        guard event.type == .keyDown else { return nil }
        var modifiers: ShortcutModifiers = []
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.command) { modifiers.insert(.primary) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        if flags.contains(.option) { modifiers.insert(.alternate) }
        if flags.contains(.control) { modifiers.insert(.control) }
        if let named = namedKey(for: event.keyCode) {
            return ShortcutStroke(key: .named(named), modifiers: modifiers)
        }
        guard let characters = event.charactersIgnoringModifiers?.uppercased(),
              characters.unicodeScalars.count == 1,
              let scalar = characters.unicodeScalars.first
        else {
            return nil
        }
        return ShortcutStroke(key: .unicodeScalar(scalar.value), modifiers: modifiers)
    }

    private static func namedKey(for keyCode: UInt16) -> ShortcutNamedKey? {
        switch keyCode {
        case 36, 76: .return
        case 48: .tab
        case 49: .space
        case 51: .backspace
        case 53: .escape
        case 117: .delete
        case 123: .left
        case 124: .right
        case 125: .down
        case 126: .up
        case 115: .home
        case 119: .end
        case 116: .pageUp
        case 121: .pageDown
        case 122: .f1
        case 120: .f2
        case 99: .f3
        case 118: .f4
        case 96: .f5
        case 97: .f6
        case 98: .f7
        case 100: .f8
        case 101: .f9
        case 109: .f10
        case 103: .f11
        case 111: .f12
        case 105: .f13
        case 107: .f14
        case 113: .f15
        case 106: .f16
        case 64: .f17
        case 79: .f18
        case 80: .f19
        case 90: .f20
        default: nil
        }
    }
}

@MainActor
final class ShortcutEventMonitor {
    private struct EventBox: @unchecked Sendable {
        let event: NSEvent
    }

    private weak var application: ApplicationCoordinator?
    private var token: Any?

    func start(application: ApplicationCoordinator) {
        guard token == nil else { return }
        self.application = application
        token = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            let box = EventBox(event: event)
            let consumed = MainActor.assumeIsolated {
                self?.consume(box.event) ?? false
            }
            return consumed ? nil : event
        }
    }

    func stop() {
        if let token {
            NSEvent.removeMonitor(token)
        }
        token = nil
        application = nil
    }

    private func consume(_ event: NSEvent) -> Bool {
        guard let application, let stroke = ShortcutEventNormalizer.stroke(from: event) else {
            return false
        }
        let shortcuts = application.shortcutController
        if shortcuts.captureCommand != nil {
            if stroke.key == .named(.escape) {
                shortcuts.cancelCapture()
            } else {
                _ = shortcuts.appendCapturedStroke(stroke)
            }
            return true
        }

        let responder = event.window?.firstResponder
        let inputClient = responder as? NSTextInputClient
        guard ShortcutInputGuard.shouldHandle(
            isTextInput: inputClient != nil,
            hasMarkedText: inputClient?.hasMarkedText() ?? false,
            stroke: stroke
        ) else {
            shortcuts.cancelPending()
            return false
        }

        switch shortcuts.consume(stroke) {
        case .prefix:
            return true
        case let .exact(command):
            let context = application.commandContext(for: event.window)
            _ = application.route(command, context: context)
            return true
        case .none, .invalid:
            return false
        }
    }
}
