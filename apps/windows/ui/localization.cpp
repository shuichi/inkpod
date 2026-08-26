#include "ui/localization.h"

#include <array>
#include <cstddef>
#include <cstdint>
#include <new>
#include <utility>

namespace inkpod::windows::ui {
namespace {

constexpr std::size_t kMaximumPreferredLanguageBytes = 64U * 1024U;

UiLanguage g_language{UiLanguage::English};
UiLanguagePreference g_preference{UiLanguagePreference::System};

struct UiStringEntry final {
    const wchar_t* japanese;
    std::size_t japanese_length;
    const wchar_t* english;
    std::size_t english_length;
};

constexpr UiStringEntry kUiStrings[] = {
#include "ui/localization_catalog.generated.inc"
};

static_assert(
    std::size(kUiStrings) == static_cast<std::size_t>(UiStringId::Count));

bool IsValidPreference(UiLanguagePreference preference) noexcept {
    return preference == UiLanguagePreference::System
        || preference == UiLanguagePreference::Japanese
        || preference == UiLanguagePreference::English;
}

bool StartsWithJapaneseLanguage(std::wstring_view language) noexcept {
    if (language.size() < 2U) {
        return false;
    }
    const wchar_t first = language[0];
    const wchar_t second = language[1];
    if (!((first == L'j' || first == L'J')
          && (second == L'a' || second == L'A'))) {
        return false;
    }
    return language.size() == 2U || language[2] == L'-'
        || language[2] == L'_';
}

bool HasJapaneseCharacters(std::wstring_view text) noexcept {
    for (const wchar_t character : text) {
        const std::uint32_t value = static_cast<std::uint32_t>(character);
        if ((value >= 0x3040U && value <= 0x30ffU)
            || (value >= 0x31f0U && value <= 0x31ffU)
            || (value >= 0x3400U && value <= 0x4dbfU)
            || (value >= 0x4e00U && value <= 0x9fffU)
            || (value >= 0xf900U && value <= 0xfaffU)
            || (value >= 0xff66U && value <= 0xff9fU)) {
            return true;
        }
    }
    return false;
}

}  // namespace

UiLanguage ResolveUiLanguage(
    UiLanguagePreference preference,
    std::span<const std::wstring_view> preferred_ui_languages) noexcept {
    if (preference == UiLanguagePreference::Japanese) {
        return UiLanguage::Japanese;
    }
    if (preference == UiLanguagePreference::English) {
        return UiLanguage::English;
    }
    return !preferred_ui_languages.empty()
            && StartsWithJapaneseLanguage(preferred_ui_languages.front())
        ? UiLanguage::Japanese
        : UiLanguage::English;
}

UiLanguage DetectSystemUiLanguage() noexcept {
    ULONG count{};
    ULONG character_count{};
    if (!GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME, &count, nullptr, &character_count)
        || count == 0U || character_count < 2U
        || character_count > kMaximumPreferredLanguageBytes / sizeof(wchar_t)) {
        return UiLanguage::English;
    }
    try {
        std::vector<wchar_t> buffer(character_count, L'\0');
        if (!GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &count,
                buffer.data(),
                &character_count)
            || buffer[0] == L'\0') {
            return UiLanguage::English;
        }
        const std::array languages{std::wstring_view(buffer.data())};
        return ResolveUiLanguage(UiLanguagePreference::System, languages);
    } catch (const std::bad_alloc&) {
        return UiLanguage::English;
    }
}

bool InitializeUiLocalization(
    HINSTANCE instance,
    UiLanguagePreference preference) noexcept {
    if (instance == nullptr || !IsValidPreference(preference)) {
        return false;
    }
    g_preference = preference;
    g_language = preference == UiLanguagePreference::System
        ? DetectSystemUiLanguage()
        : ResolveUiLanguage(preference, {});
    (void)SetThreadUILanguage(CurrentUiResourceLanguageId());
    return true;
}

void ShutdownUiLocalization() noexcept {}

UiLanguage CurrentUiLanguage() noexcept {
    return g_language;
}

UiLanguagePreference CurrentUiLanguagePreference() noexcept {
    return g_preference;
}

LANGID CurrentUiResourceLanguageId() noexcept {
    return g_language == UiLanguage::Japanese
        ? MAKELANGID(LANG_JAPANESE, SUBLANG_JAPANESE_JAPAN)
        : MAKELANGID(LANG_ENGLISH, SUBLANG_ENGLISH_US);
}

std::size_t UiStringCount() noexcept {
    return std::size(kUiStrings);
}

const wchar_t* UiText(UiStringId id) noexcept {
    return UiText(id, g_language);
}

const wchar_t* UiText(UiStringId id, UiLanguage language) noexcept {
    const std::wstring_view text = UiTextView(id, language);
    return text.data() == nullptr ? L"" : text.data();
}

std::wstring_view UiTextView(UiStringId id) noexcept {
    return UiTextView(id, g_language);
}

std::wstring_view UiTextView(UiStringId id, UiLanguage language) noexcept {
    const std::size_t index = static_cast<std::size_t>(id);
    if (index >= std::size(kUiStrings)) {
        return {};
    }
    const UiStringEntry& entry = kUiStrings[index];
    if (language == UiLanguage::Japanese) {
        return {entry.japanese, entry.japanese_length};
    }
    return {entry.english, entry.english_length};
}

bool UiStringCatalogIsComplete() noexcept {
    for (const UiStringEntry& entry : kUiStrings) {
        if (entry.japanese == nullptr || entry.japanese_length == 0U
            || entry.english == nullptr || entry.english_length == 0U
            || HasJapaneseCharacters(
                {entry.english, entry.english_length})) {
            return false;
        }
    }
    return true;
}

std::wstring UiTextWithUserText(
    UiStringId prefix, std::wstring_view user_text) {
    const std::wstring_view localized_prefix = UiTextView(prefix);
    std::wstring result;
    result.reserve(localized_prefix.size() + user_text.size());
    result.append(localized_prefix);
    result.append(user_text);
    return result;
}

std::wstring UiTextSequence(
    std::initializer_list<UiStringId> fragments) {
    std::size_t total_length{};
    for (const UiStringId fragment : fragments) {
        total_length += UiTextView(fragment).size();
    }
    std::wstring result;
    result.reserve(total_length);
    for (const UiStringId fragment : fragments) {
        result.append(UiTextView(fragment));
    }
    return result;
}

}  // namespace inkpod::windows::ui
