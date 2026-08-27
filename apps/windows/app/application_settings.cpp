#include "application_settings.h"

#include <windows.h>

#include <algorithm>
#include <array>
#include <charconv>
#include <cstddef>
#include <cstdint>
#include <initializer_list>
#include <limits>
#include <new>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "application_data_paths.h"
#include "ui/command_catalog.h"

namespace inkpod::app {
namespace {

using windows::ui::DockLayoutRecord;
using windows::ui::DockPaneType;
using windows::ui::DockStackMode;
using windows::ui::DockZone;
using windows::ui::ShortcutAction;
using windows::ui::ShortcutContext;
using windows::ui::ShortcutInputStroke;
using windows::ui::ShortcutKeyMatch;
using windows::ui::ShortcutKeyboardLayout;
using windows::ui::ShortcutProfile;
using windows::ui::ShortcutProfileBinding;
using windows::ui::ShortcutProfileSet;
using windows::ui::ShortcutSlot;
using windows::ui::ToolTab;
using windows::ui::ToolTabId;
using windows::ui::UiLanguagePreference;
using windows::ui::WorkspaceAutoHideEdge;
using windows::ui::WorkspaceAuxiliaryPane;
using windows::ui::WorkspaceDensity;
using windows::ui::WorkspaceLayoutState;
using windows::ui::WorkspacePreset;
using windows::ui::WorkspaceSplitOrientation;

constexpr char kSettingsFormat[] = "inkpod-settings";
constexpr std::size_t kMaximumJsonDepth = 32U;
constexpr std::size_t kMaximumJsonMembers = 64U * 1024U;

enum class JsonKind : std::uint8_t {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
};

struct JsonValue final {
    JsonKind kind{JsonKind::Null};
    bool boolean{};
    std::int64_t number{};
    std::string string;
    std::vector<JsonValue> array;
    std::vector<std::pair<std::string, JsonValue>> object;
};

bool AppendUtf8CodePoint(std::string& output, std::uint32_t value) {
    if (value <= 0x7fU) {
        output.push_back(static_cast<char>(value));
    } else if (value <= 0x7ffU) {
        output.push_back(static_cast<char>(0xc0U | (value >> 6U)));
        output.push_back(static_cast<char>(0x80U | (value & 0x3fU)));
    } else if (value <= 0xffffU && (value < 0xd800U || value > 0xdfffU)) {
        output.push_back(static_cast<char>(0xe0U | (value >> 12U)));
        output.push_back(static_cast<char>(0x80U | ((value >> 6U) & 0x3fU)));
        output.push_back(static_cast<char>(0x80U | (value & 0x3fU)));
    } else if (value <= 0x10ffffU) {
        output.push_back(static_cast<char>(240U | (value >> 18U)));
        output.push_back(static_cast<char>(0x80U | ((value >> 12U) & 0x3fU)));
        output.push_back(static_cast<char>(0x80U | ((value >> 6U) & 0x3fU)));
        output.push_back(static_cast<char>(0x80U | (value & 0x3fU)));
    } else {
        return false;
    }
    return true;
}

class JsonParser final {
public:
    explicit JsonParser(std::string_view input) noexcept : input_(input) {}

    bool Parse(JsonValue& output) {
        if (input_.size() >= 3U
            && static_cast<unsigned char>(input_[0]) == 239U
            && static_cast<unsigned char>(input_[1]) == 187U
            && static_cast<unsigned char>(input_[2]) == 191U) {
            cursor_ = 3U;
        }
        SkipWhitespace();
        if (!ParseValue(output, 0U)) {
            return false;
        }
        SkipWhitespace();
        return cursor_ == input_.size();
    }

private:
    void SkipWhitespace() noexcept {
        while (cursor_ < input_.size()) {
            const char value = input_[cursor_];
            if (value != ' ' && value != '\t' && value != '\r' && value != '\n') {
                break;
            }
            ++cursor_;
        }
    }

    bool Consume(char expected) noexcept {
        if (cursor_ >= input_.size() || input_[cursor_] != expected) {
            return false;
        }
        ++cursor_;
        return true;
    }

    bool ConsumeLiteral(std::string_view literal) noexcept {
        if (cursor_ > input_.size()
            || input_.size() - cursor_ < literal.size()
            || input_.substr(cursor_, literal.size()) != literal) {
            return false;
        }
        cursor_ += literal.size();
        return true;
    }

    bool ParseHex4(std::uint32_t& output) noexcept {
        if (cursor_ > input_.size() || input_.size() - cursor_ < 4U) {
            return false;
        }
        output = 0U;
        for (std::size_t index = 0U; index < 4U; ++index) {
            const char value = input_[cursor_++];
            std::uint32_t digit{};
            if (value >= '0' && value <= '9') {
                digit = static_cast<std::uint32_t>(value - '0');
            } else if (value >= 'a' && value <= 'f') {
                digit = static_cast<std::uint32_t>(value - 'a' + 10);
            } else if (value >= 'A' && value <= 'F') {
                digit = static_cast<std::uint32_t>(value - 'A' + 10);
            } else {
                return false;
            }
            output = (output << 4U) | digit;
        }
        return true;
    }

    bool ParseString(std::string& output) {
        if (!Consume('"')) {
            return false;
        }
        output.clear();
        while (cursor_ < input_.size()) {
            const unsigned char value =
                static_cast<unsigned char>(input_[cursor_++]);
            if (value == '"') {
                return true;
            }
            if (value < 0x20U) {
                return false;
            }
            if (value != '\\') {
                output.push_back(static_cast<char>(value));
                continue;
            }
            if (cursor_ >= input_.size()) {
                return false;
            }
            const char escape = input_[cursor_++];
            switch (escape) {
            case '"': output.push_back('"'); break;
            case '\\': output.push_back('\\'); break;
            case '/': output.push_back('/'); break;
            case 'b': output.push_back('\b'); break;
            case 'f': output.push_back('\f'); break;
            case 'n': output.push_back('\n'); break;
            case 'r': output.push_back('\r'); break;
            case 't': output.push_back('\t'); break;
            case 'u': {
                std::uint32_t first{};
                if (!ParseHex4(first)) {
                    return false;
                }
                std::uint32_t code_point = first;
                if (first >= 0xd800U && first <= 0xdbffU) {
                    if (!Consume('\\') || !Consume('u')) {
                        return false;
                    }
                    std::uint32_t second{};
                    if (!ParseHex4(second) || second < 0xdc00U
                        || second > 0xdfffU) {
                        return false;
                    }
                    code_point = 0x10000U
                        + ((first - 0xd800U) << 10U) + (second - 0xdc00U);
                } else if (first >= 0xdc00U && first <= 0xdfffU) {
                    return false;
                }
                if (!AppendUtf8CodePoint(output, code_point)) {
                    return false;
                }
                break;
            }
            default:
                return false;
            }
        }
        return false;
    }

    bool ParseNumber(JsonValue& output) noexcept {
        const std::size_t begin = cursor_;
        if (cursor_ < input_.size() && input_[cursor_] == '-') {
            ++cursor_;
        }
        if (cursor_ >= input_.size()) {
            return false;
        }
        if (input_[cursor_] == '0') {
            ++cursor_;
            if (cursor_ < input_.size() && input_[cursor_] >= '0'
                && input_[cursor_] <= '9') {
                return false;
            }
        } else {
            if (input_[cursor_] < '1' || input_[cursor_] > '9') {
                return false;
            }
            while (cursor_ < input_.size() && input_[cursor_] >= '0'
                && input_[cursor_] <= '9') {
                ++cursor_;
            }
        }
        if (cursor_ < input_.size()
            && (input_[cursor_] == '.' || input_[cursor_] == 'e'
                || input_[cursor_] == 'E')) {
            return false;
        }
        const char* first = input_.data() + begin;
        const char* last = input_.data() + cursor_;
        std::int64_t value{};
        const auto parsed = std::from_chars(first, last, value);
        if (parsed.ec != std::errc{} || parsed.ptr != last) {
            return false;
        }
        output.kind = JsonKind::Number;
        output.number = value;
        return true;
    }

    bool ParseArray(JsonValue& output, std::size_t depth) {
        if (!Consume('[')) {
            return false;
        }
        output.kind = JsonKind::Array;
        SkipWhitespace();
        if (Consume(']')) {
            return true;
        }
        while (output.array.size() < kMaximumJsonMembers) {
            JsonValue item{};
            if (!ParseValue(item, depth + 1U)) {
                return false;
            }
            output.array.push_back(std::move(item));
            SkipWhitespace();
            if (Consume(']')) {
                return true;
            }
            if (!Consume(',')) {
                return false;
            }
            SkipWhitespace();
        }
        return false;
    }

    bool ParseObject(JsonValue& output, std::size_t depth) {
        if (!Consume('{')) {
            return false;
        }
        output.kind = JsonKind::Object;
        SkipWhitespace();
        if (Consume('}')) {
            return true;
        }
        while (output.object.size() < kMaximumJsonMembers) {
            std::string name;
            if (!ParseString(name)
                || std::any_of(
                    output.object.begin(), output.object.end(),
                    [&name](const auto& item) { return item.first == name; })) {
                return false;
            }
            SkipWhitespace();
            if (!Consume(':')) {
                return false;
            }
            SkipWhitespace();
            JsonValue value{};
            if (!ParseValue(value, depth + 1U)) {
                return false;
            }
            output.object.emplace_back(std::move(name), std::move(value));
            SkipWhitespace();
            if (Consume('}')) {
                return true;
            }
            if (!Consume(',')) {
                return false;
            }
            SkipWhitespace();
        }
        return false;
    }

    bool ParseValue(JsonValue& output, std::size_t depth) {
        if (depth > kMaximumJsonDepth || cursor_ >= input_.size()) {
            return false;
        }
        switch (input_[cursor_]) {
        case 'n':
            output.kind = JsonKind::Null;
            return ConsumeLiteral("null");
        case 't':
            output.kind = JsonKind::Boolean;
            output.boolean = true;
            return ConsumeLiteral("true");
        case 'f':
            output.kind = JsonKind::Boolean;
            output.boolean = false;
            return ConsumeLiteral("false");
        case '"':
            output.kind = JsonKind::String;
            return ParseString(output.string);
        case '[':
            return ParseArray(output, depth);
        case '{':
            return ParseObject(output, depth);
        default:
            return ParseNumber(output);
        }
    }

    std::string_view input_;
    std::size_t cursor_{};
};

const JsonValue* Member(const JsonValue& object, std::string_view name) noexcept {
    if (object.kind != JsonKind::Object) {
        return nullptr;
    }
    for (const auto& item : object.object) {
        if (item.first == name) {
            return &item.second;
        }
    }
    return nullptr;
}

bool HasOnlyMembers(
    const JsonValue& object,
    std::initializer_list<std::string_view> allowed) noexcept {
    if (object.kind != JsonKind::Object) {
        return false;
    }
    return std::all_of(
        object.object.begin(), object.object.end(),
        [allowed](const auto& member) {
            return std::find(allowed.begin(), allowed.end(), member.first)
                != allowed.end();
        });
}

bool StringMember(
    const JsonValue& object, std::string_view name, std::string_view& output) noexcept {
    const JsonValue* value = Member(object, name);
    if (value == nullptr || value->kind != JsonKind::String) {
        return false;
    }
    output = value->string;
    return true;
}

bool BoolMember(
    const JsonValue& object, std::string_view name, bool& output) noexcept {
    const JsonValue* value = Member(object, name);
    if (value == nullptr || value->kind != JsonKind::Boolean) {
        return false;
    }
    output = value->boolean;
    return true;
}

template <typename T>
bool IntegerMember(
    const JsonValue& object,
    std::string_view name,
    T minimum,
    T maximum,
    T& output) noexcept {
    const JsonValue* value = Member(object, name);
    if (value == nullptr || value->kind != JsonKind::Number
        || value->number < static_cast<std::int64_t>(minimum)
        || value->number > static_cast<std::int64_t>(maximum)) {
        return false;
    }
    output = static_cast<T>(value->number);
    return true;
}

JsonValue StringValue(std::string value) {
    JsonValue result{};
    result.kind = JsonKind::String;
    result.string = std::move(value);
    return result;
}

JsonValue NumberValue(std::int64_t value) noexcept {
    JsonValue result{};
    result.kind = JsonKind::Number;
    result.number = value;
    return result;
}

JsonValue BooleanValue(bool value) noexcept {
    JsonValue result{};
    result.kind = JsonKind::Boolean;
    result.boolean = value;
    return result;
}

JsonValue ArrayValue() noexcept {
    JsonValue result{};
    result.kind = JsonKind::Array;
    return result;
}

JsonValue ObjectValue() noexcept {
    JsonValue result{};
    result.kind = JsonKind::Object;
    return result;
}

void Add(JsonValue& object, std::string name, JsonValue value) {
    object.object.emplace_back(std::move(name), std::move(value));
}

bool AppendJsonString(std::string& output, std::string_view value) {
    output.push_back('"');
    constexpr char kHex[] = "0123456789abcdef";
    for (const unsigned char character : value) {
        switch (character) {
        case '"': output.append("\\\""); break;
        case '\\': output.append("\\\\"); break;
        case '\b': output.append("\\b"); break;
        case '\f': output.append("\\f"); break;
        case '\n': output.append("\\n"); break;
        case '\r': output.append("\\r"); break;
        case '\t': output.append("\\t"); break;
        default:
            if (character < 0x20U) {
                output.append("\\u00");
                output.push_back(kHex[character >> 4U]);
                output.push_back(kHex[character & 0x0fU]);
            } else {
                output.push_back(static_cast<char>(character));
            }
        }
    }
    output.push_back('"');
    return true;
}

bool WriteJson(
    const JsonValue& value,
    std::string& output,
    std::size_t depth) {
    const auto indent = [&output](std::size_t count) {
        output.append(count * 2U, ' ');
    };
    switch (value.kind) {
    case JsonKind::Null:
        output.append("null");
        return true;
    case JsonKind::Boolean:
        output.append(value.boolean ? "true" : "false");
        return true;
    case JsonKind::Number:
        output.append(std::to_string(value.number));
        return true;
    case JsonKind::String:
        return AppendJsonString(output, value.string);
    case JsonKind::Array:
        output.push_back('[');
        if (!value.array.empty()) {
            output.push_back('\n');
            for (std::size_t index = 0U; index < value.array.size(); ++index) {
                indent(depth + 1U);
                if (!WriteJson(value.array[index], output, depth + 1U)) {
                    return false;
                }
                output.append(index + 1U == value.array.size() ? "\n" : ",\n");
            }
            indent(depth);
        }
        output.push_back(']');
        return true;
    case JsonKind::Object:
        output.push_back('{');
        if (!value.object.empty()) {
            output.push_back('\n');
            for (std::size_t index = 0U; index < value.object.size(); ++index) {
                indent(depth + 1U);
                AppendJsonString(output, value.object[index].first);
                output.append(": ");
                if (!WriteJson(value.object[index].second, output, depth + 1U)) {
                    return false;
                }
                output.append(index + 1U == value.object.size() ? "\n" : ",\n");
            }
            indent(depth);
        }
        output.push_back('}');
        return true;
    }
    return false;
}

bool ValidUtf8(std::string_view value) noexcept {
    if (value.empty()) {
        return true;
    }
    if (value.size() > static_cast<std::size_t>(std::numeric_limits<int>::max())) {
        return false;
    }
    return MultiByteToWideChar(
               CP_UTF8,
               MB_ERR_INVALID_CHARS,
               value.data(),
               static_cast<int>(value.size()),
               nullptr,
               0)
        > 0;
}

bool WideToUtf8(std::wstring_view value, std::string& output) {
    if (value.empty()) {
        output.clear();
        return true;
    }
    if (value.size() > static_cast<std::size_t>(std::numeric_limits<int>::max())) {
        return false;
    }
    const int length = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        value.data(),
        static_cast<int>(value.size()),
        nullptr,
        0,
        nullptr,
        nullptr);
    if (length <= 0) {
        return false;
    }
    output.resize(static_cast<std::size_t>(length));
    return WideCharToMultiByte(
               CP_UTF8,
               WC_ERR_INVALID_CHARS,
               value.data(),
               static_cast<int>(value.size()),
               output.data(),
               length,
               nullptr,
               nullptr)
        == length;
}

bool Utf8ToWide(std::string_view value, std::wstring& output) {
    if (value.empty()) {
        output.clear();
        return true;
    }
    if (value.size() > static_cast<std::size_t>(std::numeric_limits<int>::max())) {
        return false;
    }
    const int length = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        value.data(),
        static_cast<int>(value.size()),
        nullptr,
        0);
    if (length <= 0) {
        return false;
    }
    output.resize(static_cast<std::size_t>(length));
    return MultiByteToWideChar(
               CP_UTF8,
               MB_ERR_INVALID_CHARS,
               value.data(),
               static_cast<int>(value.size()),
               output.data(),
               length)
        == length;
}

template <typename T, std::size_t Size>
std::string_view EnumName(
    T value, const std::array<std::pair<T, std::string_view>, Size>& names) noexcept {
    for (const auto& item : names) {
        if (item.first == value) {
            return item.second;
        }
    }
    return {};
}

template <typename T, std::size_t Size>
bool ParseEnum(
    std::string_view name,
    const std::array<std::pair<T, std::string_view>, Size>& names,
    T& output) noexcept {
    for (const auto& item : names) {
        if (item.second == name) {
            output = item.first;
            return true;
        }
    }
    return false;
}

constexpr std::array kLanguageNames{
    std::pair{UiLanguagePreference::System, std::string_view{"system"}},
    std::pair{UiLanguagePreference::Japanese, std::string_view{"ja-JP"}},
    std::pair{UiLanguagePreference::English, std::string_view{"en-US"}},
};
constexpr std::array kRasterFormatNames{
    std::pair{RasterFileFormatSetting::Png, std::string_view{"png"}},
    std::pair{RasterFileFormatSetting::Tiff, std::string_view{"tiff"}},
    std::pair{RasterFileFormatSetting::Tga, std::string_view{"tga"}},
    std::pair{RasterFileFormatSetting::Bmp, std::string_view{"bmp"}},
};
constexpr std::array kSequenceSwitchNames{
    std::pair{SequenceCellSwitchPolicy::Prompt, std::string_view{"prompt"}},
    std::pair{
        SequenceCellSwitchPolicy::AutosaveBeforeSwitch,
        std::string_view{"autosave-before-switch"}},
};
constexpr std::array kSequenceEndpointNames{
    std::pair{SequenceEndpointPolicy::Stop, std::string_view{"stop"}},
    std::pair{SequenceEndpointPolicy::Wrap, std::string_view{"wrap"}},
};
constexpr std::array kOutputProfileNames{
    std::pair{
        OutputColorGuardProfileSetting::Bt709ConservativeYcbcr,
        std::string_view{"bt709-conservative-ycbcr"}},
};
constexpr std::array kKeyboardLayoutNames{
    std::pair{ShortcutKeyboardLayout::Automatic, std::string_view{"automatic"}},
    std::pair{ShortcutKeyboardLayout::Jis109, std::string_view{"jis-109"}},
    std::pair{ShortcutKeyboardLayout::UsAnsi104, std::string_view{"us-ansi-104"}},
};
constexpr std::array kShortcutSlotNames{
    std::pair{ShortcutSlot::Primary, std::string_view{"primary"}},
    std::pair{ShortcutSlot::Secondary, std::string_view{"secondary"}},
};
constexpr std::array kShortcutContextNames{
    std::pair{ShortcutContext::Global, std::string_view{"global"}},
    std::pair{ShortcutContext::Canvas, std::string_view{"canvas"}},
    std::pair{ShortcutContext::Timeline, std::string_view{"timeline"}},
    std::pair{ShortcutContext::Pane, std::string_view{"pane"}},
};
constexpr std::array kShortcutActionNames{
    std::pair{ShortcutAction::Execute, std::string_view{"execute"}},
    std::pair{ShortcutAction::Hold, std::string_view{"hold"}},
    std::pair{ShortcutAction::Toggle, std::string_view{"toggle"}},
};
constexpr std::array kShortcutMatchNames{
    std::pair{ShortcutKeyMatch::Logical, std::string_view{"logical"}},
    std::pair{ShortcutKeyMatch::Physical, std::string_view{"physical"}},
};

struct NamedKey final {
    std::uint32_t value;
    std::string_view name;
};

constexpr std::array kLogicalNamedKeys{
    NamedKey{VK_BACK, "Backspace"},
    NamedKey{VK_TAB, "Tab"},
    NamedKey{VK_RETURN, "Enter"},
    NamedKey{VK_PAUSE, "Pause"},
    NamedKey{VK_CAPITAL, "CapsLock"},
    NamedKey{VK_ESCAPE, "Escape"},
    NamedKey{VK_SPACE, "Space"},
    NamedKey{VK_PRIOR, "PageUp"},
    NamedKey{VK_NEXT, "PageDown"},
    NamedKey{VK_END, "End"},
    NamedKey{VK_HOME, "Home"},
    NamedKey{VK_LEFT, "ArrowLeft"},
    NamedKey{VK_UP, "ArrowUp"},
    NamedKey{VK_RIGHT, "ArrowRight"},
    NamedKey{VK_DOWN, "ArrowDown"},
    NamedKey{VK_SNAPSHOT, "PrintScreen"},
    NamedKey{VK_INSERT, "Insert"},
    NamedKey{VK_DELETE, "Delete"},
    NamedKey{VK_LWIN, "WindowsLeft"},
    NamedKey{VK_RWIN, "WindowsRight"},
    NamedKey{VK_APPS, "ContextMenu"},
    NamedKey{VK_NUMPAD0, "Numpad0"},
    NamedKey{VK_NUMPAD1, "Numpad1"},
    NamedKey{VK_NUMPAD2, "Numpad2"},
    NamedKey{VK_NUMPAD3, "Numpad3"},
    NamedKey{VK_NUMPAD4, "Numpad4"},
    NamedKey{VK_NUMPAD5, "Numpad5"},
    NamedKey{VK_NUMPAD6, "Numpad6"},
    NamedKey{VK_NUMPAD7, "Numpad7"},
    NamedKey{VK_NUMPAD8, "Numpad8"},
    NamedKey{VK_NUMPAD9, "Numpad9"},
    NamedKey{VK_MULTIPLY, "NumpadMultiply"},
    NamedKey{VK_ADD, "NumpadAdd"},
    NamedKey{VK_SUBTRACT, "NumpadSubtract"},
    NamedKey{VK_DECIMAL, "NumpadDecimal"},
    NamedKey{VK_DIVIDE, "NumpadDivide"},
    NamedKey{VK_NUMLOCK, "NumLock"},
    NamedKey{VK_SCROLL, "ScrollLock"},
    NamedKey{VK_OEM_1, "Semicolon"},
    NamedKey{VK_OEM_PLUS, "Equal"},
    NamedKey{VK_OEM_COMMA, "Comma"},
    NamedKey{VK_OEM_MINUS, "Minus"},
    NamedKey{VK_OEM_PERIOD, "Period"},
    NamedKey{VK_OEM_2, "Slash"},
    NamedKey{VK_OEM_3, "Backquote"},
    NamedKey{VK_OEM_4, "BracketLeft"},
    NamedKey{VK_OEM_5, "Backslash"},
    NamedKey{VK_OEM_6, "BracketRight"},
    NamedKey{VK_OEM_7, "Quote"},
    NamedKey{VK_OEM_102, "IntlBackslash"},
};

constexpr std::array kPhysicalNamedKeys{
    NamedKey{0x001U, "Escape"},
    NamedKey{0x002U, "Digit1"}, NamedKey{0x003U, "Digit2"},
    NamedKey{0x004U, "Digit3"}, NamedKey{0x005U, "Digit4"},
    NamedKey{0x006U, "Digit5"}, NamedKey{0x007U, "Digit6"},
    NamedKey{0x008U, "Digit7"}, NamedKey{0x009U, "Digit8"},
    NamedKey{0x00aU, "Digit9"}, NamedKey{0x00bU, "Digit0"},
    NamedKey{0x00cU, "Minus"}, NamedKey{0x00dU, "Equal"},
    NamedKey{0x00eU, "Backspace"}, NamedKey{0x00fU, "Tab"},
    NamedKey{0x010U, "KeyQ"}, NamedKey{0x011U, "KeyW"},
    NamedKey{0x012U, "KeyE"}, NamedKey{0x013U, "KeyR"},
    NamedKey{0x014U, "KeyT"}, NamedKey{0x015U, "KeyY"},
    NamedKey{0x016U, "KeyU"}, NamedKey{0x017U, "KeyI"},
    NamedKey{0x018U, "KeyO"}, NamedKey{0x019U, "KeyP"},
    NamedKey{0x01aU, "BracketLeft"}, NamedKey{0x01bU, "BracketRight"},
    NamedKey{0x01cU, "Enter"}, NamedKey{0x01dU, "ControlLeft"},
    NamedKey{0x01eU, "KeyA"}, NamedKey{0x01fU, "KeyS"},
    NamedKey{0x020U, "KeyD"}, NamedKey{0x021U, "KeyF"},
    NamedKey{0x022U, "KeyG"}, NamedKey{0x023U, "KeyH"},
    NamedKey{0x024U, "KeyJ"}, NamedKey{0x025U, "KeyK"},
    NamedKey{0x026U, "KeyL"}, NamedKey{0x027U, "Semicolon"},
    NamedKey{0x028U, "Quote"}, NamedKey{0x029U, "Backquote"},
    NamedKey{0x02aU, "ShiftLeft"}, NamedKey{0x02bU, "Backslash"},
    NamedKey{0x02cU, "KeyZ"}, NamedKey{0x02dU, "KeyX"},
    NamedKey{0x02eU, "KeyC"}, NamedKey{0x02fU, "KeyV"},
    NamedKey{0x030U, "KeyB"}, NamedKey{0x031U, "KeyN"},
    NamedKey{0x032U, "KeyM"}, NamedKey{0x033U, "Comma"},
    NamedKey{0x034U, "Period"}, NamedKey{0x035U, "Slash"},
    NamedKey{0x036U, "ShiftRight"}, NamedKey{0x037U, "NumpadMultiply"},
    NamedKey{0x038U, "AltLeft"}, NamedKey{0x039U, "Space"},
    NamedKey{0x03aU, "CapsLock"},
    NamedKey{0x03bU, "F1"}, NamedKey{0x03cU, "F2"},
    NamedKey{0x03dU, "F3"}, NamedKey{0x03eU, "F4"},
    NamedKey{0x03fU, "F5"}, NamedKey{0x040U, "F6"},
    NamedKey{0x041U, "F7"}, NamedKey{0x042U, "F8"},
    NamedKey{0x043U, "F9"}, NamedKey{0x044U, "F10"},
    NamedKey{0x045U, "NumLock"}, NamedKey{0x046U, "ScrollLock"},
    NamedKey{0x047U, "Numpad7"}, NamedKey{0x048U, "Numpad8"},
    NamedKey{0x049U, "Numpad9"}, NamedKey{0x04aU, "NumpadSubtract"},
    NamedKey{0x04bU, "Numpad4"}, NamedKey{0x04cU, "Numpad5"},
    NamedKey{0x04dU, "Numpad6"}, NamedKey{0x04eU, "NumpadAdd"},
    NamedKey{0x04fU, "Numpad1"}, NamedKey{0x050U, "Numpad2"},
    NamedKey{0x051U, "Numpad3"}, NamedKey{0x052U, "Numpad0"},
    NamedKey{0x053U, "NumpadDecimal"},
    NamedKey{0x056U, "IntlBackslash"},
    NamedKey{0x057U, "F11"}, NamedKey{0x058U, "F12"},
    NamedKey{0x11cU, "NumpadEnter"}, NamedKey{0x11dU, "ControlRight"},
    NamedKey{0x135U, "NumpadDivide"}, NamedKey{0x137U, "PrintScreen"},
    NamedKey{0x138U, "AltRight"}, NamedKey{0x147U, "Home"},
    NamedKey{0x148U, "ArrowUp"}, NamedKey{0x149U, "PageUp"},
    NamedKey{0x14bU, "ArrowLeft"}, NamedKey{0x14dU, "ArrowRight"},
    NamedKey{0x14fU, "End"}, NamedKey{0x150U, "ArrowDown"},
    NamedKey{0x151U, "PageDown"}, NamedKey{0x152U, "Insert"},
    NamedKey{0x153U, "Delete"}, NamedKey{0x15bU, "WindowsLeft"},
    NamedKey{0x15cU, "WindowsRight"}, NamedKey{0x15dU, "ContextMenu"},
};

std::uint32_t NormalizePhysicalKey(std::uint32_t value) noexcept {
    if ((value & 0xff00U) == 0xe000U) {
        return UINT32_C(0x100) | (value & 0xffU);
    }
    return value;
}

bool LogicalKeyName(std::uint32_t value, std::string& output) {
    if ((value >= 'A' && value <= 'Z') || (value >= '0' && value <= '9')) {
        output.assign(1U, static_cast<char>(value));
        return true;
    }
    if (value >= VK_F1 && value <= VK_F24) {
        output = "F" + std::to_string(value - VK_F1 + 1U);
        return true;
    }
    for (const NamedKey& key : kLogicalNamedKeys) {
        if (key.value == value) {
            output.assign(key.name);
            return true;
        }
    }
    return false;
}

bool ParseLogicalKey(std::string_view name, std::uint32_t& output) noexcept {
    if (name.size() == 1U
        && ((name[0] >= 'A' && name[0] <= 'Z')
            || (name[0] >= '0' && name[0] <= '9'))) {
        output = static_cast<std::uint32_t>(name[0]);
        return true;
    }
    if (name.size() >= 2U && name.size() <= 3U && name[0] == 'F') {
        unsigned int ordinal{};
        const auto parsed = std::from_chars(
            name.data() + 1U, name.data() + name.size(), ordinal);
        if (parsed.ec == std::errc{} && parsed.ptr == name.data() + name.size()
            && ordinal >= 1U && ordinal <= 24U) {
            output = VK_F1 + ordinal - 1U;
            return true;
        }
    }
    for (const NamedKey& key : kLogicalNamedKeys) {
        if (key.name == name) {
            output = key.value;
            return true;
        }
    }
    return false;
}

bool PhysicalKeyName(std::uint32_t value, std::string& output) {
    value = NormalizePhysicalKey(value);
    for (const NamedKey& key : kPhysicalNamedKeys) {
        if (key.value == value) {
            output.assign(key.name);
            return true;
        }
    }
    return false;
}

bool ParsePhysicalKey(std::string_view name, std::uint32_t& output) noexcept {
    for (const NamedKey& key : kPhysicalNamedKeys) {
        if (key.name == name) {
            output = key.value;
            return true;
        }
    }
    return false;
}

bool EncodeModifiers(std::uint32_t modifiers, JsonValue& output) {
    constexpr std::array<std::pair<std::uint32_t, std::string_view>, 4U> names{{
        {INKPOD_SHORTCUT_MODIFIER_CONTROL, "ctrl"},
        {INKPOD_SHORTCUT_MODIFIER_SHIFT, "shift"},
        {INKPOD_SHORTCUT_MODIFIER_ALT, "alt"},
        {windows::ui::kShortcutModifierWindows, "windows"},
    }};
    if ((modifiers & ~windows::ui::kShortcutProfileModifierMask) != 0U) {
        return false;
    }
    output = ArrayValue();
    for (const auto& item : names) {
        if ((modifiers & item.first) != 0U) {
            output.array.push_back(StringValue(std::string(item.second)));
        }
    }
    return true;
}

bool DecodeModifiers(const JsonValue& input, std::uint32_t& output) noexcept {
    if (input.kind != JsonKind::Array || input.array.size() > 4U) {
        return false;
    }
    output = 0U;
    for (const JsonValue& item : input.array) {
        if (item.kind != JsonKind::String) {
            return false;
        }
        std::uint32_t bit{};
        if (item.string == "ctrl") {
            bit = INKPOD_SHORTCUT_MODIFIER_CONTROL;
        } else if (item.string == "shift") {
            bit = INKPOD_SHORTCUT_MODIFIER_SHIFT;
        } else if (item.string == "alt") {
            bit = INKPOD_SHORTCUT_MODIFIER_ALT;
        } else if (item.string == "windows") {
            bit = windows::ui::kShortcutModifierWindows;
        } else {
            return false;
        }
        if ((output & bit) != 0U) {
            return false;
        }
        output |= bit;
    }
    return true;
}

bool EncodeShortcutStroke(const ShortcutInputStroke& stroke, JsonValue& output) {
    std::string logical;
    std::string physical;
    JsonValue modifiers{};
    if (!LogicalKeyName(stroke.logical_key, logical)
        || !PhysicalKeyName(stroke.physical_key, physical)
        || !EncodeModifiers(stroke.modifiers, modifiers)) {
        return false;
    }
    output = ObjectValue();
    Add(output, "logicalKey", StringValue(std::move(logical)));
    Add(output, "physicalKey", StringValue(std::move(physical)));
    Add(output, "modifiers", std::move(modifiers));
    return true;
}

bool DecodeShortcutStroke(const JsonValue& input, ShortcutInputStroke& output) {
    if (!HasOnlyMembers(input, {"logicalKey", "physicalKey", "modifiers"})) {
        return false;
    }
    std::string_view logical;
    std::string_view physical;
    const JsonValue* modifiers = Member(input, "modifiers");
    if (!StringMember(input, "logicalKey", logical)
        || !StringMember(input, "physicalKey", physical)
        || modifiers == nullptr
        || !ParseLogicalKey(logical, output.logical_key)
        || !ParsePhysicalKey(physical, output.physical_key)
        || !DecodeModifiers(*modifiers, output.modifiers)) {
        return false;
    }
    if ((output.physical_key & UINT32_C(0x100)) != 0U) {
        output.modifiers |= INKPOD_SHORTCUT_MODIFIER_EXTENDED;
    }
    return true;
}

bool EncodeShortcutBinding(
    const ShortcutProfileBinding& binding, JsonValue& output) {
    const std::string command = windows::ui::CommandStableKey(binding.command_id);
    const std::string_view slot = EnumName(binding.slot, kShortcutSlotNames);
    const std::string_view context = EnumName(binding.context, kShortcutContextNames);
    const std::string_view action = EnumName(binding.action, kShortcutActionNames);
    const std::string_view match = EnumName(binding.key_match, kShortcutMatchNames);
    if (command.empty() || slot.empty() || context.empty() || action.empty()
        || match.empty() || binding.stroke_count == 0U
        || binding.stroke_count > INKPOD_SHORTCUT_MAX_STROKES) {
        return false;
    }
    output = ObjectValue();
    Add(output, "command", StringValue(command));
    Add(output, "slot", StringValue(std::string(slot)));
    Add(output, "context", StringValue(std::string(context)));
    Add(output, "action", StringValue(std::string(action)));
    Add(output, "match", StringValue(std::string(match)));
    JsonValue strokes = ArrayValue();
    for (std::uint32_t index = 0U; index < binding.stroke_count; ++index) {
        JsonValue stroke{};
        if (!EncodeShortcutStroke(binding.strokes[index], stroke)) {
            return false;
        }
        strokes.array.push_back(std::move(stroke));
    }
    Add(output, "strokes", std::move(strokes));
    return true;
}

bool DecodeShortcutBinding(
    const JsonValue& input, ShortcutProfileBinding& output) {
    if (!HasOnlyMembers(
            input, {"command", "slot", "context", "action", "match", "strokes"})) {
        return false;
    }
    std::string_view command;
    std::string_view slot;
    std::string_view context;
    std::string_view action;
    std::string_view match;
    const JsonValue* strokes = Member(input, "strokes");
    if (!StringMember(input, "command", command)
        || !StringMember(input, "slot", slot)
        || !StringMember(input, "context", context)
        || !StringMember(input, "action", action)
        || !StringMember(input, "match", match) || strokes == nullptr
        || strokes->kind != JsonKind::Array || strokes->array.empty()
        || strokes->array.size() > INKPOD_SHORTCUT_MAX_STROKES) {
        return false;
    }
    output.command_id = windows::ui::CommandFromStableKey(command);
    if (output.command_id == 0U
        || !ParseEnum(slot, kShortcutSlotNames, output.slot)
        || !ParseEnum(context, kShortcutContextNames, output.context)
        || !ParseEnum(action, kShortcutActionNames, output.action)
        || !ParseEnum(match, kShortcutMatchNames, output.key_match)) {
        return false;
    }
    output.stroke_count = static_cast<std::uint32_t>(strokes->array.size());
    for (std::size_t index = 0U; index < strokes->array.size(); ++index) {
        if (!DecodeShortcutStroke(strokes->array[index], output.strokes[index])) {
            return false;
        }
    }
    return true;
}

bool ValidProfileSet(const ShortcutProfileSet& set) noexcept {
    if (set.profiles.empty() || set.profiles.size() > windows::ui::kMaximumShortcutProfiles
        || set.active_profile >= set.profiles.size()
        || !set.profiles.front().built_in) {
        return false;
    }
    for (std::size_t index = 0U; index < set.profiles.size(); ++index) {
        if ((index == 0U) != set.profiles[index].built_in
            || windows::ui::ValidateShortcutProfile(set.profiles[index], false)
                != windows::ui::ShortcutProfileValidation::Ok) {
            return false;
        }
    }
    return !EnumName(set.keyboard_layout, kKeyboardLayoutNames).empty();
}

bool EncodeShortcutSettings(const ShortcutProfileSet& settings, JsonValue& output) {
    if (!ValidProfileSet(settings)) {
        return false;
    }
    output = ObjectValue();
    Add(
        output,
        "keyboardLayout",
        StringValue(std::string(EnumName(settings.keyboard_layout, kKeyboardLayoutNames))));
    const std::string active = settings.active_profile == 0U
        ? "built-in-default"
        : "user-" + std::to_string(settings.active_profile);
    Add(output, "activeProfile", StringValue(active));
    JsonValue profiles = ArrayValue();
    for (std::size_t index = 1U; index < settings.profiles.size(); ++index) {
        const ShortcutProfile& profile = settings.profiles[index];
        std::string name;
        if (!WideToUtf8(profile.name, name)) {
            return false;
        }
        JsonValue item = ObjectValue();
        Add(item, "key", StringValue("user-" + std::to_string(index)));
        Add(item, "name", StringValue(std::move(name)));
        JsonValue bindings = ArrayValue();
        for (const ShortcutProfileBinding& binding : profile.bindings) {
            JsonValue encoded{};
            if (!EncodeShortcutBinding(binding, encoded)) {
                return false;
            }
            bindings.array.push_back(std::move(encoded));
        }
        Add(item, "bindings", std::move(bindings));
        profiles.array.push_back(std::move(item));
    }
    Add(output, "customProfiles", std::move(profiles));
    return true;
}

bool DecodeShortcutSettings(
    const JsonValue& input,
    const ShortcutProfileSet& defaults,
    ShortcutProfileSet& output) {
    if (!ValidProfileSet(defaults)
        || !HasOnlyMembers(
            input, {"keyboardLayout", "activeProfile", "customProfiles"})) {
        return false;
    }
    std::string_view layout;
    std::string_view active;
    const JsonValue* profiles = Member(input, "customProfiles");
    if (!StringMember(input, "keyboardLayout", layout)
        || !StringMember(input, "activeProfile", active) || profiles == nullptr
        || profiles->kind != JsonKind::Array
        || profiles->array.size() + defaults.profiles.size()
            > windows::ui::kMaximumShortcutProfiles) {
        return false;
    }
    output = defaults;
    if (!ParseEnum(layout, kKeyboardLayoutNames, output.keyboard_layout)) {
        return false;
    }
    for (std::size_t index = 0U; index < profiles->array.size(); ++index) {
        const JsonValue& item = profiles->array[index];
        if (!HasOnlyMembers(item, {"key", "name", "bindings"})) {
            return false;
        }
        std::string_view key;
        std::string_view name;
        const JsonValue* bindings = Member(item, "bindings");
        const std::string expected_key = "user-" + std::to_string(index + 1U);
        if (!StringMember(item, "key", key) || key != expected_key
            || !StringMember(item, "name", name) || bindings == nullptr
            || bindings->kind != JsonKind::Array
            || bindings->array.size() > windows::ui::kMaximumShortcutProfileBindings) {
            return false;
        }
        ShortcutProfile profile{};
        if (!Utf8ToWide(name, profile.name) || profile.name.empty()
            || profile.name.size() > windows::ui::kMaximumShortcutProfileNameLength) {
            return false;
        }
        profile.bindings.reserve(bindings->array.size());
        for (const JsonValue& binding_value : bindings->array) {
            ShortcutProfileBinding binding{};
            if (!DecodeShortcutBinding(binding_value, binding)) {
                return false;
            }
            profile.bindings.push_back(binding);
        }
        if (windows::ui::ValidateShortcutProfile(profile, false)
            != windows::ui::ShortcutProfileValidation::Ok) {
            return false;
        }
        output.profiles.push_back(std::move(profile));
    }
    if (active == "built-in-default") {
        output.active_profile = 0U;
    } else if (active.starts_with("user-")) {
        std::size_t ordinal{};
        const std::string_view digits = active.substr(5U);
        const auto parsed = std::from_chars(
            digits.data(), digits.data() + digits.size(), ordinal);
        if (digits.empty() || parsed.ec != std::errc{}
            || parsed.ptr != digits.data() + digits.size() || ordinal == 0U
            || ordinal >= output.profiles.size()) {
            return false;
        }
        output.active_profile = ordinal;
    } else {
        return false;
    }
    return ValidProfileSet(output);
}

constexpr std::array kWorkspacePresetNames{
    std::pair{WorkspacePreset::Coloring, std::string_view{"coloring"}},
    std::pair{WorkspacePreset::LineCleanup, std::string_view{"line-cleanup"}},
    std::pair{
        WorkspacePreset::ReferenceCheck, std::string_view{"reference-check"}},
    std::pair{WorkspacePreset::Batch, std::string_view{"batch"}},
    std::pair{WorkspacePreset::Focus, std::string_view{"focus"}},
    std::pair{WorkspacePreset::Custom, std::string_view{"custom"}},
};
constexpr std::array kWorkspaceDensityNames{
    std::pair{WorkspaceDensity::Standard, std::string_view{"standard"}},
    std::pair{WorkspaceDensity::Compact, std::string_view{"compact"}},
};
constexpr std::array kWorkspaceSplitNames{
    std::pair{WorkspaceSplitOrientation::None, std::string_view{"none"}},
    std::pair{WorkspaceSplitOrientation::Vertical, std::string_view{"vertical"}},
    std::pair{
        WorkspaceSplitOrientation::Horizontal, std::string_view{"horizontal"}},
};
constexpr std::array kDockPaneNames{
    std::pair{DockPaneType::Tool, std::string_view{"tool"}},
    std::pair{DockPaneType::ToolOptions, std::string_view{"tool-options"}},
    std::pair{DockPaneType::Color, std::string_view{"color"}},
    std::pair{DockPaneType::Layer, std::string_view{"layer-plane"}},
    std::pair{DockPaneType::Locator, std::string_view{"locator"}},
    std::pair{DockPaneType::Sequence, std::string_view{"sequence"}},
    std::pair{DockPaneType::LightTable, std::string_view{"light-table"}},
    std::pair{DockPaneType::Reference, std::string_view{"reference"}},
    std::pair{DockPaneType::Batch, std::string_view{"batch"}},
    std::pair{DockPaneType::JobProgress, std::string_view{"job-progress"}},
};
constexpr std::array kDockZoneNames{
    std::pair{DockZone::TopContext, std::string_view{"top-context"}},
    std::pair{DockZone::Left, std::string_view{"left"}},
    std::pair{DockZone::Right, std::string_view{"right"}},
    std::pair{DockZone::Bottom, std::string_view{"bottom"}},
    std::pair{DockZone::Floating, std::string_view{"floating"}},
    std::pair{DockZone::Hidden, std::string_view{"hidden"}},
    std::pair{DockZone::AutoHide, std::string_view{"auto-hide"}},
};
constexpr std::array kDockStackNames{
    std::pair{DockStackMode::Split, std::string_view{"split"}},
    std::pair{DockStackMode::Tabs, std::string_view{"tabs"}},
    std::pair{DockStackMode::Mixed, std::string_view{"mixed"}},
};
constexpr std::array kAuxiliaryPaneNames{
    std::pair{WorkspaceAuxiliaryPane::Locator, std::string_view{"locator"}},
    std::pair{WorkspaceAuxiliaryPane::Sequence, std::string_view{"sequence"}},
    std::pair{WorkspaceAuxiliaryPane::LightTable, std::string_view{"light-table"}},
    std::pair{WorkspaceAuxiliaryPane::Reference, std::string_view{"reference"}},
    std::pair{WorkspaceAuxiliaryPane::Batch, std::string_view{"batch"}},
};
constexpr std::array kAutoHideEdgeNames{
    std::pair{WorkspaceAutoHideEdge::Left, std::string_view{"left"}},
    std::pair{WorkspaceAutoHideEdge::Right, std::string_view{"right"}},
    std::pair{WorkspaceAutoHideEdge::Bottom, std::string_view{"bottom"}},
};

bool EncodeFloatingPlacement(
    const windows::ui::DockFloatingPlacement& placement, JsonValue& output) {
    output = ObjectValue();
    Add(output, "xDip", NumberValue(placement.x_dip));
    Add(output, "yDip", NumberValue(placement.y_dip));
    Add(output, "widthDip", NumberValue(placement.width_dip));
    Add(output, "heightDip", NumberValue(placement.height_dip));
    return true;
}

bool DecodeFloatingPlacement(
    const JsonValue& input,
    windows::ui::DockFloatingPlacement& output) noexcept {
    return HasOnlyMembers(input, {"xDip", "yDip", "widthDip", "heightDip"})
        && IntegerMember(
            input, "xDip", std::numeric_limits<int>::min(),
            std::numeric_limits<int>::max(), output.x_dip)
        && IntegerMember(
            input, "yDip", std::numeric_limits<int>::min(),
            std::numeric_limits<int>::max(), output.y_dip)
        && IntegerMember(input, "widthDip", 1, 16'384, output.width_dip)
        && IntegerMember(input, "heightDip", 1, 16'384, output.height_dip);
}

bool EncodeDockLayout(
    const windows::ui::DockLayoutModel& model, JsonValue& output) {
    const DockLayoutRecord record = model.ToRecord();
    output = ObjectValue();
    Add(output, "mirrored", BooleanValue(record.mirrored != 0U));
    JsonValue panes = ArrayValue();
    for (const auto& pane : record.panes) {
        const auto* descriptor = windows::ui::FindPaneDescriptor(pane.type);
        if (descriptor == nullptr || !descriptor->persist_layout) {
            continue;
        }
        const std::string_view pane_name = EnumName(pane.type, kDockPaneNames);
        const std::string_view zone = EnumName(pane.zone, kDockZoneNames);
        const std::string_view restore_zone =
            EnumName(pane.restore_zone, kDockZoneNames);
        if (pane_name.empty() || zone.empty() || restore_zone.empty()) {
            return false;
        }
        JsonValue item = ObjectValue();
        Add(item, "pane", StringValue(std::string(pane_name)));
        Add(item, "zone", StringValue(std::string(zone)));
        Add(item, "restoreZone", StringValue(std::string(restore_zone)));
        Add(item, "order", NumberValue(pane.order));
        Add(item, "stack", NumberValue(pane.stack));
        Add(item, "tabOrder", NumberValue(pane.tab_order));
        Add(item, "splitWeight", NumberValue(pane.split_weight));
        JsonValue floating{};
        EncodeFloatingPlacement(pane.floating, floating);
        Add(item, "floating", std::move(floating));
        Add(item, "present", BooleanValue(pane.present));
        Add(item, "activeTab", BooleanValue(pane.active_tab));
        panes.array.push_back(std::move(item));
    }
    Add(output, "panes", std::move(panes));
    JsonValue zones = ArrayValue();
    constexpr std::array<DockZone, windows::ui::kDockedZoneCount> docked_zones{
        DockZone::TopContext, DockZone::Left, DockZone::Right, DockZone::Bottom};
    for (const DockZone zone : docked_zones) {
        const auto* state = model.Zone(zone);
        if (state == nullptr) {
            return false;
        }
        JsonValue item = ObjectValue();
        Add(item, "zone", StringValue(std::string(EnumName(zone, kDockZoneNames))));
        Add(
            item,
            "mode",
            StringValue(std::string(EnumName(state->mode, kDockStackNames))));
        if (state->active_tab == DockPaneType::Count) {
            Add(item, "activePane", JsonValue{});
        } else {
            const std::string_view active = EnumName(state->active_tab, kDockPaneNames);
            if (active.empty()) {
                return false;
            }
            Add(item, "activePane", StringValue(std::string(active)));
        }
        Add(item, "extentDip", NumberValue(state->extent_dip));
        zones.array.push_back(std::move(item));
    }
    Add(output, "zones", std::move(zones));
    return true;
}

bool DecodeDockLayout(const JsonValue& input, windows::ui::DockLayoutModel& output) {
    if (!HasOnlyMembers(input, {"mirrored", "panes", "zones"})) {
        return false;
    }
    bool mirrored{};
    const JsonValue* panes = Member(input, "panes");
    const JsonValue* zones = Member(input, "zones");
    if (!BoolMember(input, "mirrored", mirrored) || panes == nullptr
        || panes->kind != JsonKind::Array || zones == nullptr
        || zones->kind != JsonKind::Array
        || panes->array.size() > windows::ui::kDockPaneCount
        || zones->array.size() != windows::ui::kDockedZoneCount) {
        return false;
    }
    DockLayoutRecord record = output.ToRecord();
    record.mirrored = mirrored ? 1U : 0U;
    std::array<bool, windows::ui::kDockPaneCount> seen_panes{};
    for (const JsonValue& item : panes->array) {
        if (!HasOnlyMembers(
                item,
                {"pane", "zone", "restoreZone", "order", "stack", "tabOrder",
                 "splitWeight", "floating", "present", "activeTab"})) {
            return false;
        }
        std::string_view pane_name;
        std::string_view zone_name;
        std::string_view restore_name;
        DockPaneType pane_type{};
        DockZone zone{};
        DockZone restore_zone{};
        const JsonValue* floating = Member(item, "floating");
        if (!StringMember(item, "pane", pane_name)
            || !StringMember(item, "zone", zone_name)
            || !StringMember(item, "restoreZone", restore_name)
            || !ParseEnum(pane_name, kDockPaneNames, pane_type)
            || !ParseEnum(zone_name, kDockZoneNames, zone)
            || !ParseEnum(restore_name, kDockZoneNames, restore_zone)
            || floating == nullptr) {
            return false;
        }
        const auto* descriptor = windows::ui::FindPaneDescriptor(pane_type);
        const std::size_t pane_index = static_cast<std::size_t>(pane_type);
        if (descriptor == nullptr || !descriptor->persist_layout
            || pane_index >= record.panes.size() || seen_panes[pane_index]) {
            return false;
        }
        seen_panes[pane_index] = true;
        auto& pane = record.panes[pane_index];
        pane.type = pane_type;
        pane.zone = zone;
        pane.restore_zone = restore_zone;
        if (!IntegerMember(item, "order", std::uint8_t{0}, UINT8_MAX, pane.order)
            || !IntegerMember(item, "stack", std::uint8_t{0}, UINT8_MAX, pane.stack)
            || !IntegerMember(
                item, "tabOrder", std::uint8_t{0}, UINT8_MAX, pane.tab_order)
            || !IntegerMember(
                item, "splitWeight", std::uint32_t{1U}, UINT32_MAX,
                pane.split_weight)
            || !DecodeFloatingPlacement(*floating, pane.floating)
            || !BoolMember(item, "present", pane.present)
            || !BoolMember(item, "activeTab", pane.active_tab)) {
            return false;
        }
    }
    std::array<bool, windows::ui::kDockedZoneCount> seen_zones{};
    for (const JsonValue& item : zones->array) {
        if (!HasOnlyMembers(item, {"zone", "mode", "activePane", "extentDip"})) {
            return false;
        }
        std::string_view zone_name;
        std::string_view mode_name;
        DockZone zone{};
        DockStackMode mode{};
        if (!StringMember(item, "zone", zone_name)
            || !StringMember(item, "mode", mode_name)
            || !ParseEnum(zone_name, kDockZoneNames, zone)
            || !ParseEnum(mode_name, kDockStackNames, mode)
            || !windows::ui::IsDockedZone(zone)) {
            return false;
        }
        const std::size_t zone_index = static_cast<std::size_t>(zone);
        if (zone_index >= record.zones.size() || seen_zones[zone_index]) {
            return false;
        }
        seen_zones[zone_index] = true;
        auto& state = record.zones[zone_index];
        state.mode = mode;
        const JsonValue* active = Member(item, "activePane");
        if (active == nullptr) {
            return false;
        }
        if (active->kind == JsonKind::Null) {
            state.active_tab = DockPaneType::Count;
        } else if (active->kind == JsonKind::String
            && ParseEnum(active->string, kDockPaneNames, state.active_tab)) {
        } else {
            return false;
        }
        if (!IntegerMember(
                item, "extentDip", 0, 16'384, state.extent_dip)) {
            return false;
        }
    }
    return output.LoadRecord(record);
}

std::string ToolTabKey(ToolTabId id) {
    return "right-tab-" + std::to_string(id.Value());
}

bool ParseToolTabKey(std::string_view key, ToolTabId& output) noexcept {
    constexpr std::string_view prefix = "right-tab-";
    if (!key.starts_with(prefix)) {
        return false;
    }
    const std::string_view digits = key.substr(prefix.size());
    std::uint32_t value{};
    const auto parsed = std::from_chars(
        digits.data(), digits.data() + digits.size(), value);
    if (digits.empty() || parsed.ec != std::errc{}
        || parsed.ptr != digits.data() + digits.size() || value == 0U) {
        return false;
    }
    output = ToolTabId(value);
    return true;
}

bool EncodeRightToolTabs(
    const windows::ui::RightToolTabsModel& model, JsonValue& output) {
    output = ObjectValue();
    if (model.Selected()) {
        Add(output, "selected", StringValue(ToolTabKey(model.Selected())));
    } else {
        Add(output, "selected", JsonValue{});
    }
    Add(output, "nextKey", StringValue(ToolTabKey(ToolTabId(model.NextStableId()))));
    JsonValue tabs = ArrayValue();
    for (const ToolTab& tab : model.Tabs()) {
        JsonValue item = ObjectValue();
        Add(item, "key", StringValue(ToolTabKey(tab.id)));
        JsonValue panes = ArrayValue();
        for (std::size_t index = 0U; index < tab.pane_count; ++index) {
            const std::string_view name = EnumName(tab.panes[index], kDockPaneNames);
            if (name.empty()) {
                return false;
            }
            panes.array.push_back(StringValue(std::string(name)));
        }
        Add(item, "panes", std::move(panes));
        tabs.array.push_back(std::move(item));
    }
    Add(output, "tabs", std::move(tabs));
    return true;
}

bool DecodeRightToolTabs(
    const JsonValue& input, windows::ui::RightToolTabsModel& output) {
    if (!HasOnlyMembers(input, {"selected", "nextKey", "tabs"})) {
        return false;
    }
    const JsonValue* selected_value = Member(input, "selected");
    std::string_view next_key;
    const JsonValue* tabs_value = Member(input, "tabs");
    if (selected_value == nullptr || !StringMember(input, "nextKey", next_key)
        || tabs_value == nullptr || tabs_value->kind != JsonKind::Array
        || tabs_value->array.size() > windows::ui::kMaximumToolTabs) {
        return false;
    }
    ToolTabId selected{};
    if (selected_value->kind == JsonKind::String) {
        if (!ParseToolTabKey(selected_value->string, selected)) {
            return false;
        }
    } else if (selected_value->kind != JsonKind::Null) {
        return false;
    }
    ToolTabId next{};
    if (!ParseToolTabKey(next_key, next)) {
        return false;
    }
    std::array<ToolTab, windows::ui::kMaximumToolTabs> tabs{};
    for (std::size_t index = 0U; index < tabs_value->array.size(); ++index) {
        const JsonValue& item = tabs_value->array[index];
        std::string_view key;
        const JsonValue* panes = Member(item, "panes");
        if (!HasOnlyMembers(item, {"key", "panes"})
            || !StringMember(item, "key", key)
            || !ParseToolTabKey(key, tabs[index].id) || panes == nullptr
            || panes->kind != JsonKind::Array || panes->array.empty()
            || panes->array.size() > windows::ui::kDockPaneCount) {
            return false;
        }
        tabs[index].pane_count = panes->array.size();
        for (std::size_t pane_index = 0U; pane_index < panes->array.size(); ++pane_index) {
            if (panes->array[pane_index].kind != JsonKind::String
                || !ParseEnum(
                    panes->array[pane_index].string,
                    kDockPaneNames,
                    tabs[index].panes[pane_index])) {
                return false;
            }
        }
    }
    return output.Load(
        std::span<const ToolTab>(tabs.data(), tabs_value->array.size()),
        selected,
        next.Value());
}

template <typename Placement>
bool EncodeScreenPlacement(const Placement& placement, JsonValue& output) {
    output = ObjectValue();
    Add(output, "valid", BooleanValue(placement.valid));
    Add(output, "xPx", NumberValue(placement.x_px));
    Add(output, "yPx", NumberValue(placement.y_px));
    Add(output, "widthPx", NumberValue(placement.width_px));
    Add(output, "heightPx", NumberValue(placement.height_px));
    Add(output, "captureDpi", NumberValue(placement.capture_dpi));
    return true;
}

template <typename Placement>
bool DecodeScreenPlacement(const JsonValue& input, Placement& output) noexcept {
    return HasOnlyMembers(
               input, {"valid", "xPx", "yPx", "widthPx", "heightPx", "captureDpi"})
        && BoolMember(input, "valid", output.valid)
        && IntegerMember(
            input, "xPx", std::numeric_limits<int>::min(),
            std::numeric_limits<int>::max(), output.x_px)
        && IntegerMember(
            input, "yPx", std::numeric_limits<int>::min(),
            std::numeric_limits<int>::max(), output.y_px)
        && IntegerMember(input, "widthPx", 0, 1'000'000, output.width_px)
        && IntegerMember(input, "heightPx", 0, 1'000'000, output.height_px)
        && IntegerMember(input, "captureDpi", UINT{48U}, UINT{960U}, output.capture_dpi);
}

bool EncodeWorkspace(const PersistedWorkspace& workspace, JsonValue& output) {
    std::array<std::byte, windows::ui::kMaximumWorkspaceLayoutRecordBytes> validation{};
    std::size_t validation_size{};
    if (workspace.slot >= windows::ui::kMaximumPersistedWorkspaceWindows
        || !windows::ui::EncodeWorkspaceLayout(
            workspace.layout, validation, validation_size)) {
        return false;
    }
    const WorkspaceLayoutState& state = workspace.layout;
    const std::string_view preset = EnumName(state.selected_preset, kWorkspacePresetNames);
    const std::string_view density = EnumName(state.density, kWorkspaceDensityNames);
    const std::string_view orientation = EnumName(state.split_orientation, kWorkspaceSplitNames);
    std::string custom_name;
    const auto terminator = std::find(
        state.custom_name.begin(), state.custom_name.end(), L'\0');
    if (preset.empty() || density.empty() || orientation.empty()
        || !WideToUtf8(
            std::wstring_view(
                state.custom_name.data(),
                static_cast<std::size_t>(terminator - state.custom_name.begin())),
            custom_name)) {
        return false;
    }
    output = ObjectValue();
    Add(output, "slot", NumberValue(workspace.slot));
    Add(output, "selectedPreset", StringValue(std::string(preset)));
    Add(output, "density", StringValue(std::string(density)));
    Add(output, "customName", StringValue(std::move(custom_name)));
    Add(output, "layerSplitPermille", NumberValue(state.layer_split_milli));
    JsonValue editor_split = ObjectValue();
    Add(editor_split, "orientation", StringValue(std::string(orientation)));
    Add(editor_split, "ratioPermille", NumberValue(state.split_ratio_milli));
    Add(output, "editorSplit", std::move(editor_split));
    JsonValue window{};
    EncodeScreenPlacement(state.window, window);
    Add(
        window,
        "show",
        StringValue(state.window.show_command == SW_SHOWMAXIMIZED
            ? "maximized" : "normal"));
    Add(output, "window", std::move(window));
    JsonValue dock{};
    if (!EncodeDockLayout(state.dock, dock)) {
        return false;
    }
    Add(output, "dock", std::move(dock));
    JsonValue right_tabs{};
    if (!EncodeRightToolTabs(state.right_tool_tabs, right_tabs)) {
        return false;
    }
    Add(output, "rightTabs", std::move(right_tabs));
    JsonValue auxiliary = ArrayValue();
    for (const auto& pane : state.auxiliary) {
        const std::string_view type = EnumName(pane.type, kAuxiliaryPaneNames);
        const std::string_view edge = EnumName(pane.edge, kAutoHideEdgeNames);
        if (type.empty() || edge.empty()) {
            return false;
        }
        JsonValue item = ObjectValue();
        Add(item, "pane", StringValue(std::string(type)));
        Add(item, "visible", BooleanValue(pane.visible));
        Add(item, "autoHide", BooleanValue(pane.auto_hide));
        Add(item, "edge", StringValue(std::string(edge)));
        JsonValue floating{};
        EncodeScreenPlacement(pane.floating, floating);
        Add(item, "floating", std::move(floating));
        auxiliary.array.push_back(std::move(item));
    }
    Add(output, "auxiliaryPanes", std::move(auxiliary));
    return true;
}

bool DecodeWorkspace(const JsonValue& input, PersistedWorkspace& output) {
    if (!HasOnlyMembers(
            input,
            {"slot", "selectedPreset", "density", "customName", "layerSplitPermille",
             "editorSplit", "window", "dock", "rightTabs", "auxiliaryPanes"})) {
        return false;
    }
    WorkspaceLayoutState candidate{};
    std::string_view preset;
    std::string_view density;
    std::string_view custom_name;
    const JsonValue* editor_split = Member(input, "editorSplit");
    const JsonValue* window = Member(input, "window");
    const JsonValue* dock = Member(input, "dock");
    const JsonValue* right_tabs = Member(input, "rightTabs");
    const JsonValue* auxiliary = Member(input, "auxiliaryPanes");
    if (!IntegerMember(
            input, "slot", std::uint32_t{0U},
            windows::ui::kMaximumPersistedWorkspaceWindows - 1U, output.slot)
        || !StringMember(input, "selectedPreset", preset)
        || !ParseEnum(preset, kWorkspacePresetNames, candidate.selected_preset)
        || !StringMember(input, "density", density)
        || !ParseEnum(density, kWorkspaceDensityNames, candidate.density)
        || !StringMember(input, "customName", custom_name)
        || editor_split == nullptr || window == nullptr || dock == nullptr
        || right_tabs == nullptr || auxiliary == nullptr
        || auxiliary->kind != JsonKind::Array
        || auxiliary->array.size() != candidate.auxiliary.size()) {
        return false;
    }
    std::wstring custom_name_wide;
    if (!Utf8ToWide(custom_name, custom_name_wide)
        || custom_name_wide.size() >= candidate.custom_name.size()
        || std::any_of(custom_name_wide.begin(), custom_name_wide.end(), [](wchar_t value) {
               return value < L' ';
           })) {
        return false;
    }
    std::copy(
        custom_name_wide.begin(), custom_name_wide.end(), candidate.custom_name.begin());
    if (!IntegerMember(
            input, "layerSplitPermille", std::uint32_t{200U},
            std::uint32_t{800U}, candidate.layer_split_milli)
        || !HasOnlyMembers(*editor_split, {"orientation", "ratioPermille"})) {
        return false;
    }
    std::string_view orientation;
    if (!StringMember(*editor_split, "orientation", orientation)
        || !ParseEnum(orientation, kWorkspaceSplitNames, candidate.split_orientation)
        || !IntegerMember(
            *editor_split, "ratioPermille", std::uint32_t{200U},
            std::uint32_t{800U}, candidate.split_ratio_milli)) {
        return false;
    }
    if (!HasOnlyMembers(
            *window,
            {"valid", "xPx", "yPx", "widthPx", "heightPx", "captureDpi", "show"})) {
        return false;
    }
    JsonValue screen = *window;
    screen.object.erase(
        std::remove_if(
            screen.object.begin(), screen.object.end(),
            [](const auto& item) { return item.first == "show"; }),
        screen.object.end());
    std::string_view show;
    if (!DecodeScreenPlacement(screen, candidate.window)
        || !StringMember(*window, "show", show)
        || (show != "normal" && show != "maximized")) {
        return false;
    }
    candidate.window.show_command = show == "maximized"
        ? SW_SHOWMAXIMIZED : SW_SHOWNORMAL;
    if (!DecodeDockLayout(*dock, candidate.dock)
        || !DecodeRightToolTabs(*right_tabs, candidate.right_tool_tabs)) {
        return false;
    }
    std::array<bool, windows::ui::kWorkspaceAuxiliaryPaneCount> seen{};
    for (const JsonValue& item : auxiliary->array) {
        if (!HasOnlyMembers(item, {"pane", "visible", "autoHide", "edge", "floating"})) {
            return false;
        }
        std::string_view pane_name;
        std::string_view edge_name;
        WorkspaceAuxiliaryPane type{};
        WorkspaceAutoHideEdge edge{};
        const JsonValue* floating = Member(item, "floating");
        if (!StringMember(item, "pane", pane_name)
            || !ParseEnum(pane_name, kAuxiliaryPaneNames, type)
            || !StringMember(item, "edge", edge_name)
            || !ParseEnum(edge_name, kAutoHideEdgeNames, edge)
            || floating == nullptr) {
            return false;
        }
        const std::size_t index = static_cast<std::size_t>(type);
        if (index >= candidate.auxiliary.size() || seen[index]) {
            return false;
        }
        seen[index] = true;
        auto& pane = candidate.auxiliary[index];
        pane.type = type;
        pane.edge = edge;
        if (!BoolMember(item, "visible", pane.visible)
            || !BoolMember(item, "autoHide", pane.auto_hide)
            || !DecodeScreenPlacement(*floating, pane.floating)) {
            return false;
        }
    }
    std::array<std::byte, windows::ui::kMaximumWorkspaceLayoutRecordBytes> validation{};
    std::size_t validation_size{};
    if (!windows::ui::EncodeWorkspaceLayout(
            candidate, validation, validation_size)) {
        return false;
    }
    output.layout = std::move(candidate);
    return true;
}

bool BuildSettingsJson(const ApplicationSettings& settings, JsonValue& output) {
    JsonValue shortcuts{};
    if (!EncodeShortcutSettings(settings.shortcuts, shortcuts)
        || settings.workspaces.size()
            > windows::ui::kMaximumPersistedWorkspaceWindows
        || settings.saved_workspaces.size()
            > windows::ui::kMaximumPersistedWorkspaceWindows) {
        return false;
    }
    output = ObjectValue();
    Add(output, "format", StringValue(kSettingsFormat));
    Add(
        output,
        "formatVersion",
        NumberValue(kApplicationSettingsFormatVersion));

    const std::string_view language = EnumName(settings.ui_language, kLanguageNames);
    const std::string_view raster_format =
        EnumName(settings.default_raster_format, kRasterFormatNames);
    const std::string_view switch_policy =
        EnumName(settings.sequence_switch_policy, kSequenceSwitchNames);
    const std::string_view endpoint_policy =
        EnumName(settings.sequence_endpoint_policy, kSequenceEndpointNames);
    const std::string_view output_profile =
        EnumName(settings.output_color_guard_profile, kOutputProfileNames);
    if (language.empty() || raster_format.empty() || switch_policy.empty() || endpoint_policy.empty()
        || output_profile.empty()) {
        return false;
    }

    JsonValue general = ObjectValue();
    Add(general, "uiLanguage", StringValue(std::string(language)));
    Add(output, "general", std::move(general));

    JsonValue save_and_recovery = ObjectValue();
    Add(
        save_and_recovery,
        "restorePreviousDocuments",
        BooleanValue(settings.restore_previous_documents));
    Add(
        save_and_recovery,
        "defaultRasterFormat",
        StringValue(std::string(raster_format)));
    Add(output, "saveAndRecovery", std::move(save_and_recovery));

    JsonValue animation = ObjectValue();
    Add(
        animation,
        "sequenceCellSwitch",
        StringValue(std::string(switch_policy)));
    Add(
        animation,
        "sequenceEndpoint",
        StringValue(std::string(endpoint_policy)));
    Add(output, "animation", std::move(animation));

    JsonValue color_management = ObjectValue();
    Add(
        color_management,
        "outputGuardProfile",
        StringValue(std::string(output_profile)));
    Add(output, "colorManagement", std::move(color_management));
    Add(output, "keyboardShortcuts", std::move(shortcuts));

    JsonValue workspaces = ObjectValue();
    JsonValue windows = ArrayValue();
    std::array<bool, windows::ui::kMaximumPersistedWorkspaceWindows> seen{};
    for (const PersistedWorkspace& workspace : settings.workspaces) {
        if (workspace.slot >= seen.size() || seen[workspace.slot]) {
            return false;
        }
        seen[workspace.slot] = true;
        JsonValue item{};
        if (!EncodeWorkspace(workspace, item)) {
            return false;
        }
        windows.array.push_back(std::move(item));
    }
    for (std::size_t index = 0U; index < settings.workspaces.size(); ++index) {
        if (!seen[index]) {
            return false;
        }
    }
    Add(workspaces, "windows", std::move(windows));
    JsonValue saved = ArrayValue();
    seen.fill(false);
    for (const PersistedWorkspace& workspace : settings.saved_workspaces) {
        if (workspace.slot >= seen.size() || seen[workspace.slot]) {
            return false;
        }
        seen[workspace.slot] = true;
        JsonValue item{};
        if (!EncodeWorkspace(workspace, item)) {
            return false;
        }
        saved.array.push_back(std::move(item));
    }
    Add(workspaces, "savedLayouts", std::move(saved));
    Add(output, "workspaces", std::move(workspaces));
    return true;
}

bool ParseSettingsJson(
    const JsonValue& root,
    const ShortcutProfileSet& defaults,
    ApplicationSettings& output) {
    if (!HasOnlyMembers(
            root,
            {"format", "formatVersion", "general", "saveAndRecovery", "animation",
             "colorManagement", "keyboardShortcuts", "workspaces"})) {
        return false;
    }
    std::string_view format;
    std::uint32_t version{};
    if (!StringMember(root, "format", format) || format != kSettingsFormat
        || !IntegerMember(
            root,
            "formatVersion",
            kApplicationSettingsFormatVersion,
            kApplicationSettingsFormatVersion,
            version)
        || !ValidProfileSet(defaults)) {
        return false;
    }

    ApplicationSettings candidate{};
    candidate.shortcuts = defaults;
    if (const JsonValue* general = Member(root, "general")) {
        std::string_view language;
        if (!HasOnlyMembers(*general, {"uiLanguage"})
            || !StringMember(*general, "uiLanguage", language)
            || !ParseEnum(language, kLanguageNames, candidate.ui_language)) {
            return false;
        }
    }
    if (const JsonValue* recovery = Member(root, "saveAndRecovery")) {
        std::string_view raster_format;
        if (!HasOnlyMembers(*recovery, {"restorePreviousDocuments", "defaultRasterFormat"})
            || !BoolMember(
                *recovery,
                "restorePreviousDocuments",
                candidate.restore_previous_documents)
            || !StringMember(*recovery, "defaultRasterFormat", raster_format)
            || !ParseEnum(raster_format, kRasterFormatNames, candidate.default_raster_format)) {
            return false;
        }
    }
    if (const JsonValue* animation = Member(root, "animation")) {
        std::string_view switch_policy;
        std::string_view endpoint_policy;
        if (!HasOnlyMembers(
                *animation, {"sequenceCellSwitch", "sequenceEndpoint"})
            || !StringMember(
                *animation, "sequenceCellSwitch", switch_policy)
            || !StringMember(
                *animation, "sequenceEndpoint", endpoint_policy)
            || !ParseEnum(
                switch_policy,
                kSequenceSwitchNames,
                candidate.sequence_switch_policy)
            || !ParseEnum(
                endpoint_policy,
                kSequenceEndpointNames,
                candidate.sequence_endpoint_policy)) {
            return false;
        }
    }
    if (const JsonValue* color = Member(root, "colorManagement")) {
        std::string_view profile;
        if (!HasOnlyMembers(*color, {"outputGuardProfile"})
            || !StringMember(*color, "outputGuardProfile", profile)
            || !ParseEnum(
                profile,
                kOutputProfileNames,
                candidate.output_color_guard_profile)) {
            return false;
        }
    }
    if (const JsonValue* shortcuts = Member(root, "keyboardShortcuts")) {
        if (!DecodeShortcutSettings(*shortcuts, defaults, candidate.shortcuts)) {
            return false;
        }
    }
    if (const JsonValue* workspaces = Member(root, "workspaces")) {
        const JsonValue* windows = Member(*workspaces, "windows");
        const JsonValue* saved = Member(*workspaces, "savedLayouts");
        if (!HasOnlyMembers(*workspaces, {"windows", "savedLayouts"})
            || windows == nullptr
            || windows->kind != JsonKind::Array
            || windows->array.size()
                > windows::ui::kMaximumPersistedWorkspaceWindows) {
            return false;
        }
        std::array<bool, windows::ui::kMaximumPersistedWorkspaceWindows> seen{};
        candidate.workspaces.reserve(windows->array.size());
        for (const JsonValue& item : windows->array) {
            PersistedWorkspace workspace{};
            if (!DecodeWorkspace(item, workspace) || seen[workspace.slot]) {
                return false;
            }
            seen[workspace.slot] = true;
            candidate.workspaces.push_back(std::move(workspace));
        }
        for (std::size_t index = 0U; index < windows->array.size(); ++index) {
            if (!seen[index]) {
                return false;
            }
        }
        if (saved != nullptr) {
            if (saved->kind != JsonKind::Array
                || saved->array.size()
                    > windows::ui::kMaximumPersistedWorkspaceWindows) {
                return false;
            }
            seen.fill(false);
            candidate.saved_workspaces.reserve(saved->array.size());
            for (const JsonValue& item : saved->array) {
                PersistedWorkspace workspace{};
                if (!DecodeWorkspace(item, workspace) || seen[workspace.slot]) {
                    return false;
                }
                seen[workspace.slot] = true;
                candidate.saved_workspaces.push_back(std::move(workspace));
            }
        }
    }
    output = std::move(candidate);
    return true;
}

ApplicationSettings DefaultSettings(const ShortcutProfileSet& defaults) {
    ApplicationSettings result{};
    result.shortcuts = defaults;
    return result;
}

ApplicationSettingsLoadResult ReadSettingsBytes(
    const std::wstring& path, std::string& output) noexcept {
    const HANDLE file = CreateFileW(
        path.c_str(),
        GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_SEQUENTIAL_SCAN,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        const DWORD error = GetLastError();
        return error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND
            ? ApplicationSettingsLoadResult::Missing
            : ApplicationSettingsLoadResult::IoError;
    }
    LARGE_INTEGER size{};
    if (GetFileSizeEx(file, &size) == FALSE || size.QuadPart <= 0
        || static_cast<std::uint64_t>(size.QuadPart)
            > kMaximumApplicationSettingsBytes) {
        CloseHandle(file);
        return ApplicationSettingsLoadResult::Invalid;
    }
    try {
        output.resize(static_cast<std::size_t>(size.QuadPart));
    } catch (const std::bad_alloc&) {
        CloseHandle(file);
        return ApplicationSettingsLoadResult::IoError;
    }
    std::size_t cursor{};
    while (cursor < output.size()) {
        const DWORD request = static_cast<DWORD>(std::min<std::size_t>(
            output.size() - cursor, std::numeric_limits<DWORD>::max()));
        DWORD read{};
        if (ReadFile(file, output.data() + cursor, request, &read, nullptr) == FALSE
            || read == 0U) {
            CloseHandle(file);
            return ApplicationSettingsLoadResult::IoError;
        }
        cursor += read;
    }
    if (CloseHandle(file) == FALSE) {
        return ApplicationSettingsLoadResult::IoError;
    }
    return ApplicationSettingsLoadResult::Loaded;
}

bool WriteSettingsBytesAtomic(
    const std::wstring& path, std::string_view bytes) noexcept {
    if (bytes.empty() || bytes.size() > kMaximumApplicationSettingsBytes) {
        return false;
    }
    std::wstring temporary;
    try {
        temporary = path;
        temporary += L".tmp-";
        temporary += std::to_wstring(GetCurrentProcessId());
        temporary += L'-';
        temporary += std::to_wstring(GetTickCount64());
    } catch (const std::bad_alloc&) {
        return false;
    }
    const HANDLE file = CreateFileW(
        temporary.c_str(),
        GENERIC_WRITE,
        0U,
        nullptr,
        CREATE_NEW,
        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_WRITE_THROUGH,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    std::size_t cursor{};
    bool success = true;
    while (cursor < bytes.size()) {
        const DWORD request = static_cast<DWORD>(std::min<std::size_t>(
            bytes.size() - cursor, std::numeric_limits<DWORD>::max()));
        DWORD written{};
        if (WriteFile(file, bytes.data() + cursor, request, &written, nullptr)
                == FALSE
            || written == 0U) {
            success = false;
            break;
        }
        cursor += written;
    }
    if (success && FlushFileBuffers(file) == FALSE) {
        success = false;
    }
    if (CloseHandle(file) == FALSE) {
        success = false;
    }
    if (success
        && MoveFileExW(
               temporary.c_str(),
               path.c_str(),
               MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
            == FALSE) {
        success = false;
    }
    if (!success) {
        DeleteFileW(temporary.c_str());
    }
    return success;
}

}  // namespace

bool EncodeApplicationSettingsJson(
    const ApplicationSettings& settings,
    std::string& output) noexcept {
    try {
        JsonValue root{};
        if (!BuildSettingsJson(settings, root)) {
            return false;
        }
        std::string encoded;
        encoded.reserve(16U * 1024U);
        if (!WriteJson(root, encoded, 0U)
            || encoded.size() + 1U > kMaximumApplicationSettingsBytes) {
            return false;
        }
        encoded.push_back('\n');
        output = std::move(encoded);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool DecodeApplicationSettingsJson(
    std::string_view input,
    const ShortcutProfileSet& defaults,
    ApplicationSettings& output) noexcept {
    if (input.empty() || input.size() > kMaximumApplicationSettingsBytes
        || !ValidUtf8(input)) {
        return false;
    }
    try {
        JsonValue root{};
        JsonParser parser(input);
        ApplicationSettings candidate{};
        if (!parser.Parse(root) || !ParseSettingsJson(root, defaults, candidate)) {
            return false;
        }
        output = std::move(candidate);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

ShortcutPresetJsonResult EncodeShortcutPresetJson(
    const ShortcutProfile& profile,
    std::string& output) noexcept {
    if (profile.built_in
        || windows::ui::ValidateShortcutProfile(profile, false)
            != windows::ui::ShortcutProfileValidation::Ok) {
        return ShortcutPresetJsonResult::Invalid;
    }
    try {
        std::string name;
        if (!WideToUtf8(profile.name, name)) {
            return ShortcutPresetJsonResult::Invalid;
        }
        JsonValue root = ObjectValue();
        Add(root, "format", StringValue("inkpod-shortcuts"));
        Add(root, "formatVersion", NumberValue(2U));
        Add(root, "name", StringValue(std::move(name)));
        JsonValue bindings = ArrayValue();
        for (const ShortcutProfileBinding& binding : profile.bindings) {
            JsonValue encoded{};
            if (!EncodeShortcutBinding(binding, encoded)) {
                return ShortcutPresetJsonResult::Invalid;
            }
            bindings.array.push_back(std::move(encoded));
        }
        Add(root, "bindings", std::move(bindings));
        std::string encoded;
        encoded.reserve(4U * 1024U);
        if (!WriteJson(root, encoded, 0U)) {
            return ShortcutPresetJsonResult::Invalid;
        }
        encoded.push_back('\n');
        if (encoded.size() > 2U * 1024U * 1024U) {
            return ShortcutPresetJsonResult::CapacityExceeded;
        }
        output = std::move(encoded);
        return ShortcutPresetJsonResult::Ok;
    } catch (const std::bad_alloc&) {
        return ShortcutPresetJsonResult::CapacityExceeded;
    }
}

ShortcutPresetJsonResult DecodeShortcutPresetJson(
    std::string_view input,
    ShortcutProfile& output) noexcept {
    if (input.empty() || input.size() > 2U * 1024U * 1024U
        || !ValidUtf8(input)) {
        return input.size() > 2U * 1024U * 1024U
            ? ShortcutPresetJsonResult::CapacityExceeded
            : ShortcutPresetJsonResult::Invalid;
    }
    try {
        JsonValue root{};
        JsonParser parser(input);
        if (!parser.Parse(root)
            || !HasOnlyMembers(root, {"format", "formatVersion", "name", "bindings"})) {
            return ShortcutPresetJsonResult::Invalid;
        }
        std::string_view format;
        const JsonValue* version = Member(root, "formatVersion");
        if (!StringMember(root, "format", format) || version == nullptr
            || version->kind != JsonKind::Number || format != "inkpod-shortcuts"
            || version->number != 2) {
            return ShortcutPresetJsonResult::UnsupportedVersion;
        }
        std::string_view name;
        const JsonValue* bindings = Member(root, "bindings");
        ShortcutProfile decoded{};
        if (!StringMember(root, "name", name)
            || !Utf8ToWide(name, decoded.name) || decoded.name.empty()
            || decoded.name.size() > windows::ui::kMaximumShortcutProfileNameLength
            || bindings == nullptr || bindings->kind != JsonKind::Array
            || bindings->array.size()
                > windows::ui::kMaximumShortcutProfileBindings) {
            return ShortcutPresetJsonResult::Invalid;
        }
        decoded.bindings.reserve(bindings->array.size());
        for (const JsonValue& value : bindings->array) {
            ShortcutProfileBinding binding{};
            if (!DecodeShortcutBinding(value, binding)) {
                return ShortcutPresetJsonResult::Invalid;
            }
            decoded.bindings.push_back(binding);
        }
        if (windows::ui::ValidateShortcutProfile(decoded, false)
            != windows::ui::ShortcutProfileValidation::Ok) {
            return ShortcutPresetJsonResult::Invalid;
        }
        output = std::move(decoded);
        return ShortcutPresetJsonResult::Ok;
    } catch (const std::bad_alloc&) {
        return ShortcutPresetJsonResult::CapacityExceeded;
    }
}

ApplicationSettingsLoadResult LoadApplicationSettingsFile(
    const std::wstring& path,
    const ShortcutProfileSet& defaults,
    ApplicationSettings& output) noexcept {
    try {
        std::string bytes;
        const ApplicationSettingsLoadResult result = ReadSettingsBytes(path, bytes);
        if (result != ApplicationSettingsLoadResult::Loaded) {
            output = DefaultSettings(defaults);
            return result;
        }
        ApplicationSettings candidate{};
        if (!DecodeApplicationSettingsJson(bytes, defaults, candidate)) {
            output = DefaultSettings(defaults);
            return ApplicationSettingsLoadResult::Invalid;
        }
        output = std::move(candidate);
        return ApplicationSettingsLoadResult::Loaded;
    } catch (const std::bad_alloc&) {
        return ApplicationSettingsLoadResult::IoError;
    }
}

bool SaveApplicationSettingsFile(
    const std::wstring& path,
    const ApplicationSettings& settings) noexcept {
    std::string encoded;
    return EncodeApplicationSettingsJson(settings, encoded)
        && WriteSettingsBytesAtomic(path, encoded);
}

bool LoadApplicationUiLanguagePreference(
    UiLanguagePreference& preference) noexcept {
    std::wstring path;
    if (!ResolveApplicationSettingsPath(path)) {
        return false;
    }
    ShortcutProfileSet defaults{};
    try {
        defaults.profiles.push_back(windows::ui::BuildShortcutProfileFromLegacy(
            L"Built-in",
            true,
            windows::ui::BuildDefaultShortcutSequences()));
    } catch (const std::bad_alloc&) {
        return false;
    }
    ApplicationSettings settings{};
    if (LoadApplicationSettingsFile(path, defaults, settings)
        != ApplicationSettingsLoadResult::Loaded) {
        return false;
    }
    preference = settings.ui_language;
    return true;
}

ApplicationSettingsLoadResult ApplicationSettingsStore::Load(
    const ShortcutProfileSet& defaults) noexcept {
    std::wstring path;
    if (!ResolveApplicationSettingsPath(path)) {
        (void)UseDefaults(defaults);
        return ApplicationSettingsLoadResult::IoError;
    }
    const ApplicationSettingsLoadResult result =
        LoadApplicationSettingsFile(path, defaults, values_);
    invalid_file_loaded_ = result == ApplicationSettingsLoadResult::Invalid;
    return result;
}

bool ApplicationSettingsStore::UseDefaults(
    const ShortcutProfileSet& defaults) noexcept {
    try {
        values_ = DefaultSettings(defaults);
        invalid_file_loaded_ = false;
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool ApplicationSettingsStore::ReplaceTransient(
    const ApplicationSettings& settings) noexcept {
    try {
        values_ = settings;
        invalid_file_loaded_ = false;
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool ApplicationSettingsStore::Save(
    const ApplicationSettings& settings) noexcept {
    return SaveImpl(settings, true);
}

bool ApplicationSettingsStore::SaveAutomatic(
    const ApplicationSettings& settings) noexcept {
    return SaveImpl(settings, false);
}

bool ApplicationSettingsStore::SaveImpl(
    const ApplicationSettings& settings, bool replace_invalid) noexcept {
    if (invalid_file_loaded_ && !replace_invalid) {
        return false;
    }
    ApplicationSettings candidate{};
    try {
        candidate = settings;
    } catch (const std::bad_alloc&) {
        return false;
    }
    std::wstring directory;
    std::wstring path;
    if (!EnsureApplicationDataDirectory(
            ApplicationDataDirectory::Settings, directory)
        || !ResolveApplicationSettingsPath(path)
        || !SaveApplicationSettingsFile(path, settings)) {
        return false;
    }
    values_ = std::move(candidate);
    invalid_file_loaded_ = false;
    return true;
}

const WorkspaceLayoutState* ApplicationSettingsStore::Workspace(
    std::uint32_t slot) const noexcept {
    for (const PersistedWorkspace& workspace : values_.workspaces) {
        if (workspace.slot == slot) {
            return &workspace.layout;
        }
    }
    return nullptr;
}

}  // namespace inkpod::app
