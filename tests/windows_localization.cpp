#include "app/resource.h"
#include "ui/localization.h"
#include "ui/ui_resources.h"

#include <windows.h>

#include <array>
#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

namespace {

using inkpod::windows::ui::CurrentUiResourceLanguageId;
using inkpod::windows::ui::InitializeUiLocalization;
using inkpod::windows::ui::LoadLocalizedMenuW;
using inkpod::windows::ui::LoadLocalizedStringW;
using inkpod::windows::ui::ShutdownUiLocalization;
using inkpod::windows::ui::UiLanguage;
using inkpod::windows::ui::UiLanguagePreference;
using inkpod::windows::ui::UiStringId;
using inkpod::windows::ui::UiText;
using inkpod::windows::ui::UiTextView;

bool SettingRoundTrip() {
    using inkpod::windows::ui::DecodeUiLanguagePreference;
    using inkpod::windows::ui::EncodeUiLanguagePreference;
    for (const UiLanguagePreference expected : {
             UiLanguagePreference::System,
             UiLanguagePreference::Japanese,
             UiLanguagePreference::English}) {
        std::vector<std::uint8_t> bytes;
        if (!EncodeUiLanguagePreference(expected, bytes) || bytes.size() != 16U) {
            return false;
        }
        UiLanguagePreference actual{};
        if (!DecodeUiLanguagePreference(bytes.data(), bytes.size(), actual)
            || actual != expected) {
            return false;
        }
        std::vector<std::uint8_t> corrupt = bytes;
        corrupt[4] = 2U;
        if (DecodeUiLanguagePreference(corrupt.data(), corrupt.size(), actual)) {
            return false;
        }
        corrupt = bytes;
        corrupt[8] = 99U;
        if (DecodeUiLanguagePreference(corrupt.data(), corrupt.size(), actual)
            || DecodeUiLanguagePreference(bytes.data(), bytes.size() - 1U, actual)) {
            return false;
        }
    }
    std::vector<std::uint8_t> unchanged{1U, 2U, 3U};
    return !EncodeUiLanguagePreference(
               static_cast<UiLanguagePreference>(99U), unchanged)
        && unchanged == std::vector<std::uint8_t>({1U, 2U, 3U});
}

bool ResolverContract() {
    using inkpod::windows::ui::ResolveUiLanguage;
    const std::array<std::wstring_view, 1U> japanese{L"ja-JP"};
    const std::array<std::wstring_view, 1U> japanese_short{L"JA"};
    const std::array<std::wstring_view, 1U> english{L"en-US"};
    const std::array<std::wstring_view, 1U> unsupported{L"de-DE"};
    return ResolveUiLanguage(UiLanguagePreference::System, japanese)
            == UiLanguage::Japanese
        && ResolveUiLanguage(UiLanguagePreference::System, japanese_short)
            == UiLanguage::Japanese
        && ResolveUiLanguage(UiLanguagePreference::System, english)
            == UiLanguage::English
        && ResolveUiLanguage(UiLanguagePreference::System, unsupported)
            == UiLanguage::English
        && ResolveUiLanguage(UiLanguagePreference::System, {})
            == UiLanguage::English
        && ResolveUiLanguage(UiLanguagePreference::Japanese, english)
            == UiLanguage::Japanese
        && ResolveUiLanguage(UiLanguagePreference::English, japanese)
            == UiLanguage::English;
}

bool IsJapaneseCodePoint(std::uint32_t value) {
    return (value >= 0x3000U && value <= 0x303fU)
        || (value >= 0x3040U && value <= 0x30ffU)
        || (value >= 0x31f0U && value <= 0x31ffU)
        || (value >= 0x3400U && value <= 0x4dbfU)
        || (value >= 0x4e00U && value <= 0x9fffU)
        || (value >= 0xf900U && value <= 0xfaffU)
        || (value >= 0xff65U && value <= 0xff9fU);
}

bool HasJapanese(std::wstring_view text) {
    for (const wchar_t character : text) {
        if (IsJapaneseCodePoint(static_cast<std::uint16_t>(character))) {
            return true;
        }
    }
    return false;
}

bool ParseFormatSignature(
    std::wstring_view text, std::vector<std::wstring>& signature) {
    signature.clear();
    for (std::size_t index = 0U; index < text.size(); ++index) {
        if (text[index] != L'%') {
            continue;
        }
        ++index;
        if (index >= text.size()) {
            return false;
        }
        if (text[index] == L'%') {
            continue;
        }
        while (index < text.size()
            && std::wstring_view(L"-+ #0'").find(text[index])
                != std::wstring_view::npos) {
            ++index;
        }
        if (index < text.size() && text[index] == L'*') {
            signature.emplace_back(L"int:width");
            ++index;
        } else {
            while (index < text.size() && text[index] >= L'0'
                && text[index] <= L'9') {
                ++index;
            }
        }
        if (index < text.size() && text[index] == L'.') {
            ++index;
            if (index < text.size() && text[index] == L'*') {
                signature.emplace_back(L"int:precision");
                ++index;
            } else {
                while (index < text.size() && text[index] >= L'0'
                    && text[index] <= L'9') {
                    ++index;
                }
            }
        }
        std::wstring length;
        if (index + 2U < text.size() && text.substr(index, 3U) == L"I64") {
            length = L"I64";
            index += 3U;
        } else if (index + 2U < text.size()
            && text.substr(index, 3U) == L"I32") {
            length = L"I32";
            index += 3U;
        } else if (index + 1U < text.size()
            && (text.substr(index, 2U) == L"hh"
                || text.substr(index, 2U) == L"ll")) {
            length.assign(text.substr(index, 2U));
            index += 2U;
        } else if (index < text.size()
            && std::wstring_view(L"hljztLw").find(text[index])
                != std::wstring_view::npos) {
            length.push_back(text[index]);
            ++index;
        }
        if (index >= text.size()
            || std::wstring_view(L"diuoxXfFeEgGaAcspnCSZ")
                    .find(text[index]) == std::wstring_view::npos) {
            return false;
        }
        length.push_back(L':');
        length.push_back(text[index]);
        signature.push_back(std::move(length));
    }
    return true;
}

bool TypedCatalogContract() {
    using inkpod::windows::ui::UiStringCatalogIsComplete;
    using inkpod::windows::ui::UiStringCount;
    if (!UiStringCatalogIsComplete()
        || UiStringCount() != static_cast<std::size_t>(UiStringId::Count)) {
        return false;
    }
    for (std::size_t index = 0U; index < UiStringCount(); ++index) {
        const auto id = static_cast<UiStringId>(index);
        const std::wstring_view japanese = UiTextView(id, UiLanguage::Japanese);
        const std::wstring_view english = UiTextView(id, UiLanguage::English);
        if (japanese.empty() || english.empty() || HasJapanese(english)) {
            return false;
        }
    }
    std::vector<std::wstring> japanese_signature;
    std::vector<std::wstring> english_signature;
    if (!ParseFormatSignature(
            UiTextView(
                UiStringId::RecoveryCandidatePromptFormat,
                UiLanguage::Japanese),
            japanese_signature)
        || !ParseFormatSignature(
            UiTextView(
                UiStringId::RecoveryCandidatePromptFormat,
                UiLanguage::English),
            english_signature)
        || japanese_signature != english_signature) {
        return false;
    }
    const std::wstring_view filter = UiTextView(
        UiStringId::OpenDocumentFileFilter, UiLanguage::English);
    return filter.find(L'\0') != std::wstring_view::npos
        && filter.size() >= 2U && filter[filter.size() - 1U] == L'\0'
        && filter[filter.size() - 2U] == L'\0';
}

bool OpaqueUserTextContract() {
    using inkpod::windows::ui::UiTextWithUserText;
    const std::wstring user_path =
        L"C:\\shots\\\u5f69\u8272\\\u30d5\u30a1\u30a4\u30eb.inkpod";
    const std::wstring result =
        UiTextWithUserText(UiStringId::FollowingPrefix, user_path);
    return result == L"Following: " + user_path
        && result.ends_with(user_path)
        && result.find(L"Coloring") == std::wstring::npos;
}

INT_PTR CALLBACK PassiveDialogProcedure(
    HWND, UINT message, WPARAM, LPARAM) noexcept {
    return message == WM_INITDIALOG ? TRUE : FALSE;
}

bool MenuCaptionEquals(HMENU menu, std::wstring_view expected) {
    const int length = GetMenuStringW(menu, 0U, nullptr, 0, MF_BYPOSITION);
    if (length <= 0 || static_cast<std::size_t>(length) != expected.size()) {
        return false;
    }
    std::wstring actual(static_cast<std::size_t>(length) + 1U, L'\0');
    if (GetMenuStringW(
            menu, 0U, actual.data(), static_cast<int>(actual.size()), MF_BYPOSITION)
        != length) {
        return false;
    }
    actual.resize(static_cast<std::size_t>(length));
    return actual == expected;
}

bool ResourceLanguageContract(UiLanguagePreference preference) {
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    if (!InitializeUiLocalization(instance, preference)) {
        return false;
    }
    const UiLanguage expected_language = preference == UiLanguagePreference::Japanese
        ? UiLanguage::Japanese
        : UiLanguage::English;
    const LANGID expected_langid = preference == UiLanguagePreference::Japanese
        ? MAKELANGID(LANG_JAPANESE, SUBLANG_JAPANESE_JAPAN)
        : MAKELANGID(LANG_ENGLISH, SUBLANG_ENGLISH_US);
    bool passed = CurrentUiResourceLanguageId() == expected_langid
        && FindResourceExW(
               instance, RT_MENU, MAKEINTRESOURCEW(IDR_MAIN_MENU), expected_langid)
            != nullptr
        && FindResourceExW(
               instance, RT_DIALOG, MAKEINTRESOURCEW(IDD_HISTORY), expected_langid)
            != nullptr;

    std::array<wchar_t, 128U> text{};
    passed = passed
        && LoadLocalizedStringW(
               instance, IDS_DOCK_PANE_TOOL, text.data(),
               static_cast<int>(text.size())) > 0
        && std::wstring_view(text.data())
            == UiTextView(UiStringId::Text0242, expected_language);

    HMENU menu = LoadLocalizedMenuW(instance, MAKEINTRESOURCEW(IDR_MAIN_MENU));
    passed = passed && menu != nullptr
        && MenuCaptionEquals(
            menu, UiTextView(UiStringId::Text0280, expected_language));
    if (menu != nullptr) {
        DestroyMenu(menu);
    }

    HWND dialog = inkpod::windows::ui::CreateLocalizedDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_HISTORY),
        nullptr,
        PassiveDialogProcedure,
        0);
    text.fill(L'\0');
    passed = passed && dialog != nullptr
        && GetWindowTextW(dialog, text.data(), static_cast<int>(text.size())) > 0
        && std::wstring_view(text.data())
            == UiTextView(UiStringId::Text0631, expected_language);
    if (dialog != nullptr) {
        DestroyWindow(dialog);
    }
    ShutdownUiLocalization();
    return passed;
}

}  // namespace

int wmain() {
    if (!SettingRoundTrip()) return 1;
    if (!ResolverContract()) return 2;
    if (!TypedCatalogContract()) return 3;
    if (!ResourceLanguageContract(UiLanguagePreference::English)) return 4;
    if (!OpaqueUserTextContract()) return 5;
    if (!ResourceLanguageContract(UiLanguagePreference::Japanese)) return 6;
    return 0;
}
