#include "app/resource.h"
#include "ui/dialogs/layer_palette_badge_layout.h"
#include "ui/dialogs/layer_palette_status_layout.h"
#include "ui/history_presentation.h"
#include "ui/localization.h"
#include "ui/panes/pane_dialog_layout.h"
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

bool OpaqueUserTextContract(UiLanguagePreference preference) {
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    if (!InitializeUiLocalization(instance, preference)) {
        return false;
    }
    using inkpod::windows::ui::UiTextWithUserText;
    const std::wstring user_path =
        L"C:\\shots\\\u5f69\u8272\\\u30d5\u30a1\u30a4\u30eb.inkpod";
    const std::wstring result =
        UiTextWithUserText(UiStringId::FollowingPrefix, user_path);
    const std::wstring expected =
        std::wstring(UiText(UiStringId::FollowingPrefix)) + user_path;
    const bool passed = result == expected
        && result.ends_with(user_path)
        && result.find(L"Coloring") == std::wstring::npos;
    ShutdownUiLocalization();
    return passed;
}

bool HistoryPresentationContract() {
    using inkpod::windows::ui::HistoryUiStringId;
    const std::array<InkpodHistoryEntryKind, 5U> kinds{
        INKPOD_HISTORY_ENTRY_RASTER,
        INKPOD_HISTORY_ENTRY_PALETTE,
        INKPOD_HISTORY_ENTRY_COLOR_CHART,
        INKPOD_HISTORY_ENTRY_MAIN_LINE_COLOR,
        INKPOD_HISTORY_ENTRY_DOCUMENT};
    std::array<UiStringId, kinds.size()> ids{};
    for (std::size_t index = 0U; index < kinds.size(); ++index) {
        const auto id = HistoryUiStringId(kinds[index]);
        if (!id.has_value()) {
            return false;
        }
        ids[index] = id.value();
        if (UiTextView(ids[index], UiLanguage::Japanese).empty()
            || UiTextView(ids[index], UiLanguage::English).empty()) {
            return false;
        }
        for (std::size_t prior = 0U; prior < index; ++prior) {
            if (ids[prior] == ids[index]) {
                return false;
            }
        }
    }
    return !HistoryUiStringId(0U).has_value()
        && !HistoryUiStringId(UINT32_MAX).has_value();
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

bool LocalizedButtonLayoutContract(UiLanguagePreference preference) {
    using inkpod::windows::ui::panes::PaneButtonIdealWidthAtDpi;
    using inkpod::windows::ui::panes::PaneButtonRowCount;
    using inkpod::windows::ui::panes::PlacePaneButtonRows;
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    if (!InitializeUiLocalization(instance, preference)) {
        return false;
    }
    HWND parent = CreateWindowExW(
        0,
        L"STATIC",
        nullptr,
        WS_POPUP,
        0,
        0,
        1600,
        1200,
        nullptr,
        nullptr,
        instance,
        nullptr);
    const std::array<UiStringId, 13U> labels{
        UiStringId::Text0706,
        UiStringId::Text0903,
        UiStringId::Delete,
        UiStringId::Text0426,
        UiStringId::Text0430,
        UiStringId::Text0922,
        UiStringId::Register,
        UiStringId::Clear,
        UiStringId::Load,
        UiStringId::Save,
        UiStringId::ToolEyedropper,
        UiStringId::PinDocument,
        UiStringId::ReturnToFollowing};
    std::array<int, labels.size()> controls{};
    bool passed = parent != nullptr;
    for (std::size_t index = 0U; passed && index < labels.size(); ++index) {
        controls[index] = 1000 + static_cast<int>(index);
        passed = CreateWindowExW(
                     0,
                     L"BUTTON",
                     UiText(labels[index]),
                     WS_CHILD | BS_PUSHBUTTON,
                     0,
                     0,
                     1,
                     1,
                     parent,
                     reinterpret_cast<HMENU>(
                         static_cast<INT_PTR>(controls[index])),
                     instance,
                     nullptr)
            != nullptr;
    }
    const std::array<UINT, 4U> dpis{96U, 120U, 144U, 192U};
    for (const UINT dpi : dpis) {
        if (!passed) {
            break;
        }
        const HFONT font = CreateFontW(
            -MulDiv(9, static_cast<int>(dpi), 72),
            0,
            0,
            0,
            FW_NORMAL,
            FALSE,
            FALSE,
            FALSE,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH | FF_DONTCARE,
            L"Segoe UI");
        if (font == nullptr) {
            passed = false;
            break;
        }
        for (const int control : controls) {
            SendDlgItemMessageW(
                parent,
                control,
                WM_SETFONT,
                reinterpret_cast<WPARAM>(font),
                FALSE);
        }
        const int available = MulDiv(240 - 12, static_cast<int>(dpi), 96);
        const int gap = MulDiv(4, static_cast<int>(dpi), 96);
        const int row_height = MulDiv(26, static_cast<int>(dpi), 96);
        const std::span<const int> action_controls{controls.data(), 10U};
        const std::size_t rows = PaneButtonRowCount(
            parent, action_controls, available, gap, dpi);
        passed = rows >= 2U && rows <= action_controls.size()
            && PlacePaneButtonRows(
                   parent,
                   action_controls,
                   0,
                   0,
                   available,
                   row_height,
                   gap,
                   dpi) == rows;
        std::array<RECT, 10U> bounds{};
        for (std::size_t index = 0U; passed && index < bounds.size(); ++index) {
            const HWND button = GetDlgItem(parent, controls[index]);
            passed = button != nullptr
                && GetWindowRect(button, &bounds[index]) != FALSE;
            if (!passed) {
                break;
            }
            MapWindowPoints(
                HWND_DESKTOP,
                parent,
                reinterpret_cast<POINT*>(&bounds[index]),
                2U);
            passed = bounds[index].left >= 0
                && bounds[index].right <= available
                && bounds[index].bottom
                    <= static_cast<int>(rows) * row_height
                        + std::max(0, static_cast<int>(rows) - 1) * gap
                && bounds[index].right - bounds[index].left
                    >= PaneButtonIdealWidthAtDpi(
                        parent, controls[index], dpi);
            for (std::size_t prior = 0U; passed && prior < index; ++prior) {
                RECT intersection{};
                passed = IntersectRect(
                    &intersection, &bounds[prior], &bounds[index]) == FALSE;
            }
        }
        for (std::size_t index = 10U; passed && index < controls.size(); ++index) {
            passed = PaneButtonIdealWidthAtDpi(
                         parent, controls[index], dpi)
                <= available;
        }
        DeleteObject(font);
    }
    if (parent != nullptr) {
        DestroyWindow(parent);
    }
    ShutdownUiLocalization();
    return passed;
}

bool LayerPaletteOwnerDrawCompactCellContract(
    UiLanguagePreference preference) {
    using inkpod::windows::ui::kLayerPaletteStatusButtonSizeDip;
    using inkpod::windows::ui::kLayerPaletteStatusGapDip;
    using inkpod::windows::ui::LayoutLayerPaletteStatusCells;
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    if (!InitializeUiLocalization(instance, preference)) {
        return false;
    }
    const std::array<std::wstring_view, 4U> labels{
        UiTextView(UiStringId::Visible),
        UiTextView(UiStringId::Hidden),
        UiTextView(UiStringId::Editable),
        UiTextView(UiStringId::Protected)};
    bool passed = std::all_of(
        labels.begin(), labels.end(), [](std::wstring_view label) {
            return !label.empty();
        });
    const std::array<UINT, 4U> dpis{96U, 120U, 144U, 192U};
    for (const UINT dpi : dpis) {
        if (!passed) {
            break;
        }
        const int button_size = MulDiv(
            kLayerPaletteStatusButtonSizeDip, static_cast<int>(dpi), 96);
        const int gap = MulDiv(
            kLayerPaletteStatusGapDip, static_cast<int>(dpi), 96);
        const RECT content{
            0,
            0,
            button_size * 2 + gap + MulDiv(80, static_cast<int>(dpi), 96),
            MulDiv(52, static_cast<int>(dpi), 96)};
        const auto layout = LayoutLayerPaletteStatusCells(content, dpi);
        RECT intersection{};
        passed = passed
            && layout.visibility.right - layout.visibility.left == button_size
            && layout.visibility.bottom - layout.visibility.top == button_size
            && layout.editability.right - layout.editability.left == button_size
            && layout.editability.bottom - layout.editability.top == button_size
            && layout.editability.left - layout.visibility.right == gap
            && layout.visibility.top == layout.editability.top
            && layout.visibility.top
                == (content.bottom - content.top - button_size) / 2
            && layout.text_right == layout.visibility.left
            && IntersectRect(
                   &intersection,
                   &layout.visibility,
                   &layout.editability) == FALSE;
    }
    ShutdownUiLocalization();
    return passed;
}

bool LayerPalettePlaneBadgeLayoutContract(
    UiLanguagePreference preference) {
    using inkpod::windows::ui::kLayerPalettePlaneBadgeHeightDip;
    using inkpod::windows::ui::kLayerPalettePlaneBadgePaddingDip;
    using inkpod::windows::ui::kLayerPalettePlaneBadgeWidthDip;
    using inkpod::windows::ui::LayerPalettePlaneBadgeLineCount;
    using inkpod::windows::ui::LayerPalettePlaneBadgeTextFits;
    using inkpod::windows::ui::LayoutLayerPalettePlaneBadgeText;
    using inkpod::windows::ui::MeasureLayerPalettePlaneBadgeText;
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    if (!InitializeUiLocalization(instance, preference)) {
        return false;
    }
    HDC device = GetDC(nullptr);
    bool passed = device != nullptr;
    const std::array<UiStringId, 5U> labels{
        UiStringId::PlaneBadgeMainLine,
        UiStringId::PlaneBadgeColoring,
        UiStringId::PlaneBadgeRaster,
        UiStringId::PlaneBadgeSelection,
        UiStringId::PlaneBadgeUnknown};
    const std::array<UINT, 4U> dpis{96U, 120U, 144U, 192U};
    for (const UINT dpi : dpis) {
        if (!passed) {
            break;
        }
        const HFONT font = CreateFontW(
            -MulDiv(9, static_cast<int>(dpi), 72),
            0,
            0,
            0,
            FW_NORMAL,
            FALSE,
            FALSE,
            FALSE,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH | FF_DONTCARE,
            L"Segoe UI");
        if (font == nullptr) {
            passed = false;
            break;
        }
        const int width = MulDiv(
            kLayerPalettePlaneBadgeWidthDip, static_cast<int>(dpi), 96);
        const int height = MulDiv(
            kLayerPalettePlaneBadgeHeightDip, static_cast<int>(dpi), 96);
        const int padding = MulDiv(
            kLayerPalettePlaneBadgePaddingDip, static_cast<int>(dpi), 96);
        const RECT frame{0, 0, width, height};
        for (const UiStringId id : labels) {
            const std::wstring_view label = UiTextView(id);
            const SIZE measured = MeasureLayerPalettePlaneBadgeText(
                device, font, dpi, label);
            const RECT bounds = LayoutLayerPalettePlaneBadgeText(
                device, font, dpi, label, frame);
            passed = passed
                && label.find(L'\u2026') == std::wstring_view::npos
                && LayerPalettePlaneBadgeLineCount(label) <= 2U
                && LayerPalettePlaneBadgeTextFits(device, font, dpi, label)
                && measured.cx <= width - padding * 2
                && measured.cy <= height - padding * 2
                && bounds.left >= frame.left + padding
                && bounds.right <= frame.right - padding
                && bounds.top >= frame.top + padding
                && bounds.bottom <= frame.bottom - padding;
        }
        DeleteObject(font);
    }
    if (preference == UiLanguagePreference::English) {
        passed = passed
            && UiTextView(UiStringId::PlaneBadgeMainLine).find(L'\n')
                != std::wstring_view::npos
            && UiTextView(UiStringId::PlaneBadgeColoring).find(L'\n')
                != std::wstring_view::npos;
    }
    if (device != nullptr) {
        ReleaseDC(nullptr, device);
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
    if (!OpaqueUserTextContract(UiLanguagePreference::English)) return 5;
    if (!ResourceLanguageContract(UiLanguagePreference::Japanese)) return 6;
    if (!HistoryPresentationContract()) return 7;
    if (!LocalizedButtonLayoutContract(UiLanguagePreference::English)) return 8;
    if (!LocalizedButtonLayoutContract(UiLanguagePreference::Japanese)) return 9;
    if (!OpaqueUserTextContract(UiLanguagePreference::Japanese)) return 10;
    if (!LayerPaletteOwnerDrawCompactCellContract(
            UiLanguagePreference::English)) return 11;
    if (!LayerPaletteOwnerDrawCompactCellContract(
            UiLanguagePreference::Japanese)) return 12;
    if (!LayerPalettePlaneBadgeLayoutContract(
            UiLanguagePreference::English)) return 13;
    if (!LayerPalettePlaneBadgeLayoutContract(
            UiLanguagePreference::Japanese)) return 14;
    return 0;
}
