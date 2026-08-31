#pragma once

#include <windows.h>

#include <array>
#include <cstdint>
#include <span>
#include <string>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui {

inline constexpr std::uint32_t kShortcutModifierWindows = 1U << 4U;
inline constexpr std::uint32_t kShortcutProfileModifierMask =
    INKPOD_SHORTCUT_MODIFIER_CONTROL | INKPOD_SHORTCUT_MODIFIER_SHIFT
    | INKPOD_SHORTCUT_MODIFIER_ALT | INKPOD_SHORTCUT_MODIFIER_EXTENDED
    | kShortcutModifierWindows;
inline constexpr std::size_t kMaximumShortcutProfiles = 32U;
inline constexpr std::size_t kMaximumShortcutProfileBindings = 2U * 1'024U;
inline constexpr std::size_t kMaximumShortcutProfileNameLength = 128U;

enum class ShortcutSlot : std::uint32_t {
    Primary = 1U,
    Secondary = 2U,
};

enum class ShortcutContext : std::uint32_t {
    Global = 1U,
    Canvas = 2U,
    Timeline = 3U,
    Pane = 4U,
};

enum class ShortcutAction : std::uint32_t {
    Execute = 1U,
    Hold = 2U,
    Toggle = 3U,
};

enum class ShortcutKeyMatch : std::uint32_t {
    Logical = 1U,
    Physical = 2U,
};

enum class ShortcutKeyboardLayout : std::uint32_t {
    Automatic = 1U,
    Jis109 = 2U,
    UsAnsi104 = 3U,
};

enum class ShortcutConflictKind : std::uint32_t {
    Exact = 1U,
    Prefix = 2U,
};

struct ShortcutInputStroke final {
    std::uint32_t logical_key{};
    std::uint32_t physical_key{};
    std::uint32_t modifiers{};

    friend bool operator==(
        const ShortcutInputStroke&, const ShortcutInputStroke&) = default;
};

struct ShortcutProfileBinding final {
    std::uint32_t command_id{};
    ShortcutSlot slot{ShortcutSlot::Primary};
    ShortcutContext context{ShortcutContext::Global};
    ShortcutAction action{ShortcutAction::Execute};
    ShortcutKeyMatch key_match{ShortcutKeyMatch::Logical};
    std::uint32_t stroke_count{};
    std::array<ShortcutInputStroke, INKPOD_SHORTCUT_MAX_STROKES> strokes{};

    friend bool operator==(
        const ShortcutProfileBinding&, const ShortcutProfileBinding&) = default;
};

struct ShortcutProfile final {
    std::wstring name;
    bool built_in{};
    std::vector<ShortcutProfileBinding> bindings;

    friend bool operator==(const ShortcutProfile&, const ShortcutProfile&) = default;
};

struct ShortcutProfileSet final {
    std::vector<ShortcutProfile> profiles;
    std::size_t active_profile{};
    ShortcutKeyboardLayout keyboard_layout{ShortcutKeyboardLayout::Automatic};

    friend bool operator==(const ShortcutProfileSet&, const ShortcutProfileSet&) = default;
};

struct ShortcutConflict final {
    ShortcutConflictKind kind{ShortcutConflictKind::Exact};
    std::size_t first_index{};
    std::size_t second_index{};
};

enum class ShortcutProfileValidation : std::uint32_t {
    Ok = 0U,
    InvalidValue,
    DuplicateSlot,
    PrefixConflict,
    ExactConflict,
    CapacityExceeded,
};

struct ShortcutResolution final {
    InkpodShortcutMatch match{INKPOD_SHORTCUT_MATCH_NONE};
    std::uint32_t command_id{};
    ShortcutSlot slot{ShortcutSlot::Primary};
    ShortcutAction action{ShortcutAction::Execute};
};

[[nodiscard]] bool ShortcutContextsOverlap(
    ShortcutContext left, ShortcutContext right) noexcept;

[[nodiscard]] std::vector<ShortcutConflict> AnalyzeShortcutConflicts(
    std::span<const ShortcutProfileBinding> bindings);

[[nodiscard]] ShortcutProfileValidation ValidateShortcutProfile(
    const ShortcutProfile& profile,
    bool allow_exact_conflicts,
    std::vector<ShortcutConflict>* conflicts = nullptr) noexcept;

[[nodiscard]] ShortcutResolution ResolveShortcutProfile(
    std::span<const ShortcutProfileBinding> bindings,
    ShortcutContext active_context,
    std::span<const ShortcutInputStroke> entered) noexcept;

[[nodiscard]] const ShortcutProfileBinding* FindShortcutBinding(
    std::span<const ShortcutProfileBinding> bindings,
    std::uint32_t command_id,
    ShortcutSlot slot) noexcept;

[[nodiscard]] ShortcutProfileBinding* FindShortcutBinding(
    std::span<ShortcutProfileBinding> bindings,
    std::uint32_t command_id,
    ShortcutSlot slot) noexcept;

[[nodiscard]] std::vector<InkpodShortcutSequence> FlattenShortcutProfile(
    const ShortcutProfile& profile);

[[nodiscard]] ShortcutProfile BuildShortcutProfileFromLegacy(
    std::wstring name,
    bool built_in,
    std::span<const InkpodShortcutSequence> sequences);

[[nodiscard]] bool ShortcutStrokeReservedForNativeMenu(
    std::uint32_t command_id, const ShortcutInputStroke& stroke) noexcept;

[[nodiscard]] inline std::uint32_t ShortcutPhysicalKeyFromVirtualKey(
    std::uint32_t virtual_key, std::uint32_t modifiers) noexcept {
    const UINT mapped = MapVirtualKeyW(
        static_cast<UINT>(virtual_key), MAPVK_VK_TO_VSC_EX);
    if (mapped == 0U) {
        return virtual_key;
    }
    std::uint32_t physical_key = mapped & UINT32_C(0xff);
    if ((modifiers & INKPOD_SHORTCUT_MODIFIER_EXTENDED) != 0U
        || (mapped & UINT32_C(0xff00)) != 0U) {
        physical_key |= UINT32_C(0x100);
    }
    return physical_key;
}

[[nodiscard]] std::uint32_t ShortcutPhysicalKeyFromMessage(
    WPARAM virtual_key, LPARAM key_data) noexcept;

}  // namespace inkpod::windows::ui
