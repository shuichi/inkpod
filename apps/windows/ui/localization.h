#pragma once

#include <windows.h>

#include <cstdint>
#include <initializer_list>
#include <span>
#include <string>
#include <string_view>
#include <vector>

namespace inkpod::windows::ui {

enum class UiLanguagePreference : std::uint32_t {
    System = 1U,
    Japanese = 2U,
    English = 3U,
};

enum class UiLanguage : std::uint32_t {
    Japanese = 1U,
    English = 2U,
};

// Stable, typed identifiers for product-owned presentation text. Callers must
// compose user-owned names and paths separately so localization never rewrites
// document data. Product UI uses only these IDs at presentation boundaries.
enum class UiStringId : std::uint16_t {
#define INKPOD_UI_STRING_ID(identifier) identifier,
#include "ui/localization_catalog_ids.generated.inc"
#undef INKPOD_UI_STRING_ID
    Count,
};

[[nodiscard]] bool EncodeUiLanguagePreference(
    UiLanguagePreference preference,
    std::vector<std::uint8_t>& output) noexcept;
[[nodiscard]] bool DecodeUiLanguagePreference(
    const std::uint8_t* bytes,
    std::size_t length,
    UiLanguagePreference& preference) noexcept;
[[nodiscard]] bool LoadUiLanguagePreference(
    UiLanguagePreference& preference) noexcept;
[[nodiscard]] bool SaveUiLanguagePreference(
    UiLanguagePreference preference) noexcept;

[[nodiscard]] UiLanguage ResolveUiLanguage(
    UiLanguagePreference preference,
    std::span<const std::wstring_view> preferred_ui_languages) noexcept;
[[nodiscard]] UiLanguage DetectSystemUiLanguage() noexcept;

// Must be called on the UI/Input thread before any product HWND is created.
// The resolved language is process-wide for inkpod-owned presentation and is
// deliberately kept out of document state, Core history, and native files.
[[nodiscard]] bool InitializeUiLocalization(
    HINSTANCE instance,
    UiLanguagePreference preference) noexcept;
void ShutdownUiLocalization() noexcept;

[[nodiscard]] UiLanguage CurrentUiLanguage() noexcept;
[[nodiscard]] UiLanguagePreference CurrentUiLanguagePreference() noexcept;
[[nodiscard]] LANGID CurrentUiResourceLanguageId() noexcept;

[[nodiscard]] std::size_t UiStringCount() noexcept;
[[nodiscard]] std::wstring_view UiTextView(UiStringId id) noexcept;
[[nodiscard]] std::wstring_view UiTextView(
    UiStringId id, UiLanguage language) noexcept;
[[nodiscard]] const wchar_t* UiText(UiStringId id) noexcept;
[[nodiscard]] const wchar_t* UiText(UiStringId id, UiLanguage language) noexcept;
[[nodiscard]] bool UiStringCatalogIsComplete() noexcept;
// Composes a localized product-owned prefix with opaque user-owned text. The
// user text is appended verbatim and is never passed through translation.
[[nodiscard]] std::wstring UiTextWithUserText(
    UiStringId prefix, std::wstring_view user_text);
// Joins only catalog-owned fragments. User names and paths must be appended
// after this function returns, so they can never be interpreted as keys.
[[nodiscard]] std::wstring UiTextSequence(
    std::initializer_list<UiStringId> fragments);


}  // namespace inkpod::windows::ui
