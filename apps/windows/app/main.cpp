#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
#include <objbase.h>
#include <shlobj.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <climits>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <cwchar>
#include <functional>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "about_icon.h"
#include "canvas.h"
#include "core_engine.h"
#include "inkpod/core_ffi.h"
#include "resource.h"

int InkpodRunAbiSmoke();

namespace {

constexpr std::uint32_t kInteractionFill = 1001U;
constexpr std::uint32_t kInteractionEyedropper = 1002U;
constexpr std::uint32_t kInteractionM6Gradient = 1101U;
constexpr std::uint32_t kInteractionM6Airbrush = 1102U;
constexpr std::uint32_t kInteractionM6Blur = 1103U;
constexpr std::uint32_t kInteractionM6Stamp = 1104U;
constexpr std::uint32_t kInteractionM6Dust = 1105U;
constexpr std::uint32_t kInteractionM6AlphaGradient = 1106U;
constexpr UINT_PTR kAutosaveTimer = 1U;
constexpr UINT kAutosaveIntervalMilliseconds = 60U * 1000U;
constexpr UINT kM6TaskCompleted = WM_APP + 0x170U;
constexpr UINT_PTR kM6ProgressTimer = 1U;
constexpr UINT_PTR kM6ContinuousSprayTimer = 2U;
constexpr UINT kM6ContinuousSprayIntervalMilliseconds = 50U;

struct M6StopValue {
    std::uint32_t position_milli{};
    std::uint32_t rgba{};
};

struct M6ToolOptions {
    std::array<std::int32_t, 5U> parameters{};
    std::uint32_t shape{INKPOD_SELECTION_TRACE};
    std::uint32_t mode{};
    bool option{};
    bool option2{};
    std::vector<M6StopValue> stops;
};

struct M6FilterJob {
    std::uint32_t kind{INKPOD_FILTER_INVERT};
    std::uint32_t channel{INKPOD_FILTER_CHANNEL_RGB};
    std::uint32_t interpolation{INKPOD_CURVE_BEZIER};
    std::array<std::int32_t, 5U> parameters{};
    std::vector<InkpodCurvePoint> points;
    std::uint64_t plane_id{};
    bool preview{};
};

struct M6AdjustmentUiState {
    std::uint64_t id{};
    bool visible{true};
    M6FilterJob job;
    std::string name;
};

struct AppState {
    HINSTANCE instance{};
    HWND window{};
    HWND canvas{};
    std::unique_ptr<inkpod::app::CoreEngine> engine;
    std::uint32_t tool{INKPOD_TOOL_PENCIL};
    InkpodPlaneKind plane{INKPOD_PLANE_MAIN_LINE};
    std::uint32_t color_rgba{UINT32_C(0xdc281eff)};
    float diameter{8.0F};
    std::wstring current_path;
    std::wstring recovery_path;
    InkpodColorCheckMode color_check_mode{INKPOD_COLOR_CHECK_OFF};
    InkpodClipboard* clipboard{};
    std::uint64_t m3_layer_id{};
    std::uint64_t secondary_view_id{};
    std::uint64_t m6_adjustment_id{};
    bool m6_adjustment_visible{true};
    std::vector<M6AdjustmentUiState> m6_adjustments;
    InkpodM6Task* m6_task{};
    HWND m6_progress{};
    bool m6_preview_prompt{};
    bool alpha_view{};
    bool stamp_source_valid{};
    InkpodStrokeSample stamp_source{};
    M6ToolOptions m6_options{};
    std::vector<InkpodStrokeSample> m6_samples;
    bool m6_airbrush_active{};
    InkpodStrokeSample m6_airbrush_last{};
    bool view_flip_horizontal{};
    bool view_flip_vertical{};
    bool grid_visible{};
    bool smoke_test{};
};

void ResetM3DocumentUiState(AppState& state) noexcept {
    state.m3_layer_id = 0U;
    state.secondary_view_id = 0U;
    state.view_flip_horizontal = false;
    state.view_flip_vertical = false;
    state.grid_visible = false;
    state.m6_adjustment_id = 0U;
    state.m6_adjustment_visible = true;
    state.m6_adjustments.clear();
    state.alpha_view = false;
    state.stamp_source_valid = false;
    state.m6_samples.clear();
    state.m6_airbrush_active = false;
    if (state.window != nullptr) {
        KillTimer(state.window, kM6ContinuousSprayTimer);
    }
}

class ComApartment final {
public:
    HRESULT Initialize() noexcept {
        const HRESULT result = CoInitializeEx(
            nullptr, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        initialized_ = SUCCEEDED(result);
        return result;
    }

    ~ComApartment() {
        if (initialized_) {
            CoUninitialize();
        }
    }

    ComApartment(const ComApartment&) = delete;
    ComApartment& operator=(const ComApartment&) = delete;
    ComApartment() = default;

private:
    bool initialized_{};
};

struct AboutDialogState {
    HINSTANCE instance{};
    HICON display_icon{};
    HFONT name_font{};
    HFONT body_font{};
    bool close_immediately{};
    bool layout_valid{};
};

// Layout measurements are device pixels from the 144-DPI reference image.
constexpr UINT kAboutReferenceDpi = 144U;
constexpr int kAboutWindowWidth = 574;
constexpr int kAboutWindowHeight = 544;
constexpr int kAboutIconSize = 88;
constexpr int kAboutIconTop = 68;
constexpr int kAboutNameTop = 194;
constexpr int kAboutNameHeight = 40;
constexpr int kAboutDescriptionTop = 250;
constexpr int kAboutDescriptionHeight = 60;
constexpr int kAboutVersionTop = 324;
constexpr int kAboutVersionHeight = 22;
constexpr int kAboutCopyrightTop = 362;
constexpr int kAboutCopyrightHeight = 22;
constexpr int kAboutFooterHeight = 89;
constexpr int kAboutButtonWidth = 120;
constexpr int kAboutButtonHeight = 48;
constexpr int kAboutButtonRightMargin = 18;
constexpr int kAboutButtonBottomMargin = 23;

int ScaleAboutReferencePixel(int reference_pixels, UINT dpi) noexcept {
    return MulDiv(
        reference_pixels,
        static_cast<int>(dpi == 0U ? USER_DEFAULT_SCREEN_DPI : dpi),
        static_cast<int>(kAboutReferenceDpi));
}

int ClampCoordinate(int value, int minimum, int maximum) noexcept {
    if (maximum < minimum) {
        return minimum;
    }
    if (value < minimum) {
        return minimum;
    }
    return value > maximum ? maximum : value;
}

POINT CenteredAboutOrigin(HWND dialog, int width, int height) noexcept {
    const HWND owner = GetWindow(dialog, GW_OWNER);
    const HWND monitor_window = owner != nullptr ? owner : dialog;
    const HMONITOR monitor = MonitorFromWindow(
        monitor_window, MONITOR_DEFAULTTONEAREST);
    MONITORINFO monitor_info{};
    monitor_info.cbSize = sizeof(monitor_info);
    RECT work_area{};
    if (monitor == nullptr
        || GetMonitorInfoW(monitor, &monitor_info) == FALSE) {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &work_area, 0);
    } else {
        work_area = monitor_info.rcWork;
    }

    RECT anchor = work_area;
    if (owner != nullptr) {
        RECT owner_bounds{};
        if (GetWindowRect(owner, &owner_bounds) != FALSE) {
            anchor = owner_bounds;
        }
    }
    const int centered_x = anchor.left + (anchor.right - anchor.left - width) / 2;
    const int centered_y = anchor.top + (anchor.bottom - anchor.top - height) / 2;
    return POINT{
        ClampCoordinate(centered_x, work_area.left, work_area.right - width),
        ClampCoordinate(centered_y, work_area.top, work_area.bottom - height)};
}

bool MoveAboutControl(
    HWND dialog, int id, int x, int y, int width, int height) noexcept {
    const HWND control = GetDlgItem(dialog, id);
    return control != nullptr
        && MoveWindow(control, x, y, width, height, TRUE) != FALSE;
}

void ReleaseAboutFonts(AboutDialogState& state) noexcept {
    if (state.name_font != nullptr) {
        DeleteObject(state.name_font);
        state.name_font = nullptr;
    }
    if (state.body_font != nullptr) {
        DeleteObject(state.body_font);
        state.body_font = nullptr;
    }
}

bool UpdateAboutFonts(
    HWND dialog, AboutDialogState& state, UINT dpi) noexcept {
    ReleaseAboutFonts(state);
    const auto dialog_font = reinterpret_cast<HFONT>(
        SendMessageW(dialog, WM_GETFONT, 0, 0));
    LOGFONTW base_font{};
    if (dialog_font == nullptr
        || GetObjectW(
               dialog_font,
               static_cast<int>(sizeof(base_font)),
               &base_font) != static_cast<int>(sizeof(base_font))) {
        return false;
    }

    LOGFONTW name_font = base_font;
    name_font.lfHeight = -MulDiv(15, static_cast<int>(dpi), 72);
    name_font.lfWeight = FW_SEMIBOLD;
    state.name_font = CreateFontIndirectW(&name_font);

    LOGFONTW body_font = base_font;
    body_font.lfHeight = -MulDiv(9, static_cast<int>(dpi), 72);
    body_font.lfWeight = FW_NORMAL;
    state.body_font = CreateFontIndirectW(&body_font);
    if (state.name_font == nullptr || state.body_font == nullptr) {
        ReleaseAboutFonts(state);
        return false;
    }

    SendDlgItemMessageW(
        dialog,
        IDC_ABOUT_NAME,
        WM_SETFONT,
        reinterpret_cast<WPARAM>(state.name_font),
        TRUE);
    for (const int id : {
             IDC_ABOUT_DESCRIPTION,
             IDC_ABOUT_VERSION,
             IDC_ABOUT_COPYRIGHT,
             IDOK}) {
        SendDlgItemMessageW(
            dialog,
            id,
            WM_SETFONT,
            reinterpret_cast<WPARAM>(state.body_font),
            TRUE);
    }
    return true;
}

bool UpdateAboutIcon(
    HWND dialog, AboutDialogState& state, int icon_size) noexcept {
    const HWND icon_control = GetDlgItem(dialog, IDC_ABOUT_ICON);
    if (icon_control == nullptr) {
        return false;
    }
    SendMessageW(icon_control, STM_SETIMAGE, IMAGE_ICON, 0);
    if (state.display_icon != nullptr) {
        DestroyIcon(state.display_icon);
        state.display_icon = nullptr;
    }
    const int resource_id = icon_size <= kAboutIconSize
        ? IDR_ABOUT_ICON_88_PNG
        : IDR_ABOUT_ICON_256_PNG;
    state.display_icon = inkpod::windows::LoadPngIconResource(
        state.instance, resource_id, icon_size);
    if (state.display_icon == nullptr) {
        return false;
    }
    SendMessageW(
        icon_control,
        STM_SETIMAGE,
        IMAGE_ICON,
        reinterpret_cast<LPARAM>(state.display_icon));
    return true;
}

bool LayoutAboutDialog(
    HWND dialog, AboutDialogState& state, bool center_on_owner) noexcept {
    UINT dpi = GetDpiForWindow(dialog);
    if (dpi == 0U) {
        dpi = USER_DEFAULT_SCREEN_DPI;
    }
    const int window_width = ScaleAboutReferencePixel(kAboutWindowWidth, dpi);
    const int window_height = ScaleAboutReferencePixel(kAboutWindowHeight, dpi);
    RECT current_bounds{};
    if (GetWindowRect(dialog, &current_bounds) == FALSE) {
        return false;
    }
    const POINT origin = center_on_owner
        ? CenteredAboutOrigin(dialog, window_width, window_height)
        : POINT{current_bounds.left, current_bounds.top};
    if (SetWindowPos(
            dialog,
            nullptr,
            origin.x,
            origin.y,
            window_width,
            window_height,
            SWP_NOACTIVATE | SWP_NOZORDER) == FALSE) {
        return false;
    }

    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return false;
    }
    const int icon_size = ScaleAboutReferencePixel(kAboutIconSize, dpi);
    const int content_width = client.right - client.left;
    const int button_width = ScaleAboutReferencePixel(kAboutButtonWidth, dpi);
    const int button_height = ScaleAboutReferencePixel(kAboutButtonHeight, dpi);
    const int description_margin = ScaleAboutReferencePixel(48, dpi);
    const int name_margin = ScaleAboutReferencePixel(20, dpi);
    const int divider_height = ScaleAboutReferencePixel(1, dpi);
    if (!MoveAboutControl(
            dialog,
            IDC_ABOUT_ICON,
            (content_width - icon_size) / 2,
            ScaleAboutReferencePixel(kAboutIconTop, dpi),
            icon_size,
            icon_size)
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_NAME,
            name_margin,
            ScaleAboutReferencePixel(kAboutNameTop, dpi),
            content_width - name_margin * 2,
            ScaleAboutReferencePixel(kAboutNameHeight, dpi))
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_DESCRIPTION,
            description_margin,
            ScaleAboutReferencePixel(kAboutDescriptionTop, dpi),
            content_width - description_margin * 2,
            ScaleAboutReferencePixel(kAboutDescriptionHeight, dpi))
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_VERSION,
            name_margin,
            ScaleAboutReferencePixel(kAboutVersionTop, dpi),
            content_width - name_margin * 2,
            ScaleAboutReferencePixel(kAboutVersionHeight, dpi))
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_COPYRIGHT,
            name_margin,
            ScaleAboutReferencePixel(kAboutCopyrightTop, dpi),
            content_width - name_margin * 2,
            ScaleAboutReferencePixel(kAboutCopyrightHeight, dpi))
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_SEPARATOR,
            0,
            client.bottom - ScaleAboutReferencePixel(kAboutFooterHeight, dpi),
            content_width,
            divider_height)
        || !MoveAboutControl(
            dialog,
            IDOK,
            content_width
                - ScaleAboutReferencePixel(kAboutButtonRightMargin, dpi)
                - button_width,
            client.bottom
                - ScaleAboutReferencePixel(kAboutButtonBottomMargin, dpi)
                - button_height,
            button_width,
            button_height)
        || !UpdateAboutFonts(dialog, state, dpi)
        || !UpdateAboutIcon(dialog, state, icon_size)) {
        return false;
    }
    return true;
}

bool AboutControlBounds(HWND dialog, int id, RECT& bounds) noexcept {
    const HWND control = GetDlgItem(dialog, id);
    if (control == nullptr || GetWindowRect(control, &bounds) == FALSE) {
        return false;
    }
    MapWindowPoints(
        nullptr,
        dialog,
        reinterpret_cast<POINT*>(&bounds),
        2);
    return true;
}

bool AboutIconSizeMatches(HICON icon, int expected_size) noexcept {
    ICONINFO icon_info{};
    if (icon == nullptr || GetIconInfo(icon, &icon_info) == FALSE) {
        return false;
    }
    BITMAP bitmap{};
    const bool matches = icon_info.hbmColor != nullptr
        && GetObjectW(
               icon_info.hbmColor,
               static_cast<int>(sizeof(bitmap)),
               &bitmap) == static_cast<int>(sizeof(bitmap))
        && bitmap.bmWidth == expected_size
        && bitmap.bmHeight == expected_size;
    if (icon_info.hbmColor != nullptr) {
        DeleteObject(icon_info.hbmColor);
    }
    if (icon_info.hbmMask != nullptr) {
        DeleteObject(icon_info.hbmMask);
    }
    return matches;
}

bool AboutFontHeightMatches(
    HWND dialog, int control_id, int point_size, UINT dpi) noexcept {
    const auto font = reinterpret_cast<HFONT>(
        SendDlgItemMessageW(dialog, control_id, WM_GETFONT, 0, 0));
    LOGFONTW font_info{};
    return font != nullptr
        && GetObjectW(
               font,
               static_cast<int>(sizeof(font_info)),
               &font_info) == static_cast<int>(sizeof(font_info))
        && font_info.lfHeight
            == -MulDiv(point_size, static_cast<int>(dpi), 72);
}

bool ValidateAboutDialog(HWND dialog, HINSTANCE instance) noexcept {
    UINT dpi = GetDpiForWindow(dialog);
    if (dpi == 0U) {
        dpi = USER_DEFAULT_SCREEN_DPI;
    }
    RECT dialog_bounds{};
    RECT icon_bounds{};
    RECT name_bounds{};
    RECT copyright_bounds{};
    RECT separator_bounds{};
    if (GetWindowRect(dialog, &dialog_bounds) == FALSE
        || !AboutControlBounds(dialog, IDC_ABOUT_ICON, icon_bounds)
        || !AboutControlBounds(dialog, IDC_ABOUT_NAME, name_bounds)
        || !AboutControlBounds(
            dialog, IDC_ABOUT_COPYRIGHT, copyright_bounds)
        || !AboutControlBounds(
            dialog, IDC_ABOUT_SEPARATOR, separator_bounds)) {
        return false;
    }
    const int width = dialog_bounds.right - dialog_bounds.left;
    const int height = dialog_bounds.bottom - dialog_bounds.top;
    const POINT expected_origin = CenteredAboutOrigin(dialog, width, height);
    const int expected_icon_size =
        ScaleAboutReferencePixel(kAboutIconSize, dpi);
    const auto displayed_icon = reinterpret_cast<HICON>(
        SendDlgItemMessageW(
            dialog, IDC_ABOUT_ICON, STM_GETIMAGE, IMAGE_ICON, 0));
    if (width != ScaleAboutReferencePixel(kAboutWindowWidth, dpi)
        || height != ScaleAboutReferencePixel(kAboutWindowHeight, dpi)
        || dialog_bounds.left != expected_origin.x
        || dialog_bounds.top != expected_origin.y
        || name_bounds.top - icon_bounds.bottom
            != ScaleAboutReferencePixel(38, dpi)
        || name_bounds.bottom - name_bounds.top
            != ScaleAboutReferencePixel(kAboutNameHeight, dpi)
        || copyright_bounds.bottom >= separator_bounds.top
        || !AboutIconSizeMatches(displayed_icon, expected_icon_size)
        || !AboutFontHeightMatches(dialog, IDC_ABOUT_NAME, 15, dpi)
        || !AboutFontHeightMatches(
            dialog, IDC_ABOUT_DESCRIPTION, 9, dpi)) {
        return false;
    }

    std::array<wchar_t, 32> name{};
    std::array<wchar_t, 64> version{};
    std::array<wchar_t, 96> expected_version{};
    std::array<wchar_t, 512> description{};
    std::array<wchar_t, 512> expected_description{};
    std::array<wchar_t, 64> copyright{};
    std::array<wchar_t, 64> expected_copyright{};
    if (GetDlgItemTextW(
            dialog, IDC_ABOUT_NAME, name.data(), static_cast<int>(name.size())) == 0
        || GetDlgItemTextW(
               dialog,
               IDC_ABOUT_VERSION,
               expected_version.data(),
               static_cast<int>(expected_version.size())) == 0
        || GetDlgItemTextW(
               dialog,
               IDC_ABOUT_DESCRIPTION,
               description.data(),
               static_cast<int>(description.size())) == 0
        || LoadStringW(
               instance,
               IDS_APP_VERSION,
               version.data(),
               static_cast<int>(version.size())) == 0
        || LoadStringW(
               instance,
               IDS_ABOUT_DESCRIPTION,
               expected_description.data(),
               static_cast<int>(expected_description.size())) == 0
        || GetDlgItemTextW(
               dialog,
               IDC_ABOUT_COPYRIGHT,
               copyright.data(),
               static_cast<int>(copyright.size())) == 0
        || LoadStringW(
               instance,
               IDS_ABOUT_COPYRIGHT,
               expected_copyright.data(),
               static_cast<int>(expected_copyright.size())) == 0) {
        return false;
    }
    std::array<wchar_t, 96> version_label{};
    _snwprintf_s(
        version_label.data(),
        version_label.size(),
        _TRUNCATE,
        L"Version %ls",
        version.data());
    return std::wcscmp(name.data(), L"Inkpod") == 0
        && std::wcscmp(expected_version.data(), version_label.data()) == 0
        && std::wcscmp(description.data(), expected_description.data()) == 0
        && std::wcscmp(copyright.data(), expected_copyright.data()) == 0;
}

INT_PTR CALLBACK AboutDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<AboutDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<AboutDialogState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));

            std::array<wchar_t, 64> version{};
            std::array<wchar_t, 96> version_label{};
            std::array<wchar_t, 512> description{};
            std::array<wchar_t, 64> copyright{};
            if (LoadStringW(
                    state->instance,
                    IDS_APP_VERSION,
                    version.data(),
                    static_cast<int>(version.size())) == 0
                || LoadStringW(
                       state->instance,
                       IDS_ABOUT_DESCRIPTION,
                       description.data(),
                       static_cast<int>(description.size())) == 0
                || LoadStringW(
                       state->instance,
                       IDS_ABOUT_COPYRIGHT,
                       copyright.data(),
                       static_cast<int>(copyright.size())) == 0) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            _snwprintf_s(
                version_label.data(),
                version_label.size(),
                _TRUNCATE,
                L"Version %ls",
                version.data());
            SetDlgItemTextW(dialog, IDC_ABOUT_VERSION, version_label.data());
            SetDlgItemTextW(dialog, IDC_ABOUT_DESCRIPTION, description.data());
            SetDlgItemTextW(dialog, IDC_ABOUT_COPYRIGHT, copyright.data());
            if (!LayoutAboutDialog(dialog, *state, true)) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }

            const auto caption_icon = LoadIconW(
                state->instance, MAKEINTRESOURCEW(IDI_APP_ICON));
            if (caption_icon != nullptr) {
                SendMessageW(
                    dialog,
                    WM_SETICON,
                    ICON_SMALL,
                    reinterpret_cast<LPARAM>(caption_icon));
            }
            state->layout_valid = ValidateAboutDialog(
                dialog, state->instance);
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_DPICHANGED:
            if (state != nullptr) {
                const auto* suggested = reinterpret_cast<const RECT*>(lparam);
                if (suggested != nullptr) {
                    SetWindowPos(
                        dialog,
                        nullptr,
                        suggested->left,
                        suggested->top,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER);
                }
                return LayoutAboutDialog(dialog, *state, false) ? TRUE : FALSE;
            }
            break;
        case WM_COMMAND:
            if (LOWORD(wparam) == IDOK || LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, LOWORD(wparam));
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        case WM_DESTROY:
            if (state != nullptr) {
                SendDlgItemMessageW(
                    dialog, IDC_ABOUT_ICON, STM_SETIMAGE, IMAGE_ICON, 0);
                if (state->display_icon != nullptr) {
                    DestroyIcon(state->display_icon);
                    state->display_icon = nullptr;
                }
                ReleaseAboutFonts(*state);
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR ShowAboutDialog(
    HINSTANCE instance, HWND owner, bool close_immediately) noexcept {
    AboutDialogState state{
        instance, nullptr, nullptr, nullptr, close_immediately, false};
    const INT_PTR result = DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_ABOUT),
        owner,
        AboutDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
    return result == IDOK && (!close_immediately || state.layout_valid)
        ? IDOK
        : IDCANCEL;
}

struct ShortcutDialogState {
    std::uint32_t command_id{1U};
    std::uint32_t virtual_key{static_cast<std::uint32_t>('Z')};
    std::uint32_t modifiers{INKPOD_SHORTCUT_MODIFIER_CONTROL};
    bool close_immediately{};
};

constexpr WORD DefaultShortcutHotkey(std::uint32_t command_id) noexcept {
    const BYTE key = command_id == 2U
        ? static_cast<BYTE>('Y')
        : (command_id == 3U
                  ? static_cast<BYTE>('C')
                  : (command_id == 4U ? static_cast<BYTE>('V')
                                      : static_cast<BYTE>('Z')));
    return MAKEWORD(key, HOTKEYF_CONTROL);
}

std::uint32_t ShortcutModifiers(BYTE hotkey_flags) noexcept {
    std::uint32_t modifiers{};
    if ((hotkey_flags & HOTKEYF_CONTROL) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_CONTROL;
    }
    if ((hotkey_flags & HOTKEYF_SHIFT) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_SHIFT;
    }
    if ((hotkey_flags & HOTKEYF_ALT) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_ALT;
    }
    if ((hotkey_flags & HOTKEYF_EXT) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_EXTENDED;
    }
    return modifiers;
}

INT_PTR CALLBACK ShortcutDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<ShortcutDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<ShortcutDialogState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND commands = GetDlgItem(dialog, IDC_SHORTCUT_COMMAND);
            if (commands == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            constexpr std::array<const wchar_t*, 4> labels{
                L"元に戻す", L"やり直し", L"コピー", L"貼り付け"};
            for (std::size_t index = 0; index < labels.size(); ++index) {
                const LRESULT item = SendMessageW(
                    commands,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(labels[index]));
                if (item == CB_ERR || item == CB_ERRSPACE) {
                    EndDialog(dialog, IDCANCEL);
                    return TRUE;
                }
                SendMessageW(
                    commands,
                    CB_SETITEMDATA,
                    static_cast<WPARAM>(item),
                    static_cast<LPARAM>(index + 1U));
            }
            SendMessageW(commands, CB_SETCURSEL, 0, 0);
            SendDlgItemMessageW(
                dialog,
                IDC_SHORTCUT_HOTKEY,
                HKM_SETHOTKEY,
                DefaultShortcutHotkey(1U),
                0);
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDC_SHORTCUT_COMMAND
                && HIWORD(wparam) == CBN_SELCHANGE) {
                const LRESULT selected = SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_COMMAND, CB_GETCURSEL, 0, 0);
                if (selected != CB_ERR) {
                    const auto command_id = static_cast<std::uint32_t>(
                        SendDlgItemMessageW(
                            dialog,
                            IDC_SHORTCUT_COMMAND,
                            CB_GETITEMDATA,
                            static_cast<WPARAM>(selected),
                            0));
                    SendDlgItemMessageW(
                        dialog,
                        IDC_SHORTCUT_HOTKEY,
                        HKM_SETHOTKEY,
                        DefaultShortcutHotkey(command_id),
                        0);
                }
                return TRUE;
            }
            if (LOWORD(wparam) == IDOK) {
                const LRESULT selected = SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_COMMAND, CB_GETCURSEL, 0, 0);
                const LRESULT command_id = selected == CB_ERR
                    ? CB_ERR
                    : SendDlgItemMessageW(
                          dialog,
                          IDC_SHORTCUT_COMMAND,
                          CB_GETITEMDATA,
                          static_cast<WPARAM>(selected),
                          0);
                const WORD hotkey = static_cast<WORD>(SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_HOTKEY, HKM_GETHOTKEY, 0, 0));
                if (command_id == CB_ERR || LOBYTE(hotkey) == 0U) {
                    if (!state->close_immediately) {
                        MessageBoxW(
                            dialog,
                            L"コマンドとキーを指定してください。",
                            L"inkpod",
                            MB_OK | MB_ICONWARNING);
                    }
                    return TRUE;
                }
                state->command_id = static_cast<std::uint32_t>(command_id);
                state->virtual_key = LOBYTE(hotkey);
                state->modifiers = ShortcutModifiers(HIBYTE(hotkey));
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR ShowShortcutEditor(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    ShortcutDialogState& state) noexcept {
    state.close_immediately = close_immediately;
    return DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_SHORTCUT_EDITOR),
        owner,
        ShortcutDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

struct M6EditorState {
    const wchar_t* title{L"M6 エディター"};
    std::array<const wchar_t*, 5U> parameter_labels{L"P0", L"P1", L"P2", L"P3", L"P4"};
    std::array<std::int32_t, 5U> parameters{};
    std::array<const wchar_t*, 5U> channel_labels{};
    std::array<std::uint32_t, 5U> channel_values{};
    std::size_t channel_count{};
    std::uint32_t channel{};
    std::array<const wchar_t*, 4U> mode_labels{};
    std::array<std::uint32_t, 4U> mode_values{};
    std::size_t mode_count{};
    std::uint32_t mode{};
    std::wstring points;
    const wchar_t* option1_label{L"プレビューして確認"};
    const wchar_t* option2_label{L"45度制約 / 筆圧"};
    bool option1{true};
    bool option2{};
    bool option1_enabled{true};
    bool option2_enabled{true};
    bool close_immediately{};
};

bool ReadSignedDialogValue(HWND dialog, int control, std::int32_t& output) noexcept {
    std::array<wchar_t, 64U> text{};
    if (GetDlgItemTextW(dialog, control, text.data(), static_cast<int>(text.size())) <= 0) {
        return false;
    }
    wchar_t* end{};
    errno = 0;
    const long value = wcstol(text.data(), &end, 10);
    if (errno == ERANGE || end == text.data() || *end != L'\0' || value < INT32_MIN
        || value > INT32_MAX) {
        return false;
    }
    output = static_cast<std::int32_t>(value);
    return true;
}

template <std::size_t Count>
void FillM6Combo(
    HWND combo,
    const std::array<const wchar_t*, Count>& labels,
    const std::array<std::uint32_t, Count>& values,
    std::size_t count,
    std::uint32_t selected_value) noexcept {
    int selected = 0;
    for (std::size_t index = 0; index < count; ++index) {
        const LRESULT item = SendMessageW(
            combo, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(labels[index]));
        if (item == CB_ERR || item == CB_ERRSPACE) {
            continue;
        }
        SendMessageW(combo, CB_SETITEMDATA, item, static_cast<LPARAM>(values[index]));
        if (values[index] == selected_value) {
            selected = static_cast<int>(item);
        }
    }
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
    EnableWindow(combo, count != 0U ? TRUE : FALSE);
}

std::uint32_t SelectedM6Combo(HWND dialog, int control, std::uint32_t fallback) noexcept {
    const LRESULT selected = SendDlgItemMessageW(dialog, control, CB_GETCURSEL, 0, 0);
    if (selected == CB_ERR) {
        return fallback;
    }
    const LRESULT value = SendDlgItemMessageW(
        dialog, control, CB_GETITEMDATA, static_cast<WPARAM>(selected), 0);
    return value == CB_ERR ? fallback : static_cast<std::uint32_t>(value);
}

INT_PTR CALLBACK M6EditorDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<M6EditorState*>(GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<M6EditorState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            SetDlgItemTextW(dialog, IDC_M6_TITLE, state->title);
            constexpr std::array<int, 5U> labels{
                IDC_M6_PARAM0_LABEL,
                IDC_M6_PARAM1_LABEL,
                IDC_M6_PARAM2_LABEL,
                IDC_M6_PARAM3_LABEL,
                IDC_M6_PARAM4_LABEL};
            constexpr std::array<int, 5U> edits{
                IDC_M6_PARAM0, IDC_M6_PARAM1, IDC_M6_PARAM2, IDC_M6_PARAM3, IDC_M6_PARAM4};
            for (std::size_t index = 0; index < edits.size(); ++index) {
                SetDlgItemTextW(dialog, labels[index], state->parameter_labels[index]);
                std::array<wchar_t, 32U> value{};
                _snwprintf_s(
                    value.data(), value.size(), _TRUNCATE, L"%d", state->parameters[index]);
                SetDlgItemTextW(dialog, edits[index], value.data());
            }
            FillM6Combo(
                GetDlgItem(dialog, IDC_M6_CHANNEL),
                state->channel_labels,
                state->channel_values,
                state->channel_count,
                state->channel);
            FillM6Combo(
                GetDlgItem(dialog, IDC_M6_MODE),
                state->mode_labels,
                state->mode_values,
                state->mode_count,
                state->mode);
            SetDlgItemTextW(dialog, IDC_M6_POINTS, state->points.c_str());
            SetDlgItemTextW(dialog, IDC_M6_OPTION1, state->option1_label);
            SetDlgItemTextW(dialog, IDC_M6_OPTION2, state->option2_label);
            CheckDlgButton(dialog, IDC_M6_OPTION1, state->option1 ? BST_CHECKED : BST_UNCHECKED);
            CheckDlgButton(dialog, IDC_M6_OPTION2, state->option2 ? BST_CHECKED : BST_UNCHECKED);
            EnableWindow(GetDlgItem(dialog, IDC_M6_OPTION1), state->option1_enabled ? TRUE : FALSE);
            EnableWindow(GetDlgItem(dialog, IDC_M6_OPTION2), state->option2_enabled ? TRUE : FALSE);
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                constexpr std::array<int, 5U> edits{
                    IDC_M6_PARAM0, IDC_M6_PARAM1, IDC_M6_PARAM2, IDC_M6_PARAM3, IDC_M6_PARAM4};
                for (std::size_t index = 0; index < edits.size(); ++index) {
                    if (!ReadSignedDialogValue(dialog, edits[index], state->parameters[index])) {
                        if (!state->close_immediately) {
                            MessageBoxW(
                                dialog,
                                L"数値パラメーターを10進整数で入力してください。",
                                L"inkpod",
                                MB_OK | MB_ICONWARNING);
                        }
                        return TRUE;
                    }
                }
                state->channel = SelectedM6Combo(dialog, IDC_M6_CHANNEL, state->channel);
                state->mode = SelectedM6Combo(dialog, IDC_M6_MODE, state->mode);
                std::array<wchar_t, 1024U> points{};
                GetDlgItemTextW(
                    dialog, IDC_M6_POINTS, points.data(), static_cast<int>(points.size()));
                try {
                    state->points.assign(points.data());
                } catch (const std::bad_alloc&) {
                    return TRUE;
                }
                state->option1 = IsDlgButtonChecked(dialog, IDC_M6_OPTION1) == BST_CHECKED;
                state->option2 = IsDlgButtonChecked(dialog, IDC_M6_OPTION2) == BST_CHECKED;
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR ShowM6Editor(
    HINSTANCE instance, HWND owner, bool close_immediately, M6EditorState& state) noexcept {
    state.close_immediately = close_immediately;
    return DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_M6_EDITOR),
        owner,
        M6EditorDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

INT_PTR CALLBACK M6ProgressDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<AppState*>(GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            state = reinterpret_cast<AppState*>(lparam);
            if (state == nullptr) {
                DestroyWindow(dialog);
                return TRUE;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            SendDlgItemMessageW(dialog, IDC_M6_PROGRESS_BAR, PBM_SETRANGE32, 0, 1000);
            SetTimer(dialog, kM6ProgressTimer, 100U, nullptr);
            return TRUE;
        case WM_TIMER:
            if (state != nullptr && wparam == kM6ProgressTimer && state->m6_task != nullptr) {
                InkpodM6TaskInfo info{};
                info.struct_size = sizeof(info);
                if (inkpod_m6_task_query(state->m6_task, &info) == INKPOD_STATUS_OK) {
                    const std::uint64_t value = info.total_work == 0U
                        ? 0U
                        : std::min<std::uint64_t>(
                              1000U, info.completed_work * 1000U / info.total_work);
                    SendDlgItemMessageW(dialog, IDC_M6_PROGRESS_BAR, PBM_SETPOS, value, 0);
                    std::array<wchar_t, 64U> text{};
                    _snwprintf_s(
                        text.data(),
                        text.size(),
                        _TRUNCATE,
                        L"処理中... %llu / %llu",
                        static_cast<unsigned long long>(info.completed_work),
                        static_cast<unsigned long long>(info.total_work));
                    SetDlgItemTextW(dialog, IDC_M6_PROGRESS_TEXT, text.data());
                }
                return TRUE;
            }
            break;
        case WM_COMMAND:
            if (LOWORD(wparam) == IDCANCEL && state != nullptr && state->m6_task != nullptr) {
                inkpod_m6_task_cancel(state->m6_task);
                EnableWindow(GetDlgItem(dialog, IDCANCEL), FALSE);
                SetDlgItemTextW(dialog, IDC_M6_PROGRESS_TEXT, L"キャンセル中...");
                return TRUE;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr && state->m6_task != nullptr) {
                inkpod_m6_task_cancel(state->m6_task);
            }
            return TRUE;
        case WM_NCDESTROY:
            KillTimer(dialog, kM6ProgressTimer);
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

std::uint32_t CurrentShortcutModifiers(LPARAM key_data) noexcept {
    std::uint32_t modifiers{};
    if ((GetKeyState(VK_CONTROL) & 0x8000) != 0) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_CONTROL;
    }
    if ((GetKeyState(VK_SHIFT) & 0x8000) != 0) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_SHIFT;
    }
    if ((GetKeyState(VK_MENU) & 0x8000) != 0) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_ALT;
    }
    if ((static_cast<std::uint64_t>(key_data) & (UINT64_C(1) << 24)) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_EXTENDED;
    }
    return modifiers;
}

UINT ShortcutMenuCommand(std::uint32_t command_id) noexcept {
    switch (command_id) {
        case 1U:
            return IDM_EDIT_UNDO;
        case 2U:
            return IDM_EDIT_REDO;
        case 3U:
            return IDM_EDIT_COPY;
        case 4U:
            return IDM_EDIT_PASTE;
        default:
            return 0U;
    }
}

bool ResolveConfiguredShortcut(
    AppState& state,
    std::uint32_t virtual_key,
    std::uint32_t modifiers,
    UINT& menu_command) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    std::uint32_t command_id{};
    const InkpodStatus status = state.engine->Invoke(
        [virtual_key, modifiers, &command_id](InkpodCore* core) {
            return inkpod_core_shortcut_resolve(
                core, virtual_key, modifiers, &command_id);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return false;
    }
    menu_command = ShortcutMenuCommand(command_id);
    return menu_command != 0U;
}

void ShowCoreError(const AppState& state, HWND owner, const wchar_t* operation) noexcept {
    const std::wstring detail = state.engine == nullptr
        ? L"Core engine is not running"
        : state.engine->LastError();
    std::array<wchar_t, 768> message{};
    _snwprintf_s(
        message.data(),
        message.size(),
        _TRUNCATE,
        L"%ls に失敗しました。\n\n%ls",
        operation,
        detail.c_str());
    MessageBoxW(owner, message.data(), L"inkpod", MB_OK | MB_ICONERROR);
}

bool WidePathToUtf8(
    const std::wstring& path, std::vector<std::uint8_t>& output) noexcept {
    if (path.empty() || path.size() > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int required = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        path.data(),
        static_cast<int>(path.size()),
        nullptr,
        0,
        nullptr,
        nullptr);
    if (required <= 0) {
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(required));
    } catch (const std::bad_alloc&) {
        return false;
    }
    return WideCharToMultiByte(
               CP_UTF8,
               WC_ERR_INVALID_CHARS,
               path.data(),
               static_cast<int>(path.size()),
               reinterpret_cast<char*>(output.data()),
               required,
               nullptr,
               nullptr)
        == required;
}

bool EnsureDirectory(const std::wstring& path) noexcept {
    if (CreateDirectoryW(path.c_str(), nullptr) != FALSE) {
        return true;
    }
    if (GetLastError() != ERROR_ALREADY_EXISTS) {
        return false;
    }
    const DWORD attributes = GetFileAttributesW(path.c_str());
    return attributes != INVALID_FILE_ATTRIBUTES
        && (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0U;
}

bool RecoveryDirectory(std::wstring& output) noexcept {
    PWSTR local_app_data{};
    if (FAILED(SHGetKnownFolderPath(
            FOLDERID_LocalAppData, KF_FLAG_CREATE, nullptr, &local_app_data))) {
        return false;
    }
    try {
        std::wstring root(local_app_data);
        CoTaskMemFree(local_app_data);
        local_app_data = nullptr;
        root += L"\\inkpod";
        if (!EnsureDirectory(root)) {
            return false;
        }
        root += L"\\Recovery";
        if (!EnsureDirectory(root)) {
            return false;
        }
        output = std::move(root);
        return true;
    } catch (const std::bad_alloc&) {
        if (local_app_data != nullptr) {
            CoTaskMemFree(local_app_data);
        }
        return false;
    }
}

bool PrivateRecoveryPath(
    std::uint64_t uuid_high,
    std::uint64_t uuid_low,
    std::wstring& output) noexcept {
    std::wstring directory;
    if (!RecoveryDirectory(directory)) {
        return false;
    }
    std::array<wchar_t, 96> name{};
    _snwprintf_s(
        name.data(),
        name.size(),
        _TRUNCATE,
        L"\\%016llx%016llx.inkpod",
        static_cast<unsigned long long>(uuid_high),
        static_cast<unsigned long long>(uuid_low));
    try {
        output = directory + name.data();
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool NewestPrivateRecovery(std::wstring& output) noexcept {
    std::wstring directory;
    if (!RecoveryDirectory(directory)) {
        return false;
    }
    std::wstring pattern;
    try {
        pattern = directory + L"\\*.inkpod";
    } catch (const std::bad_alloc&) {
        return false;
    }
    WIN32_FIND_DATAW entry{};
    HANDLE search = FindFirstFileW(pattern.c_str(), &entry);
    if (search == INVALID_HANDLE_VALUE) {
        return false;
    }
    FILETIME newest{};
    bool found{};
    do {
        if ((entry.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) == 0U
            && (!found || CompareFileTime(&entry.ftLastWriteTime, &newest) > 0)) {
            try {
                output = directory + L"\\" + entry.cFileName;
            } catch (const std::bad_alloc&) {
                FindClose(search);
                return false;
            }
            newest = entry.ftLastWriteTime;
            found = true;
        }
    } while (FindNextFileW(search, &entry) != FALSE);
    FindClose(search);
    return found;
}

InkpodDocumentInfo EmptyDocumentInfo() noexcept {
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    return info;
}

bool QueryDocument(AppState& state, InkpodDocumentInfo& info) noexcept {
    info = EmptyDocumentInfo();
    return state.engine != nullptr && state.engine->GetDocumentInfo(info);
}

bool ParseCurvePoints(
    const std::wstring& text, std::vector<InkpodCurvePoint>& points) noexcept {
    points.clear();
    const wchar_t* cursor = text.c_str();
    try {
        while (*cursor != L'\0') {
            wchar_t* end{};
            const unsigned long input = wcstoul(cursor, &end, 10);
            if (end == cursor || *end != L':' || input > UINT16_MAX) {
                return false;
            }
            cursor = end + 1;
            const unsigned long output = wcstoul(cursor, &end, 10);
            if (end == cursor || output > UINT16_MAX
                || (*end != L';' && *end != L'\0')) {
                return false;
            }
            points.push_back(InkpodCurvePoint{
                sizeof(InkpodCurvePoint),
                0U,
                static_cast<std::uint32_t>(input),
                static_cast<std::uint32_t>(output)});
            cursor = *end == L';' ? end + 1 : end;
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    return points.size() >= 2U && points.size() <= 64U;
}

bool ParseM6Stops(
    const std::wstring& text,
    std::vector<M6StopValue>& stops,
    std::size_t minimum_stops = 3U) noexcept {
    stops.clear();
    const wchar_t* cursor = text.c_str();
    try {
        while (*cursor != L'\0') {
            wchar_t* end{};
            const unsigned long position = wcstoul(cursor, &end, 10);
            if (end == cursor || *end != L':' || position > 1000U) {
                return false;
            }
            cursor = end + 1;
            const unsigned long color = wcstoul(cursor, &end, 16);
            if (end == cursor || color > UINT32_MAX
                || (*end != L';' && *end != L'\0')) {
                return false;
            }
            stops.push_back(M6StopValue{
                static_cast<std::uint32_t>(position), static_cast<std::uint32_t>(color)});
            cursor = *end == L';' ? end + 1 : end;
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    return stops.size() >= minimum_stops && stops.size() <= 16U
        && std::is_sorted(
            stops.begin(),
            stops.end(),
            [](const M6StopValue& left, const M6StopValue& right) {
                return left.position_milli < right.position_milli;
            })
        && stops.front().position_milli == 0U && stops.back().position_milli == 1000U;
}

InkpodColorValue M6Color(std::uint32_t rgba) noexcept {
    return InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        static_cast<std::uint16_t>((rgba >> 24U) & 0xffU),
        static_cast<std::uint16_t>((rgba >> 16U) & 0xffU),
        static_cast<std::uint16_t>((rgba >> 8U) & 0xffU),
        static_cast<std::uint16_t>(rgba & 0xffU)};
}

InkpodFilterInput M6FilterInputFor(const M6FilterJob& job) noexcept {
    InkpodFilterInput input{};
    input.struct_size = sizeof(input);
    input.kind = job.kind;
    input.plane_id = job.plane_id;
    input.channel = job.channel;
    input.interpolation = job.interpolation;
    input.parameter_0 = job.parameters[0];
    input.parameter_1 = job.parameters[1];
    input.parameter_2 = job.parameters[2];
    input.parameter_3 = job.parameters[3];
    input.parameter_4 = job.parameters[4];
    if (!job.points.empty()) {
        input.points = job.points.data();
        input.point_count = job.points.size();
        input.point_stride_bytes = sizeof(InkpodCurvePoint);
    }
    return input;
}

M6AdjustmentUiState* CurrentM6Adjustment(AppState& state) noexcept {
    const auto found = std::find_if(
        state.m6_adjustments.begin(),
        state.m6_adjustments.end(),
        [&state](const M6AdjustmentUiState& adjustment) {
            return adjustment.id == state.m6_adjustment_id;
        });
    return found == state.m6_adjustments.end() ? nullptr : &*found;
}

bool FormatCurvePoints(
    const std::vector<InkpodCurvePoint>& points, std::wstring& text) noexcept {
    try {
        text.clear();
        for (std::size_t index = 0; index < points.size(); ++index) {
            if (index != 0U) {
                text.push_back(L';');
            }
            text.append(std::to_wstring(points[index].input));
            text.push_back(L':');
            text.append(std::to_wstring(points[index].output));
        }
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

InkpodStatus StartM6Task(
    AppState& state,
    bool preview_prompt,
    std::function<InkpodStatus(InkpodCore*, InkpodM6Task*)> operation) noexcept {
    if (state.engine == nullptr || state.m6_task != nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodM6Task* task{};
    InkpodStatus status = inkpod_m6_task_create(&task);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    if (state.smoke_test) {
        status = state.engine->Invoke(
            [task, operation = std::move(operation)](InkpodCore* core) {
                return operation(core, task);
            },
            true,
            true);
        if (status == INKPOD_STATUS_OK && preview_prompt) {
            status = state.engine->Invoke(
                [](InkpodCore* core) {
                    InkpodDispatchResult result{};
                    result.struct_size = sizeof(result);
                    return inkpod_core_filter_preview_apply(core, &result);
                },
                true,
                true);
        }
        inkpod_m6_task_release(&task);
        return status;
    }
    state.m6_task = task;
    state.m6_preview_prompt = preview_prompt;
    state.m6_progress = CreateDialogParamW(
        state.instance,
        MAKEINTRESOURCEW(IDD_M6_PROGRESS),
        state.window,
        M6ProgressDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
    if (state.m6_progress == nullptr) {
        inkpod_m6_task_release(&state.m6_task);
        return INKPOD_STATUS_INVALID_STATE;
    }
    ShowWindow(state.m6_progress, SW_SHOW);
    const HWND window = state.window;
    if (!state.engine->Enqueue(
            [task, operation = std::move(operation)](InkpodCore* core) {
                return operation(core, task);
            },
            true,
            true,
            true,
            [window](InkpodStatus completion_status) {
                PostMessageW(window, kM6TaskCompleted, completion_status, 0);
            })) {
        DestroyWindow(state.m6_progress);
        state.m6_progress = nullptr;
        inkpod_m6_task_release(&state.m6_task);
        state.m6_preview_prompt = false;
        return INKPOD_STATUS_INVALID_STATE;
    }
    return INKPOD_STATUS_OK;
}

std::uint32_t M6FilterKindForCommand(UINT command) noexcept {
    switch (command) {
        case IDM_FILTER_SHARPEN_WEAK:
            return INKPOD_FILTER_SHARPEN_WEAK;
        case IDM_FILTER_SHARPEN_STRONG:
            return INKPOD_FILTER_SHARPEN_STRONG;
        case IDM_FILTER_BLUR_WEAK:
            return INKPOD_FILTER_BLUR_WEAK;
        case IDM_FILTER_BLUR_STRONG:
            return INKPOD_FILTER_BLUR_STRONG;
        case IDM_FILTER_GAUSSIAN:
            return INKPOD_FILTER_GAUSSIAN_BLUR;
        case IDM_FILTER_INVERT:
            return INKPOD_FILTER_INVERT;
        case IDM_FILTER_AUTO_CONTRAST:
            return INKPOD_FILTER_AUTO_CONTRAST;
        case IDM_FILTER_BRIGHTNESS:
            return INKPOD_FILTER_BRIGHTNESS_CONTRAST;
        case IDM_FILTER_TONE_CURVE:
            return INKPOD_FILTER_TONE_CURVE;
        case IDM_FILTER_LEVELS:
            return INKPOD_FILTER_LEVELS;
        case IDM_FILTER_HSV:
            return INKPOD_FILTER_HSV;
        case IDM_FILTER_COLOR_BALANCE:
            return INKPOD_FILTER_COLOR_BALANCE;
        case IDM_FILTER_UNSHARP:
            return INKPOD_FILTER_UNSHARP_MASK;
        default:
            return 0U;
    }
}

bool ConfigureM6FilterEditor(
    AppState& state, UINT command, M6FilterJob& job) noexcept {
    job.kind = M6FilterKindForCommand(command);
    if (job.kind == 0U) {
        return false;
    }
    M6EditorState editor{};
    editor.title = L"フィルタ（選択範囲があればその内側だけに適用）";
    editor.parameter_labels = {
        L"P0 / radius", L"P1 / amount", L"P2", L"P3", L"P4"};
    editor.channel_labels = {L"RGB", L"Red", L"Green", L"Blue"};
    editor.channel_values = {
        INKPOD_FILTER_CHANNEL_RGB,
        INKPOD_FILTER_CHANNEL_RED,
        INKPOD_FILTER_CHANNEL_GREEN,
        INKPOD_FILTER_CHANNEL_BLUE};
    editor.channel_count = editor.channel_labels.size();
    editor.channel = INKPOD_FILTER_CHANNEL_RGB;
    editor.mode_labels = {L"Bezier", L"B-spline", nullptr, nullptr};
    editor.mode_values = {INKPOD_CURVE_BEZIER, INKPOD_CURVE_BSPLINE, 0U, 0U};
    editor.mode_count = 2U;
    editor.mode = INKPOD_CURVE_BEZIER;
    editor.points = L"0:0;32768:32768;65535:65535";
    editor.option1 = true;
    editor.option2_enabled = false;
    if (job.kind == INKPOD_FILTER_GAUSSIAN_BLUR) {
        editor.parameters = {3, 1000, 0, 0, 0};
    } else if (job.kind == INKPOD_FILTER_LEVELS) {
        editor.parameters = {0, 1000, 65535, 0, 65535};
    } else if (job.kind == INKPOD_FILTER_UNSHARP_MASK) {
        editor.parameters = {2, 1000, 0, 0, 0};
    }
    if (ShowM6Editor(state.instance, state.window, state.smoke_test, editor) != IDOK) {
        return false;
    }
    job.channel = editor.channel;
    job.interpolation = editor.mode;
    job.parameters = editor.parameters;
    job.preview = editor.option1;
    if (job.kind == INKPOD_FILTER_TONE_CURVE
        && !ParseCurvePoints(editor.points, job.points)) {
        if (!state.smoke_test) {
            MessageBoxW(
                state.window,
                L"トーンカーブ点は input:output;...（各0～65535）で2点以上指定してください。",
                L"inkpod",
                MB_OK | MB_ICONWARNING);
        }
        return false;
    }
    return true;
}

InkpodStatus QueueM6Filter(AppState& state, M6FilterJob job) noexcept {
    return StartM6Task(
        state,
        job.preview,
        [job = std::move(job)](InkpodCore* core, InkpodM6Task* task) {
            const InkpodFilterInput input = M6FilterInputFor(job);
            InkpodFilterPreviewInfo preview{};
            preview.struct_size = sizeof(preview);
            InkpodStatus status = inkpod_core_filter_preview_begin_task(
                core, &input, task, &preview);
            if (status == INKPOD_STATUS_OK && !job.preview) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                status = inkpod_core_filter_preview_apply(core, &result);
                if (status != INKPOD_STATUS_OK) {
                    inkpod_core_filter_preview_cancel(core, &preview);
                }
            }
            return status;
        });
}

bool ConfigureM6AdjustmentEditor(
    AppState& state, M6FilterJob& job, bool update) noexcept {
    M6EditorState editor{};
    editor.title = L"調整レイヤー（作成後も同じ項目から再編集可能）";
    editor.parameter_labels = {
        L"P0 / 明るさ / shadow",
        L"P1 / contrast / gamma",
        L"P2 / highlight",
        L"P3 / output shadow",
        L"P4 / output highlight"};
    editor.channel_labels = {
        L"明るさ・コントラスト", L"トーンカーブ", L"レベル補正", nullptr, nullptr};
    editor.channel_values = {
        INKPOD_FILTER_BRIGHTNESS_CONTRAST,
        INKPOD_FILTER_TONE_CURVE,
        INKPOD_FILTER_LEVELS,
        0U,
        0U};
    editor.channel_count = 3U;
    editor.channel = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
    editor.mode_labels = {L"Bezier", L"B-spline", nullptr, nullptr};
    editor.mode_values = {INKPOD_CURVE_BEZIER, INKPOD_CURVE_BSPLINE, 0U, 0U};
    editor.mode_count = 2U;
    editor.mode = INKPOD_CURVE_BEZIER;
    editor.points = L"0:0;32768:32768;65535:65535";
    editor.option1 = false;
    editor.option2 = false;
    editor.option1_enabled = false;
    editor.option2_enabled = false;
    if (update) {
        const M6AdjustmentUiState* current = CurrentM6Adjustment(state);
        if (current == nullptr) {
            return false;
        }
        try {
            job = current->job;
        } catch (const std::bad_alloc&) {
            return false;
        }
        editor.channel = job.kind;
        editor.mode = job.interpolation;
        editor.parameters = job.parameters;
        if (job.kind == INKPOD_FILTER_TONE_CURVE
            && !FormatCurvePoints(job.points, editor.points)) {
            return false;
        }
    }
    if (ShowM6Editor(state.instance, state.window, state.smoke_test, editor) != IDOK) {
        return false;
    }
    job.kind = editor.channel;
    job.channel = INKPOD_FILTER_CHANNEL_RGB;
    job.interpolation = editor.mode;
    job.parameters = editor.parameters;
    if (job.kind == INKPOD_FILTER_TONE_CURVE
        && !ParseCurvePoints(editor.points, job.points)) {
        if (!state.smoke_test) {
            MessageBoxW(
                state.window,
                L"トーンカーブ点は input:output;... 形式で2点以上指定してください。",
                L"inkpod",
                MB_OK | MB_ICONWARNING);
        }
        return false;
    }
    return true;
}

InkpodStatus CreateOrUpdateM6Adjustment(
    AppState& state, M6FilterJob job, bool update) noexcept {
    if (state.engine == nullptr || (update && state.m6_adjustment_id == 0U)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t layer_id = state.m6_adjustment_id;
    std::shared_ptr<M6AdjustmentUiState> pending;
    try {
        std::string name;
        if (update) {
            const M6AdjustmentUiState* current = CurrentM6Adjustment(state);
            if (current == nullptr) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            name = current->name;
        } else {
            name = "M6 Adjustment " + std::to_string(state.m6_adjustments.size() + 1U);
            state.m6_adjustments.reserve(state.m6_adjustments.size() + 1U);
        }
        pending = std::make_shared<M6AdjustmentUiState>(
            M6AdjustmentUiState{layer_id, true, std::move(job), std::move(name)});
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::uint64_t created_id{};
    const InkpodStatus status = state.engine->Invoke(
        [pending, update, layer_id, &created_id](InkpodCore* core) {
            const InkpodFilterInput input = M6FilterInputFor(pending->job);
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            if (update) {
                return inkpod_core_adjustment_update(core, layer_id, &input, &result);
            }
            return inkpod_core_adjustment_create(
                core,
                &input,
                reinterpret_cast<const std::uint8_t*>(pending->name.data()),
                pending->name.size(),
                &result,
                &created_id);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK && !update) {
        pending->id = created_id;
        state.m6_adjustment_id = created_id;
        state.m6_adjustment_visible = true;
        state.m6_adjustments.push_back(std::move(*pending));
    } else if (status == INKPOD_STATUS_OK) {
        M6AdjustmentUiState* current = CurrentM6Adjustment(state);
        if (current != nullptr) {
            current->job = std::move(pending->job);
        }
    }
    return status;
}

InkpodStatus SetM6AdjustmentVisibility(AppState& state, bool visible) noexcept {
    if (state.engine == nullptr || state.m6_adjustment_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t layer_id = state.m6_adjustment_id;
    M6AdjustmentUiState* current = CurrentM6Adjustment(state);
    if (current == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [layer_id, visible, current](InkpodCore* core) {
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_SET_LAYER_PROPERTIES;
            edit.flags = INKPOD_NODE_EDITABLE | (visible ? INKPOD_NODE_VISIBLE : 0U);
            edit.object_id = layer_id;
            edit.opacity_milli = 1000U;
            edit.name_utf8 = reinterpret_cast<const std::uint8_t*>(current->name.data());
            edit.name_bytes = current->name.size();
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t ignored{};
            return inkpod_core_tree_edit(core, &edit, &result, &ignored);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK) {
        current->visible = visible;
    }
    return status;
}

bool SelectM6Adjustment(AppState& state, bool next) noexcept {
    if (state.m6_adjustments.empty()) {
        return false;
    }
    const auto current = std::find_if(
        state.m6_adjustments.begin(),
        state.m6_adjustments.end(),
        [&state](const M6AdjustmentUiState& adjustment) {
            return adjustment.id == state.m6_adjustment_id;
        });
    std::size_t index = current == state.m6_adjustments.end()
        ? 0U
        : static_cast<std::size_t>(current - state.m6_adjustments.begin());
    if (next) {
        index = (index + 1U) % state.m6_adjustments.size();
    } else {
        index = (index + state.m6_adjustments.size() - 1U)
            % state.m6_adjustments.size();
    }
    state.m6_adjustment_id = state.m6_adjustments[index].id;
    state.m6_adjustment_visible = state.m6_adjustments[index].visible;
    return true;
}

std::vector<InkpodGradientStop> M6GradientStops(const std::vector<M6StopValue>& values) {
    std::vector<InkpodGradientStop> stops;
    stops.reserve(values.size());
    for (const M6StopValue& value : values) {
        stops.push_back(InkpodGradientStop{
            sizeof(InkpodGradientStop),
            0U,
            value.position_milli,
            0U,
            M6Color(value.rgba)});
    }
    return stops;
}

InkpodStatus QueueBoundaryAirbrush(AppState& state, const M6ToolOptions& options) noexcept {
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodColorValue> colors;
    try {
        colors.reserve(options.stops.size());
        for (const M6StopValue& stop : options.stops) {
            colors.push_back(M6Color(stop.rgba));
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint32_t width = static_cast<std::uint32_t>(options.parameters[0]);
    const std::uint32_t strength = static_cast<std::uint32_t>(options.parameters[1]);
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [colors = std::move(colors), width, strength, plane_id](InkpodCore* core) {
                   InkpodBoundaryAirbrushInput input{};
                   input.struct_size = sizeof(input);
                   input.plane_id = plane_id;
                   input.width = width;
                   input.strength_milli = strength;
                   input.colors = InkpodColorArray{
                       sizeof(InkpodColorArray),
                       0U,
                       INKPOD_FEATURE_NONE,
                       colors.data(),
                       colors.size(),
                       sizeof(InkpodColorValue)};
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_effect_boundary_airbrush(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

bool ConfigureM6Effect(AppState& state, UINT command) noexcept {
    M6EditorState editor{};
    editor.option1 = false;
    editor.option2 = false;
    editor.channel_labels = {L"ペン", L"矩形", L"折れ線", L"投げ縄", nullptr};
    editor.channel_values = {
        INKPOD_SELECTION_TRACE,
        INKPOD_SELECTION_RECTANGLE,
        INKPOD_SELECTION_POLYLINE,
        INKPOD_SELECTION_LASSO,
        0U};
    editor.channel_count = 4U;
    editor.channel = INKPOD_SELECTION_TRACE;
    editor.points = L"0:00000000;500:80808080;1000:ffffffff";
    std::uint32_t interaction{};
    switch (command) {
        case IDM_EFFECT_GRADIENT:
        case IDM_EFFECT_ALPHA_GRADIENT:
            editor.title = command == IDM_EFFECT_GRADIENT
                ? L"グラデーション（3～16 stops、Canvasをドラッグ）"
                : L"アルファグラデーション（RGBは保持、Canvasをドラッグ）";
            editor.parameter_labels = {L"未使用", L"未使用", L"未使用", L"未使用", L"未使用"};
            editor.channel_labels = {L"合成", L"上書き", nullptr, nullptr, nullptr};
            editor.channel_values = {
                INKPOD_GRADIENT_COMPOSITE, INKPOD_GRADIENT_OVERWRITE, 0U, 0U, 0U};
            editor.channel_count = 2U;
            editor.channel = INKPOD_GRADIENT_OVERWRITE;
            editor.mode_labels = {L"線形", L"放射", nullptr, nullptr};
            editor.mode_values = {INKPOD_GRADIENT_LINEAR, INKPOD_GRADIENT_RADIAL, 0U, 0U};
            editor.mode_count = 2U;
            editor.mode = INKPOD_GRADIENT_LINEAR;
            editor.option1_label = L"ディザー";
            editor.option2_label = L"45度制約";
            interaction = command == IDM_EFFECT_GRADIENT ? kInteractionM6Gradient
                                                         : kInteractionM6AlphaGradient;
            break;
        case IDM_EFFECT_AIRBRUSH:
            editor.title = L"エアブラシ（Canvasをドラッグ）";
            editor.parameter_labels = {
                L"半径 milli", L"硬さ 0-1000", L"間隔 milli", L"不透明度", L"fade"};
            editor.parameters = {8000, 500, 2000, 500, 0};
            editor.channel_count = 0U;
            editor.mode_count = 0U;
            editor.points.clear();
            editor.option1 = true;
            editor.option2 = true;
            editor.option1_label = L"筆圧で不透明度";
            editor.option2_label = L"筆圧でサイズ";
            interaction = kInteractionM6Airbrush;
            break;
        case IDM_EFFECT_BOUNDARY_AIRBRUSH:
            editor.title = L"境界色エアブラシ（現在の選択範囲へ適用）";
            editor.parameter_labels = {L"幅 px", L"強さ 0-1000", L"未使用", L"未使用", L"未使用"};
            editor.parameters = {3, 500, 0, 0, 0};
            editor.channel_count = 0U;
            editor.mode_count = 0U;
            editor.points = L"0:ff0000ff;500:00ff00ff;1000:0000ffff";
            editor.option1_enabled = false;
            editor.option2_enabled = false;
            break;
        case IDM_EFFECT_BLUR:
            editor.title = L"ぼかしツール（領域をCanvasで指定）";
            editor.parameter_labels = {
                L"ぼかし半径", L"強さ 0-1000", L"ペン径 px", L"未使用", L"未使用"};
            editor.parameters = {3, 750, 24, 0, 0};
            editor.mode_count = 0U;
            editor.points.clear();
            editor.option1 = true;
            editor.option1_label = L"ペン範囲を筆圧で細くする";
            editor.option2_enabled = false;
            interaction = kInteractionM6Blur;
            break;
        case IDM_EFFECT_STAMP:
            editor.title = L"スタンプ（Alt+クリックでsource、次にCanvasをドラッグ）";
            editor.parameter_labels = {
                L"半径 milli", L"硬さ 0-1000", L"間隔 milli", L"不透明度", L"未使用"};
            editor.parameters = {8000, 750, 2000, 1000, 0};
            editor.channel_count = 0U;
            editor.mode_labels = {L"円形", L"矩形", nullptr, nullptr};
            editor.mode_values = {INKPOD_STAMP_ROUND, INKPOD_STAMP_SQUARE, 0U, 0U};
            editor.mode_count = 2U;
            editor.mode = INKPOD_STAMP_ROUND;
            editor.points.clear();
            editor.option1 = true;
            editor.option2 = true;
            editor.option1_label = L"筆圧で不透明度";
            editor.option2_label = L"筆圧でサイズ";
            interaction = kInteractionM6Stamp;
            break;
        case IDM_EFFECT_DUST:
            editor.title = L"ゴミ取り（全体または領域を指定、vector planeは拒否）";
            editor.parameter_labels = {
                L"最大pixel数", L"ペン径 px", L"未使用", L"未使用", L"未使用"};
            editor.parameters = {8, 24, 0, 0, 0};
            editor.channel_labels = {L"全体", L"ペン", L"矩形", L"折れ線", L"投げ縄"};
            editor.channel_values = {
                0U,
                INKPOD_SELECTION_TRACE,
                INKPOD_SELECTION_RECTANGLE,
                INKPOD_SELECTION_POLYLINE,
                INKPOD_SELECTION_LASSO};
            editor.channel_count = 5U;
            editor.channel = 0U;
            editor.mode_labels = {L"前景ゴミ除去", L"透明穴埋め", L"色外れ置換", nullptr};
            editor.mode_values = {
                INKPOD_DUST_REMOVE_FOREGROUND,
                INKPOD_DUST_FILL_TRANSPARENT_HOLES,
                INKPOD_DUST_REPLACE_COLOR_OUTLIERS,
                0U};
            editor.mode_count = 3U;
            editor.mode = INKPOD_DUST_REMOVE_FOREGROUND;
            editor.points.clear();
            editor.option1 = true;
            editor.option1_label = L"プレビューして確認";
            editor.option2_enabled = false;
            interaction = kInteractionM6Dust;
            break;
        default:
            return false;
    }
    if (ShowM6Editor(state.instance, state.window, state.smoke_test, editor) != IDOK) {
        return false;
    }
    M6ToolOptions options{};
    options.parameters = editor.parameters;
    options.shape = editor.channel;
    options.mode = editor.mode;
    options.option = editor.option1;
    options.option2 = editor.option2;
    if (command == IDM_EFFECT_GRADIENT || command == IDM_EFFECT_ALPHA_GRADIENT
        || command == IDM_EFFECT_BOUNDARY_AIRBRUSH) {
        const std::size_t minimum_stops = command == IDM_EFFECT_BOUNDARY_AIRBRUSH ? 2U : 3U;
        if (!ParseM6Stops(editor.points, options.stops, minimum_stops)) {
            if (!state.smoke_test) {
                MessageBoxW(
                    state.window,
                    command == IDM_EFFECT_BOUNDARY_AIRBRUSH
                        ? L"境界色は position:RRGGBBAA;... 形式で2～16個、0から1000まで昇順に指定してください。"
                        : L"stopは position:RRGGBBAA;... 形式で3～16個、0から1000まで昇順に指定してください。",
                    L"inkpod",
                    MB_OK | MB_ICONWARNING);
            }
            return false;
        }
    }
    if (command == IDM_EFFECT_GRADIENT || command == IDM_EFFECT_ALPHA_GRADIENT) {
        options.parameters[0] = editor.option1 ? 1 : 0;
    }
    state.m6_options = std::move(options);
    state.tool = interaction;
    state.m6_samples.clear();
    if (command == IDM_EFFECT_BOUNDARY_AIRBRUSH) {
        state.tool = INKPOD_TOOL_BRUSH;
        return QueueBoundaryAirbrush(state, state.m6_options) == INKPOD_STATUS_OK;
    }
    return true;
}

InkpodStatus QueueM6GradientGesture(
    AppState& state, std::vector<InkpodStrokeSample> samples, bool alpha_only) noexcept {
    InkpodDocumentInfo document{};
    if (samples.size() < 2U || !QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodGradientStop> stops;
    try {
        stops = M6GradientStops(state.m6_options.stops);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [samples = std::move(samples), stops = std::move(stops), options, plane_id, alpha_only](
                   InkpodCore* core) {
                   InkpodLocatorOutput start{};
                   InkpodLocatorOutput end{};
                   start.struct_size = sizeof(start);
                   end.struct_size = sizeof(end);
                   InkpodStatus status = inkpod_core_locator_sample(
                       core, 0U, samples.front().x, samples.front().y, &start);
                   if (status == INKPOD_STATUS_OK) {
                       status = inkpod_core_locator_sample(
                           core, 0U, samples.back().x, samples.back().y, &end);
                   }
                   InkpodGradientInput input{};
                   input.struct_size = sizeof(input);
                   input.kind = options.mode;
                   input.feature_flags = options.option2
                       ? INKPOD_GRADIENT_FLAG_CONSTRAIN_45
                       : INKPOD_FEATURE_NONE;
                   input.plane_id = plane_id;
                   input.mode = options.shape;
                   input.dither = options.parameters[0] != 0 ? 1U : 0U;
                   input.start_x_milli = static_cast<std::int64_t>(start.document_x) * 1000;
                   input.start_y_milli = static_cast<std::int64_t>(start.document_y) * 1000;
                   input.end_x_milli = static_cast<std::int64_t>(end.document_x) * 1000;
                   input.end_y_milli = static_cast<std::int64_t>(end.document_y) * 1000;
                   input.stops = stops.data();
                   input.stop_count = stops.size();
                   input.stop_stride_bytes = sizeof(InkpodGradientStop);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   if (status != INKPOD_STATUS_OK) {
                       return status;
                   }
                   return alpha_only ? inkpod_core_alpha_gradient(core, &input, &result)
                                     : inkpod_core_effect_gradient(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus QueueM6AirbrushGesture(
    AppState& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (samples.empty() || !QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodColorValue color = M6Color(state.color_rgba);
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [samples = std::move(samples), options, color, plane_id](InkpodCore* core) {
                   InkpodAirbrushGestureInput input{};
                   input.struct_size = sizeof(input);
                   input.coordinate_space = INKPOD_COORDINATE_SPACE_DEVICE;
                   input.feature_flags = options.option2 ? INKPOD_EFFECT_FLAG_PRESSURE_SIZE : 0U;
                   if (options.option) {
                       input.feature_flags |= INKPOD_EFFECT_FLAG_PRESSURE_OPACITY;
                   }
                   input.plane_id = plane_id;
                   input.radius_milli = static_cast<std::uint32_t>(options.parameters[0]);
                    input.hardness_milli = static_cast<std::uint32_t>(options.parameters[1]);
                    input.spacing_milli = static_cast<std::uint32_t>(options.parameters[2]);
                    input.opacity_milli = static_cast<std::uint32_t>(options.parameters[3]);
                    input.fade_milli = static_cast<std::uint32_t>(options.parameters[4]);
                   input.continuous_dabs = 1U;
                   input.color = color;
                   input.samples = samples.data();
                   input.sample_count = samples.size();
                   input.sample_stride_bytes = sizeof(InkpodStrokeSample);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_effect_airbrush_gesture(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus QueueM6StampGesture(
    AppState& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (!state.stamp_source_valid || samples.empty() || !QueryDocument(state, document)
        || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStrokeSample source = state.stamp_source;
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [samples = std::move(samples), options, source, plane_id](InkpodCore* core) {
                   InkpodStampGestureInput input{};
                   input.struct_size = sizeof(input);
                   input.coordinate_space = INKPOD_COORDINATE_SPACE_DEVICE;
                   input.feature_flags = options.option2 ? INKPOD_EFFECT_FLAG_PRESSURE_SIZE : 0U;
                   if (options.option) {
                       input.feature_flags |= INKPOD_EFFECT_FLAG_PRESSURE_OPACITY;
                   }
                   input.plane_id = plane_id;
                   input.source = source;
                   input.radius_milli = static_cast<std::uint32_t>(options.parameters[0]);
                    input.hardness_milli = static_cast<std::uint32_t>(options.parameters[1]);
                    input.spacing_milli = static_cast<std::uint32_t>(options.parameters[2]);
                    input.opacity_milli = static_cast<std::uint32_t>(options.parameters[3]);
                    input.shape = options.mode;
                    input.samples = samples.data();
                   input.sample_count = samples.size();
                   input.sample_stride_bytes = sizeof(InkpodStrokeSample);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_effect_stamp_gesture(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus QueueM6BlurGesture(
    AppState& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (samples.empty() || !QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [samples = std::move(samples), options, plane_id](InkpodCore* core) {
                   InkpodBlurToolInput input{};
                   input.struct_size = sizeof(input);
                   input.coordinate_space = INKPOD_COORDINATE_SPACE_DEVICE;
                   input.feature_flags = options.option ? INKPOD_EFFECT_FLAG_PRESSURE_SIZE : 0U;
                   input.plane_id = plane_id;
                   input.radius = static_cast<std::uint32_t>(options.parameters[0]);
                   input.strength_milli = static_cast<std::uint32_t>(options.parameters[1]);
                   input.shape = options.shape;
                   input.diameter = static_cast<float>(options.parameters[2]);
                   input.samples = samples.data();
                   input.sample_count = samples.size();
                   input.sample_stride_bytes = sizeof(InkpodStrokeSample);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_effect_blur_tool(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus QueueM6Dust(
    AppState& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = document.color_plane_id;
    const bool preview = options.option;
    return StartM6Task(
        state,
        preview,
        [samples = std::move(samples), options, plane_id, preview](
            InkpodCore* core, InkpodM6Task* task) {
            InkpodDustInput input{};
            input.struct_size = sizeof(input);
            input.mode = options.mode;
            input.plane_id = plane_id;
            input.coordinate_space = INKPOD_COORDINATE_SPACE_DEVICE;
            input.shape = options.shape;
            input.maximum_pixels = static_cast<std::uint32_t>(options.parameters[0]);
            input.use_region = options.shape == 0U ? 0U : 1U;
            input.diameter = static_cast<float>(options.parameters[1]);
            if (input.use_region != 0U) {
                input.samples = samples.data();
                input.sample_count = samples.size();
                input.sample_stride_bytes = sizeof(InkpodStrokeSample);
            }
            if (preview) {
                InkpodFilterPreviewInfo info{};
                info.struct_size = sizeof(info);
                return inkpod_core_dust_preview_begin(core, &input, task, &info);
            }
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_dust_remove(core, &input, task, &result);
        });
}

InkpodStatus FinishM6CanvasGesture(AppState& state) noexcept {
    std::vector<InkpodStrokeSample> samples;
    samples.swap(state.m6_samples);
    switch (state.tool) {
        case kInteractionM6Gradient:
            return QueueM6GradientGesture(state, std::move(samples), false);
        case kInteractionM6AlphaGradient:
            return QueueM6GradientGesture(state, std::move(samples), true);
        case kInteractionM6Airbrush:
            return QueueM6AirbrushGesture(state, std::move(samples));
        case kInteractionM6Blur:
            return QueueM6BlurGesture(state, std::move(samples));
        case kInteractionM6Stamp:
            return QueueM6StampGesture(state, std::move(samples));
        case kInteractionM6Dust:
            return QueueM6Dust(state, std::move(samples));
        default:
            return INKPOD_STATUS_INVALID_STATE;
    }
}

void UpdateMenuState(AppState& state) noexcept {
    HMENU menu = GetMenu(state.window);
    if (menu == nullptr) {
        return;
    }
    InkpodDocumentInfo info{};
    const bool has_document = QueryDocument(state, info);
    EnableMenuItem(
        menu,
        IDM_FILE_SAVE,
        MF_BYCOMMAND | (has_document ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_FILE_SAVE_AS,
        MF_BYCOMMAND | (has_document ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_FILE_REVERT,
        MF_BYCOMMAND
            | (has_document && !state.current_path.empty() ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_FILE_AUTOSAVE_NOW,
        MF_BYCOMMAND | (has_document ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_EDIT_UNDO,
        MF_BYCOMMAND
            | (has_document && (info.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) != 0U
                   ? MF_ENABLED
                   : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_EDIT_REDO,
        MF_BYCOMMAND
            | (has_document && (info.flags & INKPOD_DOCUMENT_FLAG_CAN_REDO) != 0U
                   ? MF_ENABLED
                   : MF_GRAYED));
    for (const UINT command : {
             IDM_EDIT_COPY,
             IDM_EDIT_MIRROR_HORIZONTAL,
             IDM_LAYER_DUPLICATE,
             IDM_LAYER_MOVE_TOP,
             IDM_SELECTION_ALL,
             IDM_SELECTION_INVERT,
             IDM_SELECTION_EXPAND,
             IDM_SELECTION_SHRINK,
             IDM_VIEW_FLIP_HORIZONTAL,
             IDM_VIEW_FLIP_VERTICAL,
             IDM_VIEW_GRID,
             IDM_VIEW_NEW}) {
        EnableMenuItem(
            menu, command, MF_BYCOMMAND | (has_document ? MF_ENABLED : MF_GRAYED));
    }
    for (const UINT command : {
             IDM_FILTER_LAST,
             IDM_FILTER_INVERT,
             IDM_FILTER_BLUR_WEAK,
             IDM_FILTER_SHARPEN_WEAK,
             IDM_FILTER_SHARPEN_STRONG,
             IDM_FILTER_BLUR_STRONG,
             IDM_FILTER_GAUSSIAN,
             IDM_FILTER_AUTO_CONTRAST,
             IDM_FILTER_BRIGHTNESS,
             IDM_FILTER_TONE_CURVE,
             IDM_FILTER_LEVELS,
             IDM_FILTER_HSV,
             IDM_FILTER_COLOR_BALANCE,
             IDM_FILTER_UNSHARP,
             IDM_EFFECT_GRADIENT,
             IDM_EFFECT_AIRBRUSH,
             IDM_EFFECT_BOUNDARY_AIRBRUSH,
             IDM_EFFECT_BLUR,
             IDM_EFFECT_STAMP,
             IDM_EFFECT_DUST,
             IDM_EFFECT_ALPHA_GRADIENT,
             IDM_EFFECT_ALPHA_VIEW}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND
                | (has_document && state.plane == INKPOD_PLANE_COLOR ? MF_ENABLED : MF_GRAYED));
    }
    EnableMenuItem(
        menu,
        IDM_ADJUSTMENT_CREATE,
        MF_BYCOMMAND
            | (has_document && state.plane == INKPOD_PLANE_COLOR
                   ? MF_ENABLED
                   : MF_GRAYED));
    for (const UINT command : {
             IDM_ADJUSTMENT_EDIT,
             IDM_ADJUSTMENT_TOGGLE,
             IDM_ADJUSTMENT_MOVE_TOP}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND | (has_document && state.m6_adjustment_id != 0U ? MF_ENABLED
                                                                          : MF_GRAYED));
    }
    for (const UINT command : {IDM_ADJUSTMENT_PREVIOUS, IDM_ADJUSTMENT_NEXT}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND
                | (has_document && state.m6_adjustments.size() > 1U ? MF_ENABLED : MF_GRAYED));
    }
    CheckMenuItem(
        menu,
        IDM_ADJUSTMENT_TOGGLE,
        MF_BYCOMMAND | (state.m6_adjustment_visible ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_EFFECT_ALPHA_VIEW,
        MF_BYCOMMAND | (state.alpha_view ? MF_CHECKED : MF_UNCHECKED));
    EnableMenuItem(
        menu,
        IDM_EDIT_PASTE,
        MF_BYCOMMAND
            | (has_document && state.clipboard != nullptr ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_LAYER_DELETE,
        MF_BYCOMMAND
            | (has_document && state.m3_layer_id != 0U ? MF_ENABLED : MF_GRAYED));
    CheckMenuItem(
        menu,
        IDM_VIEW_FLIP_HORIZONTAL,
        MF_BYCOMMAND | (state.view_flip_horizontal ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_FLIP_VERTICAL,
        MF_BYCOMMAND | (state.view_flip_vertical ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_GRID,
        MF_BYCOMMAND | (state.grid_visible ? MF_CHECKED : MF_UNCHECKED));
    for (const UINT command : {
             IDM_TOOL_PENCIL, IDM_TOOL_BRUSH, IDM_TOOL_ERASER, IDM_TOOL_FILL,
             IDM_TOOL_EYEDROPPER,
             IDM_PLANE_MAIN_LINE, IDM_PLANE_COLOR}) {
        CheckMenuItem(menu, command, MF_BYCOMMAND | MF_UNCHECKED);
    }
    const UINT tool_command = state.tool == INKPOD_TOOL_PENCIL
        ? IDM_TOOL_PENCIL
        : (state.tool == INKPOD_TOOL_BRUSH
                  ? IDM_TOOL_BRUSH
                  : (state.tool == INKPOD_TOOL_ERASER
                            ? IDM_TOOL_ERASER
                            : (state.tool == kInteractionFill ? IDM_TOOL_FILL
                                                               : IDM_TOOL_EYEDROPPER)));
    CheckMenuItem(menu, tool_command, MF_BYCOMMAND | MF_CHECKED);
    CheckMenuItem(
        menu,
        state.plane == INKPOD_PLANE_MAIN_LINE ? IDM_PLANE_MAIN_LINE : IDM_PLANE_COLOR,
        MF_BYCOMMAND | MF_CHECKED);
    for (const UINT command : {
             IDM_COLOR_CHECK_OFF, IDM_COLOR_CHECK_LEGACY, IDM_COLOR_CHECK_NATIVE}) {
        CheckMenuItem(menu, command, MF_BYCOMMAND | MF_UNCHECKED);
    }
    const UINT check_command = state.color_check_mode == INKPOD_COLOR_CHECK_LEGACY_WHITE
        ? IDM_COLOR_CHECK_LEGACY
        : (state.color_check_mode == INKPOD_COLOR_CHECK_NATIVE_ALPHA
                  ? IDM_COLOR_CHECK_NATIVE
                  : IDM_COLOR_CHECK_OFF);
    CheckMenuItem(menu, check_command, MF_BYCOMMAND | MF_CHECKED);

    std::array<wchar_t, 1024> title{};
    const wchar_t* name = state.current_path.empty()
        ? ((info.flags & INKPOD_DOCUMENT_FLAG_RECOVERED) != 0U ? L"Recovery" : L"無題")
        : state.current_path.c_str();
    _snwprintf_s(
        title.data(),
        title.size(),
        _TRUNCATE,
        L"%ls%ls - inkpod",
        name,
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U ? L" *" : L"");
    SetWindowTextW(state.window, title.data());
    DrawMenuBar(state.window);
}

InkpodStatus ApplyView(
    AppState& state,
    InkpodViewCommandKind kind,
    double value1,
    double value2,
    double value3 = 0.0) noexcept {
    const InkpodViewInput input{
        sizeof(InkpodViewInput), kind, 0U, value1, value2, value3, 0.0};
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [input](InkpodCore* core) {
                  InkpodDocumentInfo info = EmptyDocumentInfo();
                  return inkpod_core_apply_view(core, &input, &info);
              },
              true,
              true);
}

InkpodStatus FitCanvas(AppState& state, InkpodViewCommandKind kind) noexcept {
    RECT client{};
    GetClientRect(state.canvas, &client);
    return ApplyView(
        state,
        kind,
        static_cast<double>(client.right - client.left),
        static_cast<double>(client.bottom - client.top));
}

InkpodStatus ApplyTreeEdit(
    AppState& state,
    InkpodTreeOperation operation,
    std::uint64_t object_id,
    std::uint32_t destination_index,
    std::uint64_t& out_object_id) noexcept {
    InkpodTreeEdit edit{};
    edit.struct_size = sizeof(edit);
    edit.operation = operation;
    edit.object_id = object_id;
    edit.destination_index = destination_index;
    out_object_id = 0U;
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [edit, &out_object_id](InkpodCore* core) {
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_tree_edit(
                      core, &edit, &result, &out_object_id);
              },
              true,
              true);
}

InkpodStatus AdjustSelection(
    AppState& state, std::uint32_t operation, std::uint32_t pixels) noexcept {
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [operation, pixels](InkpodCore* core) {
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_selection_adjust(
                      core, operation, pixels, &result);
              },
              true,
              true);
}

InkpodStatus CreateDefaultCell(AppState& state) noexcept {
    GUID uuid{};
    if (FAILED(CoCreateGuid(&uuid))) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    static_assert(sizeof(uuid) == sizeof(std::uint64_t) * 2U);
    std::uint64_t uuid_high{};
    std::uint64_t uuid_low{};
    std::memcpy(&uuid_high, &uuid, sizeof(uuid_high));
    std::memcpy(
        &uuid_low,
        reinterpret_cast<const std::uint8_t*>(&uuid) + sizeof(uuid_high),
        sizeof(uuid_low));
    std::wstring private_recovery_path;
    if (!PrivateRecoveryPath(uuid_high, uuid_low, private_recovery_path)) {
        return INKPOD_STATUS_IO_ERROR;
    }
    const InkpodCellCreateOptions options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        uuid_high,
        uuid_low,
        1920U,
        1080U,
        96000U,
        96000U};
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [options](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_new_cell(core, &options, &info);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    state.current_path.clear();
    state.recovery_path = std::move(private_recovery_path);
    state.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    state.plane = INKPOD_PLANE_MAIN_LINE;
    ResetM3DocumentUiState(state);
    const InkpodPlaneKind plane = state.plane;
    const InkpodStatus plane_status = state.engine->Invoke(
        [plane](InkpodCore* core) { return inkpod_core_set_active_plane(core, plane); },
        false,
        false);
    if (plane_status != INKPOD_STATUS_OK) {
        return plane_status;
    }
    return FitCanvas(state, INKPOD_VIEW_FIT);
}

bool ChooseInkpodPath(
    HWND owner, bool save, std::wstring& selected_path) noexcept {
    std::array<wchar_t, 32768> path{};
    if (!selected_path.empty()) {
        wcsncpy_s(path.data(), path.size(), selected_path.c_str(), _TRUNCATE);
    }
    constexpr wchar_t filter[] = L"inkpod セル (*.inkpod)\0*.inkpod\0すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.lpstrDefExt = L"inkpod";
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        | (save ? OFN_OVERWRITEPROMPT : OFN_FILEMUSTEXIST);
    const BOOL accepted = save ? GetSaveFileNameW(&dialog) : GetOpenFileNameW(&dialog);
    if (accepted == FALSE) {
        return false;
    }
    try {
        selected_path.assign(path.data());
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

InkpodStatus SaveToPath(AppState& state, const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::wstring old_recovery_path;
    std::wstring next_current_path;
    std::wstring next_recovery_path;
    try {
        old_recovery_path = state.recovery_path;
        next_current_path = path;
        next_recovery_path = path + L".recovery.inkpod";
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_save(core, utf8.data(), utf8.size(), &info);
        },
        false,
        true);
    if (status == INKPOD_STATUS_OK) {
        state.current_path = std::move(next_current_path);
        state.recovery_path = std::move(next_recovery_path);
        if (!old_recovery_path.empty() && old_recovery_path != path) {
            DeleteFileW(old_recovery_path.c_str());
        }
        UpdateMenuState(state);
    }
    return status;
}

InkpodStatus SaveDocument(AppState& state, bool force_dialog) noexcept {
    std::wstring path = state.current_path;
    if (force_dialog || path.empty()) {
        if (!ChooseInkpodPath(state.window, true, path)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }
    return SaveToPath(state, path);
}

InkpodStatus OpenFromPath(AppState& state, const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_open(core, utf8.data(), utf8.size(), &info);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    try {
        state.current_path = path;
        state.recovery_path = path + L".recovery.inkpod";
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    state.plane = INKPOD_PLANE_MAIN_LINE;
    state.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetM3DocumentUiState(state);
    const InkpodPlaneKind plane = state.plane;
    const InkpodStatus plane_status = state.engine->Invoke(
        [plane](InkpodCore* core) { return inkpod_core_set_active_plane(core, plane); },
        false,
        false);
    if (plane_status != INKPOD_STATUS_OK) {
        return plane_status;
    }
    const InkpodStatus view_status = FitCanvas(state, INKPOD_VIEW_FIT);
    UpdateMenuState(state);
    return view_status;
}

InkpodStatus OpenRecoveryFromPath(AppState& state, const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodStatus status = state.engine->Invoke(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_open_recovery(core, utf8.data(), utf8.size(), &info);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    state.current_path.clear();
    try {
        state.recovery_path = path;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    state.plane = INKPOD_PLANE_MAIN_LINE;
    state.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetM3DocumentUiState(state);
    const InkpodStatus plane_status = state.engine->Invoke(
        [](InkpodCore* core) {
            return inkpod_core_set_active_plane(core, INKPOD_PLANE_MAIN_LINE);
        },
        false,
        false);
    if (plane_status != INKPOD_STATUS_OK) {
        return plane_status;
    }
    const InkpodStatus view_status = FitCanvas(state, INKPOD_VIEW_FIT);
    UpdateMenuState(state);
    return view_status;
}

bool RecoveryIsNewer(
    const std::wstring& normal_path, const std::wstring& recovery_path) noexcept {
    WIN32_FILE_ATTRIBUTE_DATA recovery{};
    if (GetFileAttributesExW(
            recovery_path.c_str(), GetFileExInfoStandard, &recovery) == FALSE) {
        return false;
    }
    WIN32_FILE_ATTRIBUTE_DATA normal{};
    if (GetFileAttributesExW(normal_path.c_str(), GetFileExInfoStandard, &normal) == FALSE) {
        return true;
    }
    return CompareFileTime(&recovery.ftLastWriteTime, &normal.ftLastWriteTime) > 0;
}

bool QueueAutosave(AppState& state, const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (state.engine == nullptr || !WidePathToUtf8(path, utf8)) {
        return false;
    }
    return state.engine->Enqueue(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_autosave(core, utf8.data(), utf8.size(), &info);
        },
        false,
        false,
        true);
}

InkpodStatus ApplyFillAtDevicePoint(AppState& state, float device_x, float device_y) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1
        || info.width == 0U || info.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double document_x = (static_cast<double>(device_x) - bounds.left) / zoom;
    const double document_y = (static_cast<double>(device_y) - bounds.top) / zoom;
    if (!std::isfinite(document_x) || !std::isfinite(document_y) || document_x < 0.0
        || document_y < 0.0 || document_x >= static_cast<double>(info.width)
        || document_y >= static_cast<double>(info.height)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodFillInput input{};
    input.struct_size = sizeof(input);
    input.operation = INKPOD_FILL_SEED;
    input.flags = INKPOD_FILL_FLAG_OVERFLOW_ABORT;
    input.seed_x = static_cast<std::uint32_t>(std::floor(document_x));
    input.seed_y = static_cast<std::uint32_t>(std::floor(document_y));
    input.color = InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        static_cast<std::uint16_t>((state.color_rgba >> 24) & 0xffU),
        static_cast<std::uint16_t>((state.color_rgba >> 16) & 0xffU),
        static_cast<std::uint16_t>((state.color_rgba >> 8) & 0xffU),
        static_cast<std::uint16_t>(state.color_rgba & 0xffU)};
    input.inclusion_mode = INKPOD_INCLUSION_NONE;
    input.inclusion_color_stride_bytes = sizeof(InkpodColorValue);
    const InkpodStatus status = state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [input](InkpodCore* core) {
                  InkpodFillResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_apply_fill(core, &input, &result);
              },
              true,
              true);
    if (status == INKPOD_STATUS_OK) {
        state.plane = INKPOD_PLANE_COLOR;
    }
    return status;
}

InkpodStatus EyedropAtDevicePoint(AppState& state, float device_x, float device_y) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1
        || info.width == 0U || info.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    const double document_x = (static_cast<double>(device_x) - bounds.left) / zoom;
    const double document_y = (static_cast<double>(device_y) - bounds.top) / zoom;
    if (!std::isfinite(document_x) || !std::isfinite(document_y) || document_x < 0.0
        || document_y < 0.0 || document_x >= static_cast<double>(info.width)
        || document_y >= static_cast<double>(info.height)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodColorValue sampled{};
    sampled.struct_size = sizeof(sampled);
    const auto x = static_cast<std::uint32_t>(std::floor(document_x));
    const auto y = static_cast<std::uint32_t>(std::floor(document_y));
    const InkpodStatus status = state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [x, y, &sampled](InkpodCore* core) {
                  return inkpod_core_eyedropper(
                      core, INKPOD_EYEDROPPER_COMPOSITE, x, y, &sampled);
              },
              false,
              false);
    if (status == INKPOD_STATUS_OK && sampled.depth == INKPOD_COLOR_DEPTH_8) {
        state.color_rgba = (static_cast<std::uint32_t>(sampled.red) << 24)
            | (static_cast<std::uint32_t>(sampled.green) << 16)
            | (static_cast<std::uint32_t>(sampled.blue) << 8)
            | static_cast<std::uint32_t>(sampled.alpha);
    }
    return status;
}

bool DiscardCurrentRecovery(AppState& state) noexcept {
    if (state.recovery_path.empty()) {
        return true;
    }
    if (state.engine != nullptr && state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return false;
    }
    if (DeleteFileW(state.recovery_path.c_str()) == FALSE
        && GetLastError() != ERROR_FILE_NOT_FOUND) {
        return false;
    }
    state.recovery_path.clear();
    return true;
}

bool ConfirmDiscard(AppState& state) noexcept {
    InkpodDocumentInfo info{};
    if (!QueryDocument(state, info)) {
        return true;
    }
    if ((info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        return true;
    }
    const int choice = MessageBoxW(
        state.window,
        L"変更を保存しますか？",
        L"inkpod",
        MB_YESNOCANCEL | MB_ICONQUESTION);
    if (choice == IDCANCEL) {
        return false;
    }
    if (choice == IDYES) {
        const InkpodStatus status = SaveDocument(state, false);
        if (status != INKPOD_STATUS_OK) {
            if (status != INKPOD_STATUS_INVALID_STATE) {
                ShowCoreError(state, state.window, L"保存");
            }
            return false;
        }
    } else if (!DiscardCurrentRecovery(state)) {
        ShowCoreError(state, state.window, L"Recoveryの破棄");
        return false;
    }
    return true;
}

bool SameFrame(const InkpodFrameRect& left, const InkpodFrameRect& right) noexcept {
    return left.x == right.x && left.y == right.y && left.width == right.width
        && left.height == right.height;
}

bool SamePersistentMetadata(
    const InkpodDocumentInfo& left, const InkpodDocumentInfo& right) noexcept {
    return left.document_id == right.document_id && left.layer_id == right.layer_id
        && left.document_uuid_high == right.document_uuid_high
        && left.document_uuid_low == right.document_uuid_low
        && left.main_plane_id == right.main_plane_id
        && left.color_plane_id == right.color_plane_id && left.width == right.width
        && left.height == right.height && left.dpi_x_milli == right.dpi_x_milli
        && left.dpi_y_milli == right.dpi_y_milli
        && SameFrame(left.hundred_frame, right.hundred_frame)
        && SameFrame(left.reference_frame, right.reference_frame)
        && SameFrame(left.drawing_frame, right.drawing_frame)
        && SameFrame(left.safe_frame, right.safe_frame)
        && left.margin_left == right.margin_left && left.margin_top == right.margin_top
        && left.margin_right == right.margin_right && left.margin_bottom == right.margin_bottom
        && left.main_plane_checksum == right.main_plane_checksum
        && left.color_plane_checksum == right.color_plane_checksum;
}

void PumpPendingWindowMessages() noexcept {
    MSG message{};
    while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE) != FALSE) {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
}

int RunM1Smoke(AppState& state) noexcept {
    const HMENU menu = GetMenu(state.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_HELP_ABOUT, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || SendMessageW(state.window, WM_COMMAND, IDM_HELP_ABOUT, 0) != 1) {
        return 29;
    }
    if (state.engine == nullptr
        || MoveWindow(state.canvas, 0, 0, 640, 480, FALSE) == FALSE
        || FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 30;
    }
    const std::wstring initial_recovery_path = state.recovery_path;
    std::wstring discovered_recovery;
    if (initial_recovery_path.empty()
        || !QueueAutosave(state, initial_recovery_path)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || GetFileAttributesW(initial_recovery_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || !NewestPrivateRecovery(discovered_recovery)
        || _wcsicmp(discovered_recovery.c_str(), initial_recovery_path.c_str()) != 0) {
        return 215;
    }
    std::wstring active_stroke_recovery_path;
    try {
        active_stroke_recovery_path = initial_recovery_path + L".active-stroke-test";
    } catch (const std::bad_alloc&) {
        return 217;
    }
    if (DeleteFileW(active_stroke_recovery_path.c_str()) == FALSE
        && GetLastError() != ERROR_FILE_NOT_FOUND) {
        return 218;
    }
    PumpPendingWindowMessages();
    const DWORD ui_thread = GetCurrentThreadId();
    const DWORD core_thread = state.engine->ThreadId();
    const DWORD renderer_thread = static_cast<DWORD>(SendMessageW(
        state.canvas, inkpod::renderer::kCanvasGetRendererThreadId, 0, 0));
    if (core_thread == 0U || renderer_thread == 0U || core_thread == ui_thread
        || renderer_thread == ui_thread || core_thread == renderer_thread) {
        return 31;
    }
    inkpod::renderer::CanvasDocumentBounds document_bounds{};
    if (SendMessageW(
            state.canvas,
            inkpod::renderer::kCanvasGetDocumentBounds,
            0,
            reinterpret_cast<LPARAM>(&document_bounds))
            != 1
        || std::abs(document_bounds.left - 16.0) > 0.01
        || std::abs(document_bounds.top - 69.0) > 0.01
        || std::abs(document_bounds.right - 624.0) > 0.01
        || std::abs(document_bounds.bottom - 411.0) > 0.01) {
        return 53;
    }

    InkpodDocumentInfo before_line{};
    if (!QueryDocument(state, before_line)) {
        return 32;
    }
    const auto frames_before = static_cast<std::uint64_t>(SendMessageW(
        state.canvas, inkpod::renderer::kCanvasGetPresentedFrameCount, 0, 0));
    SendMessageW(state.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(80, 100));
    for (int x = 90; x <= 240; x += 15) {
        SendMessageW(state.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x, 120));
    }
    if (state.engine->FlushPreview() != INKPOD_STATUS_OK) {
        return 33;
    }
    if (!QueueAutosave(state, active_stroke_recovery_path)) {
        return 219;
    }
    PumpPendingWindowMessages();
    if (SendMessageW(state.canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) {
        return 34;
    }
    InkpodDocumentInfo during_line{};
    const auto frames_during = static_cast<std::uint64_t>(SendMessageW(
        state.canvas, inkpod::renderer::kCanvasGetPresentedFrameCount, 0, 0));
    if (!QueryDocument(state, during_line)) {
        return 130;
    }
    if (during_line.document_revision != before_line.document_revision) {
        return 131;
    }
    if (during_line.main_plane_checksum != before_line.main_plane_checksum) {
        return 132;
    }
    if ((during_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY)
        != (before_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY)) {
        return 133;
    }
    if (frames_during <= frames_before) {
        return 134;
    }
    if (SendMessageW(state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(250, 120)) != 1) {
        return 36;
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 37;
    }
    if (GetFileAttributesW(active_stroke_recovery_path.c_str())
        == INVALID_FILE_ATTRIBUTES) {
        return 220;
    }
    DeleteFileW(active_stroke_recovery_path.c_str());
    PumpPendingWindowMessages();
    InkpodDocumentInfo after_line{};
    if (!QueryDocument(state, after_line)
        || after_line.document_revision != before_line.document_revision + 1U
        || after_line.main_plane_checksum == before_line.main_plane_checksum
        || (after_line.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 38;
    }
    const std::uint64_t line_checksum = after_line.main_plane_checksum;

    SendMessageW(state.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(80, 150));
    SendMessageW(state.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(180, 150));
    SendMessageW(state.canvas, WM_CAPTURECHANGED, 0, 0);
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 54;
    }
    InkpodDocumentInfo after_cancel{};
    if (!QueryDocument(state, after_cancel)
        || after_cancel.document_revision != after_line.document_revision
        || after_cancel.main_plane_checksum != after_line.main_plane_checksum) {
        return 55;
    }

    state.plane = INKPOD_PLANE_COLOR;
    if (state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR);
            },
            false,
            true)
        != INKPOD_STATUS_OK) {
        return 39;
    }
    SendMessageW(state.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(100, 180));
    for (int x = 115; x <= 260; x += 15) {
        SendMessageW(state.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x, 190));
    }
    if (SendMessageW(state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(270, 190)) != 1) {
        return 40;
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 41;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo after_color{};
    if (!QueryDocument(state, after_color)
        || after_color.main_plane_checksum != line_checksum
        || after_color.color_plane_checksum == after_line.color_plane_checksum) {
        return 42;
    }
    const inkpod::app::EngineMetrics metrics = state.engine->Metrics();
    if (metrics.completed_strokes != 2U || metrics.completed_samples <= 2U
        || metrics.preview_snapshots == 0U) {
        return 43;
    }

    if (state.engine->Invoke(
            [](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_undo(core, &result);
            },
            true,
            true)
            != INKPOD_STATUS_OK
        || state.engine->Invoke(
               [](InkpodCore* core) {
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_redo(core, &result);
               },
               true,
               true)
            != INKPOD_STATUS_OK) {
        return 44;
    }
    InkpodDocumentInfo after_redo{};
    if (!QueryDocument(state, after_redo)
        || after_redo.color_plane_checksum != after_color.color_plane_checksum) {
        return 45;
    }

    const std::uint64_t revision_before_view = after_redo.document_revision;
    SendMessageW(state.canvas, WM_MBUTTONDOWN, MK_MBUTTON, MAKELPARAM(300, 220));
    SendMessageW(state.canvas, WM_MOUSEMOVE, MK_MBUTTON, MAKELPARAM(320, 230));
    SendMessageW(state.canvas, WM_MBUTTONUP, 0, MAKELPARAM(320, 230));
    RECT canvas_bounds{};
    GetWindowRect(state.canvas, &canvas_bounds);
    SendMessageW(
        state.canvas,
        WM_MOUSEWHEEL,
        MAKEWPARAM(0, WHEEL_DELTA),
        MAKELPARAM(canvas_bounds.left + 320, canvas_bounds.top + 240));
    InkpodDocumentInfo after_view{};
    if (!QueryDocument(state, after_view)
        || after_view.document_revision != revision_before_view
        || after_view.view_revision == after_redo.view_revision) {
        return 46;
    }

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 47;
    }
    std::array<wchar_t, MAX_PATH> temporary_file{};
    _snwprintf_s(
        temporary_file.data(),
        temporary_file.size(),
        _TRUNCATE,
        L"%lsinkpod-m1-smoke-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring path(temporary_file.data());
    if (SaveToPath(state, path) != INKPOD_STATUS_OK) {
        return 48;
    }
    if (GetFileAttributesW(initial_recovery_path.c_str()) != INVALID_FILE_ATTRIBUTES
        || GetLastError() != ERROR_FILE_NOT_FOUND) {
        DeleteFileW(path.c_str());
        return 216;
    }
    InkpodDocumentInfo saved{};
    if (!QueryDocument(state, saved)
        || (saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        DeleteFileW(path.c_str());
        return 49;
    }
    if (CreateDefaultCell(state) != INKPOD_STATUS_OK
        || OpenFromPath(state, path) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 50;
    }
    InkpodDocumentInfo reopened{};
    const bool round_trip = QueryDocument(state, reopened)
        && SamePersistentMetadata(saved, reopened)
        && (reopened.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U;
    DeleteFileW(path.c_str());
    if (!round_trip) {
        return 51;
    }

    inkpod::renderer::CanvasDocumentBounds before_dpi_bounds{};
    inkpod::renderer::CanvasDocumentBounds after_dpi_bounds{};
    const bool bounds_before_dpi = SendMessageW(
                                       state.canvas,
                                       inkpod::renderer::kCanvasGetDocumentBounds,
                                       0,
                                       reinterpret_cast<LPARAM>(&before_dpi_bounds)) == 1;
    const bool dpi_changed = SendMessageW(
                                 state.canvas,
                                 WM_DPICHANGED_AFTERPARENT,
                                 0,
                                 0) == 1;
    const bool bounds_after_dpi = SendMessageW(
                                      state.canvas,
                                      inkpod::renderer::kCanvasGetDocumentBounds,
                                      0,
                                      reinterpret_cast<LPARAM>(&after_dpi_bounds)) == 1;
    const bool dpi_transform_stable = bounds_before_dpi && bounds_after_dpi
        && std::abs(before_dpi_bounds.left - after_dpi_bounds.left) <= 0.01
        && std::abs(before_dpi_bounds.top - after_dpi_bounds.top) <= 0.01
        && std::abs(before_dpi_bounds.right - after_dpi_bounds.right) <= 0.01
        && std::abs(before_dpi_bounds.bottom - after_dpi_bounds.bottom) <= 0.01;
    const bool device_recovered = SendMessageW(
                                      state.canvas,
                                      inkpod::renderer::kCanvasSimulateDeviceLoss,
                                      0,
                                      0) == 1;
    const bool rendered = SendMessageW(
                              state.canvas,
                              inkpod::renderer::kCanvasRenderOnce,
                              0,
                              0) == 1;
    return dpi_changed && dpi_transform_stable && device_recovered && rendered ? 0 : 52;
}

int RunM2Smoke(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return 200;
    }
    const HMENU menu = GetMenu(state.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_TOOL_FILL, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_TOOL_EYEDROPPER, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_COLOR_CHECK_NATIVE, MF_BYCOMMAND)
            == static_cast<UINT>(-1)) {
        return 201;
    }

    std::array<InkpodStrokeSample, 5> boundary_samples{{
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 100.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 200.0F, 100.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 200.0F, 200.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 200.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 100.0F, 1.0F, 0U},
    }};
    const InkpodStrokeInput boundary{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        boundary_samples.data(),
        boundary_samples.size(),
        sizeof(InkpodStrokeSample)};
    if (state.engine->Invoke(
            [boundary](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &boundary, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 202;
    }
    InkpodDocumentInfo before_fill{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, before_fill)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1) {
        return 203;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(before_fill.width);
    const int fill_x = static_cast<int>(std::lround(bounds.left + 150.0 * zoom));
    const int fill_y = static_cast<int>(std::lround(bounds.top + 150.0 * zoom));
    SendMessageW(state.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    if (SendMessageW(
            state.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1) {
        return 204;
    }
    SendMessageW(state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));
    InkpodDocumentInfo after_fill{};
    if (!QueryDocument(state, after_fill)
        || after_fill.document_revision != before_fill.document_revision + 1U
        || after_fill.main_plane_checksum != before_fill.main_plane_checksum
        || after_fill.color_plane_checksum == before_fill.color_plane_checksum) {
        return 205;
    }

    const std::uint32_t fill_color = state.color_rgba;
    state.color_rgba = UINT32_C(0x010203ff);
    SendMessageW(state.window, WM_COMMAND, IDM_TOOL_EYEDROPPER, 0);
    if (SendMessageW(
            state.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1
        || state.color_rgba != fill_color) {
        return 206;
    }
    SendMessageW(state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));

    const std::uint64_t revision_before_check = after_fill.document_revision;
    const std::uint64_t view_before_check = after_fill.view_revision;
    SendMessageW(state.window, WM_COMMAND, IDM_COLOR_CHECK_NATIVE, 0);
    InkpodDocumentInfo during_check{};
    std::uint64_t check_features{};
    const InkpodStatus check_snapshot_status = state.engine->Invoke(
        [&check_features](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            InkpodSnapshotView view{};
            view.struct_size = sizeof(view);
            status = inkpod_snapshot_get_view(snapshot, &view);
            if (status == INKPOD_STATUS_OK) {
                check_features = view.feature_flags;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (!QueryDocument(state, during_check)
        || check_snapshot_status != INKPOD_STATUS_OK
        || check_features != INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA
        || during_check.document_revision != revision_before_check
        || during_check.view_revision <= view_before_check
        || SendMessageW(state.canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) {
        return 207;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_COLOR_CHECK_OFF, 0);

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 208;
    }
    const auto suffix = static_cast<unsigned long long>(GetTickCount64());
    std::array<wchar_t, MAX_PATH> normal_buffer{};
    std::array<wchar_t, MAX_PATH> recovery_buffer{};
    _snwprintf_s(
        normal_buffer.data(),
        normal_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-m2-normal-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    _snwprintf_s(
        recovery_buffer.data(),
        recovery_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-m2-recovery-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    const std::wstring normal_path(normal_buffer.data());
    const std::wstring recovery_path(recovery_buffer.data());
    if (SaveToPath(state, normal_path) != INKPOD_STATUS_OK) {
        return 209;
    }
    InkpodDocumentInfo normally_saved{};
    if (!QueryDocument(state, normally_saved)) {
        return 210;
    }
    std::array<InkpodStrokeSample, 1> edit_sample{{
        {sizeof(InkpodStrokeSample), 0U, 300.0F, 300.0F, 1.0F, 0U},
    }};
    const InkpodStrokeInput edit{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x010203ff),
        1.0F,
        edit_sample.data(),
        edit_sample.size(),
        sizeof(InkpodStrokeSample)};
    if (state.engine->Invoke(
            [edit](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &edit, &result);
            },
            true,
            true) != INKPOD_STATUS_OK
        || !QueueAutosave(state, recovery_path)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || GetFileAttributesW(normal_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(recovery_path.c_str()) == INVALID_FILE_ATTRIBUTES) {
        DeleteFileW(normal_path.c_str());
        DeleteFileW(recovery_path.c_str());
        return 211;
    }
    InkpodDocumentInfo autosaved{};
    if (!QueryDocument(state, autosaved)
        || (autosaved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        DeleteFileW(normal_path.c_str());
        DeleteFileW(recovery_path.c_str());
        return 212;
    }
    if (CreateDefaultCell(state) != INKPOD_STATUS_OK
        || OpenRecoveryFromPath(state, recovery_path) != INKPOD_STATUS_OK) {
        DeleteFileW(normal_path.c_str());
        DeleteFileW(recovery_path.c_str());
        return 213;
    }
    InkpodDocumentInfo recovered{};
    const bool recovery_state = QueryDocument(state, recovered)
        && (recovered.flags & INKPOD_DOCUMENT_FLAG_RECOVERED) != 0U
        && (recovered.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        && recovered.color_plane_checksum == autosaved.color_plane_checksum
        && state.current_path.empty();
    const InkpodStatus revert_status = state.engine->Invoke(
        [](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_revert(core, &info);
        },
        false,
        false);
    const bool normal_unchanged = OpenFromPath(state, normal_path) == INKPOD_STATUS_OK;
    InkpodDocumentInfo reopened_normal{};
    const bool normal_matches = QueryDocument(state, reopened_normal)
        && reopened_normal.color_plane_checksum == normally_saved.color_plane_checksum
        && reopened_normal.color_plane_checksum != recovered.color_plane_checksum;
    DeleteFileW(normal_path.c_str());
    DeleteFileW(recovery_path.c_str());
    return recovery_state && revert_status == INKPOD_STATUS_INVALID_STATE && normal_unchanged
            && normal_matches
        ? 0
        : 214;
}

int RunM3Smoke(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return 300;
    }
    const HMENU menu = GetMenu(state.window);
    for (const UINT command : {
             IDM_EDIT_COPY,
             IDM_EDIT_PASTE,
             IDM_EDIT_MIRROR_HORIZONTAL,
             IDM_LAYER_DUPLICATE,
             IDM_LAYER_DELETE,
             IDM_LAYER_MOVE_TOP,
             IDM_SELECTION_ALL,
             IDM_SELECTION_INVERT,
             IDM_SELECTION_EXPAND,
             IDM_SELECTION_SHRINK,
             IDM_VIEW_FLIP_HORIZONTAL,
             IDM_VIEW_FLIP_VERTICAL,
             IDM_VIEW_GRID,
             IDM_VIEW_NEW,
             IDM_SHORTCUT_EDIT,
             IDM_SHORTCUT_RESET}) {
        if (menu == nullptr
            || GetMenuState(menu, command, MF_BYCOMMAND)
                == static_cast<UINT>(-1)) {
            return 301;
        }
    }

    const InkpodCellCreateOptions source_options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d33000000000001),
        UINT64_C(0x4d33000000000002),
        8U,
        8U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [source_options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(core, &source_options, &info);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 302;
    }
    ResetM3DocumentUiState(state);
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 303;
    }

    InkpodDocumentInfo initial{};
    if (!QueryDocument(state, initial)) {
        return 304;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_DUPLICATE, 0);
    const std::uint64_t duplicate_id = state.m3_layer_id;
    InkpodDocumentInfo duplicated{};
    if (duplicate_id == 0U || !QueryDocument(state, duplicated)
        || duplicated.document_revision != initial.document_revision + 1U) {
        return 305;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_MOVE_TOP, 0);
    InkpodNodeInfo top_layer{};
    top_layer.struct_size = sizeof(top_layer);
    const InkpodStatus top_status = state.engine->Invoke(
        [&top_layer](InkpodCore* core) {
            return inkpod_core_node_get(
                core, 0U, UINT32_MAX, &top_layer);
        },
        false,
        false);
    if (top_status != INKPOD_STATUS_OK || top_layer.id != duplicate_id) {
        return 306;
    }

    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_DELETE, 0);
    InkpodDocumentInfo after_delete{};
    if (!QueryDocument(state, after_delete)
        || (after_delete.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 307;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    if (state.engine->Invoke(
            [&top_layer](InkpodCore* core) {
                return inkpod_core_node_get(
                    core, 0U, UINT32_MAX, &top_layer);
            },
            false,
            false) != INKPOD_STATUS_OK
        || top_layer.id != duplicate_id) {
        return 308;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    if (state.engine->Invoke(
            [&top_layer](InkpodCore* core) {
                return inkpod_core_node_get(
                    core, 0U, UINT32_MAX, &top_layer);
            },
            false,
            false) != INKPOD_STATUS_OK
        || top_layer.id == duplicate_id) {
        return 309;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_UNDO, 0);

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 310;
    }
    std::array<wchar_t, MAX_PATH> file_buffer{};
    _snwprintf_s(
        file_buffer.data(),
        file_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-m3-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring path(file_buffer.data());
    if (SaveToPath(state, path) != INKPOD_STATUS_OK
        || OpenFromPath(state, path) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 311;
    }
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    const bool reopened_tree = state.engine->Invoke(
                                   [&top_layer](InkpodCore* core) {
                                       return inkpod_core_node_get(
                                           core,
                                           0U,
                                           UINT32_MAX,
                                           &top_layer);
                                   },
                                   false,
                                   false) == INKPOD_STATUS_OK
        && top_layer.id == duplicate_id;
    DeleteFileW(path.c_str());
    if (!reopened_tree) {
        return 312;
    }

    InkpodDocumentInfo before_invalid{};
    if (!QueryDocument(state, before_invalid)) {
        return 313;
    }
    static constexpr std::array<std::uint8_t, 7> invalid_name{
        'I', 'n', 'v', 'a', 'l', 'i', 'd'};
    InkpodTreeEdit invalid_plane{};
    invalid_plane.struct_size = sizeof(invalid_plane);
    invalid_plane.operation = INKPOD_TREE_CREATE_PLANE;
    invalid_plane.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
    invalid_plane.parent_id = duplicate_id;
    invalid_plane.kind = INKPOD_TYPED_PLANE_SELECTION;
    invalid_plane.pixel_format = INKPOD_STORAGE_BINARY8;
    invalid_plane.opacity_milli = 1000U;
    invalid_plane.name_utf8 = invalid_name.data();
    invalid_plane.name_bytes = invalid_name.size();
    InkpodStatus invalid_status = INKPOD_STATUS_OK;
    state.engine->Invoke(
        [&invalid_plane, &invalid_status](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t object_id{};
            invalid_status = inkpod_core_tree_edit(
                core, &invalid_plane, &result, &object_id);
            return INKPOD_STATUS_OK;
        },
        false,
        false);
    InkpodDocumentInfo after_invalid{};
    if (invalid_status != INKPOD_STATUS_INVALID_ARGUMENT
        || !QueryDocument(state, after_invalid)
        || after_invalid.document_revision != before_invalid.document_revision) {
        return 314;
    }

    const auto apply_rectangle = [&state](
                                     std::uint32_t operation,
                                     std::int32_t x,
                                     std::int32_t y,
                                     std::int32_t width,
                                     std::int32_t height) noexcept {
        InkpodSelectionInput input{};
        input.struct_size = sizeof(input);
        input.shape = INKPOD_SELECTION_RECTANGLE;
        input.operation = operation;
        input.bounds = {x, y, width, height};
        return state.engine->Invoke(
            [input](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_selection(core, &input, &result);
            },
            true,
            true);
    };
    if (apply_rectangle(INKPOD_SELECTION_NEW, 0, 0, 4, 4)
            != INKPOD_STATUS_OK
        || apply_rectangle(INKPOD_SELECTION_ADD, 4, 0, 2, 2)
            != INKPOD_STATUS_OK
        || apply_rectangle(INKPOD_SELECTION_SUBTRACT, 0, 0, 1, 4)
            != INKPOD_STATUS_OK
        || apply_rectangle(INKPOD_SELECTION_INTERSECT, 2, 0, 2, 4)
            != INKPOD_STATUS_OK) {
        return 315;
    }
    InkpodLocatorOutput locator{};
    locator.struct_size = sizeof(locator);
    if (state.engine->Invoke(
            [&locator](InkpodCore* core) {
                return inkpod_core_locator_sample(
                    core, 0U, 0.0, 0.0, &locator);
            },
            false,
            false) != INKPOD_STATUS_OK
        || (locator.flags & 1U) == 0U || locator.selection.x != 2
        || locator.selection.y != 0 || locator.selection.width != 2
        || locator.selection.height != 4) {
        return 316;
    }

    const InkpodStrokeSample source_sample{
        sizeof(InkpodStrokeSample), 0U, 6.0F, 6.0F, 1.0F, 0U};
    const InkpodStrokeInput source_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        &source_sample,
        1U,
        sizeof(source_sample)};
    if (state.engine->Invoke(
            [source_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &source_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK
        || apply_rectangle(INKPOD_SELECTION_NEW, 6, 6, 1, 1)
            != INKPOD_STATUS_OK) {
        return 317;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_COPY, 0);
    if (state.clipboard == nullptr) {
        return 318;
    }

    const InkpodCellCreateOptions destination_options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d33000000000003),
        UINT64_C(0x4d33000000000004),
        4U,
        4U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [destination_options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(
                    core, &destination_options, &info);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 319;
    }
    ResetM3DocumentUiState(state);
    const InkpodFloatingTransform floating{
        sizeof(InkpodFloatingTransform),
        0U,
        -4.0,
        -4.0,
        1.0,
        1.0,
        0.0};
    const InkpodStatus paste_status = state.engine->Invoke(
        [&state, floating](InkpodCore* core) {
            InkpodStatus status = inkpod_core_paste_begin(
                core, state.clipboard);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_floating_transform(core, &floating);
            }
            if (status == INKPOD_STATUS_OK) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                status = inkpod_core_floating_commit(core, &result);
            }
            if (status != INKPOD_STATUS_OK) {
                inkpod_core_floating_cancel(core);
            }
            return status;
        },
        true,
        true);
    InkpodColorValue pasted_color{};
    pasted_color.struct_size = sizeof(pasted_color);
    if (paste_status != INKPOD_STATUS_OK
        || state.engine->Invoke(
               [&pasted_color](InkpodCore* core) {
                   return inkpod_core_eyedropper(
                       core,
                       INKPOD_EYEDROPPER_SELECTED_PLANE,
                       2U,
                       2U,
                       &pasted_color);
               },
               false,
               false) != INKPOD_STATUS_OK
        || pasted_color.red != 0U || pasted_color.green != 0U
        || pasted_color.blue != 0U || pasted_color.alpha == 0U) {
        return 320;
    }

    InkpodDocumentInfo before_flip{};
    if (!QueryDocument(state, before_flip)) {
        return 321;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_FLIP_HORIZONTAL, 0);
    InkpodDocumentInfo after_flip{};
    InkpodSnapshotTransform transform{};
    transform.struct_size = sizeof(transform);
    const InkpodStatus transform_status = state.engine->Invoke(
        [&transform](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &snapshot);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(snapshot, &transform);
            }
            const InkpodStatus release_status =
                inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (!QueryDocument(state, after_flip)
        || after_flip.document_revision != before_flip.document_revision
        || after_flip.view_revision <= before_flip.view_revision
        || transform_status != INKPOD_STATUS_OK
        || (transform.flags & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
            == 0U) {
        return 322;
    }
    SendMessageW(
        state.window, WM_COMMAND, IDM_EDIT_MIRROR_HORIZONTAL, 0);
    InkpodDocumentInfo after_mirror{};
    if (!QueryDocument(state, after_mirror)
        || after_mirror.document_revision
            != after_flip.document_revision + 1U
        || after_mirror.view_revision != after_flip.view_revision
        || (after_mirror.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 323;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_UNDO, 0);

    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_NEW, 0);
    if (state.secondary_view_id == 0U) {
        return 324;
    }
    const InkpodViewInput secondary_pan{
        sizeof(InkpodViewInput),
        INKPOD_VIEW_PAN_BY,
        0U,
        5.0,
        0.0,
        0.0,
        0.0};
    const std::uint64_t secondary_view_id = state.secondary_view_id;
    if (state.engine->Invoke(
            [secondary_view_id, secondary_pan](InkpodCore* core) {
                return inkpod_core_view_apply(
                    core, secondary_view_id, &secondary_pan);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 324;
    }
    const InkpodStrokeSample multi_view_sample{
        sizeof(InkpodStrokeSample), 0U, 0.0F, 0.0F, 1.0F, 0U};
    const InkpodStrokeInput multi_view_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        &multi_view_sample,
        1U,
        sizeof(multi_view_sample)};
    if (state.engine->Invoke(
            [multi_view_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &multi_view_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 325;
    }
    std::uint64_t primary_revision{};
    std::uint64_t secondary_revision{};
    double primary_pan_x{};
    double secondary_pan_x{};
    const InkpodStatus multi_view_status = state.engine->Invoke(
        [secondary_view_id,
         &primary_revision,
         &secondary_revision,
         &primary_pan_x,
         &secondary_pan_x](
            InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* primary{};
            InkpodSnapshot* secondary{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &primary);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_build_snapshot_for_view(
                    core, secondary_view_id, &options, &secondary);
            }
            InkpodSnapshotView primary_view{};
            primary_view.struct_size = sizeof(primary_view);
            InkpodSnapshotView secondary_view{};
            secondary_view.struct_size = sizeof(secondary_view);
            InkpodSnapshotTransform primary_transform{};
            primary_transform.struct_size = sizeof(primary_transform);
            InkpodSnapshotTransform secondary_transform{};
            secondary_transform.struct_size = sizeof(secondary_transform);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_view(primary, &primary_view);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_view(
                    secondary, &secondary_view);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(
                    primary, &primary_transform);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(
                    secondary, &secondary_transform);
            }
            if (status == INKPOD_STATUS_OK) {
                primary_revision = primary_view.revision;
                secondary_revision = secondary_view.revision;
                primary_pan_x = primary_transform.pan_x;
                secondary_pan_x = secondary_transform.pan_x;
            }
            const InkpodStatus primary_release =
                inkpod_snapshot_release(&primary);
            const InkpodStatus secondary_release =
                inkpod_snapshot_release(&secondary);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            return primary_release == INKPOD_STATUS_OK
                    && secondary_release == INKPOD_STATUS_OK
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_STATE;
        },
        false,
        false);
    if (multi_view_status != INKPOD_STATUS_OK || primary_revision == 0U
        || primary_revision != secondary_revision
        || primary_pan_x == secondary_pan_x) {
        return 326;
    }

    InkpodGridInput grid{};
    grid.struct_size = sizeof(grid);
    grid.spacing_x = 8U;
    grid.spacing_y = 8U;
    grid.subdivisions = 2U;
    const InkpodStatus navigation_status = state.engine->Invoke(
        [&grid](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t guide_id{};
            InkpodStatus status = inkpod_core_guide_add(
                core,
                INKPOD_GUIDE_VERTICAL,
                2,
                &result,
                &guide_id);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_grid_set(core, &grid, &result);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_shortcut_rebind(
                    core,
                    99U,
                    static_cast<std::uint32_t>('Z'),
                    INKPOD_SHORTCUT_MODIFIER_CONTROL);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_shortcut_rebind(
                    core,
                    1U,
                    static_cast<std::uint32_t>('U'),
                    INKPOD_SHORTCUT_MODIFIER_CONTROL);
            }
            return guide_id != 0U ? status : INKPOD_STATUS_INVALID_STATE;
        },
        true,
        true);
    if (navigation_status != INKPOD_STATUS_OK) {
        return 327;
    }
    UINT shortcut_menu_command{};
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('U'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO
        || ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)) {
        return 328;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_GRID, 0);
    if (SendMessageW(
            state.window, WM_COMMAND, IDM_SHORTCUT_EDIT, 0) != 1) {
        return 329;
    }
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO
        || ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('U'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)) {
        return 330;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SHORTCUT_RESET, 0);
    locator = {};
    locator.struct_size = sizeof(locator);
    if (!state.grid_visible
        || state.engine->Invoke(
               [&locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, 0U, 2.0, 2.0, &locator);
               },
               false,
               false) != INKPOD_STATUS_OK) {
        return 331;
    }
    bool overlay_connected{};
    const InkpodStatus overlay_status = state.engine->Invoke(
        [&overlay_connected](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &snapshot);
            InkpodSnapshotOverlay overlay{};
            overlay.struct_size = sizeof(overlay);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_overlay(snapshot, &overlay);
            }
            if (status == INKPOD_STATUS_OK) {
                overlay_connected =
                    (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE) != 0U
                    && overlay.grid_spacing_x == 8U
                    && overlay.grid_subdivisions == 2U
                    && overlay.guide_count == 1U
                    && overlay.guides != nullptr;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (overlay_status != INKPOD_STATUS_OK || !overlay_connected) {
        return 332;
    }
    return SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) == 1
        ? 0
        : 333;
}

int RunM5Smoke(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return 500;
    }
    const InkpodCellCreateOptions options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d35000000000001),
        UINT64_C(0x4d35000000000002),
        64U,
        64U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(core, &options, &info);
            },
            false,
            false) != INKPOD_STATUS_OK
        || FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 501;
    }

    InkpodSnapshotVectorSegment geometry_before{};
    std::uint64_t vector_path_id{};
    std::uint64_t vector_fill_id{};
    const InkpodStatus vector_status = state.engine->Invoke(
        [&geometry_before, &vector_path_id, &vector_fill_id](InkpodCore* core) {
            static constexpr std::array<std::uint8_t, 6U> name{
                'V', 'e', 'c', 't', 'o', 'r'};
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_CREATE_LAYER;
            edit.kind = INKPOD_LAYER_VECTOR_COLORING;
            edit.name_utf8 = name.data();
            edit.name_bytes = name.size();
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t layer_id{};
            InkpodStatus status = inkpod_core_tree_edit(
                core, &edit, &result, &layer_id);
            InkpodNodeInfo trace{};
            trace.struct_size = sizeof(trace);
            InkpodNodeInfo fill{};
            fill.struct_size = sizeof(fill);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_node_get(core, 1U, 1U, &trace);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_node_get(core, 1U, 2U, &fill);
            }
            if (status != INKPOD_STATUS_OK || layer_id == 0U
                || trace.kind != INKPOD_TYPED_PLANE_COLOR_TRACE
                || fill.kind != INKPOD_TYPED_PLANE_VECTOR_FILL) {
                return status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_STATE
                    : status;
            }
            constexpr auto point = [](float x, float y) noexcept {
                return InkpodVectorPoint{x, y};
            };
            constexpr auto line = [](InkpodVectorPoint start, InkpodVectorPoint end) noexcept {
                return InkpodVectorCubicSegment{
                    sizeof(InkpodVectorCubicSegment),
                    0U,
                    start,
                    InkpodVectorPoint{
                        (start.x * 2.0F + end.x) / 3.0F,
                        (start.y * 2.0F + end.y) / 3.0F},
                    InkpodVectorPoint{
                        (start.x + end.x * 2.0F) / 3.0F,
                        (start.y + end.y * 2.0F) / 3.0F},
                    end,
                    1.0F,
                    5.0F};
            };
            constexpr std::array<InkpodVectorPoint, 5U> corners{
                point(8.0F, 8.0F),
                point(56.0F, 8.0F),
                point(56.0F, 56.0F),
                point(8.0F, 56.0F),
                point(8.0F, 8.0F)};
            const std::array<InkpodVectorCubicSegment, 4U> segments{
                line(corners[0], corners[1]),
                line(corners[1], corners[2]),
                line(corners[2], corners[3]),
                line(corners[3], corners[4])};
            const InkpodVectorPathInput path{
                sizeof(InkpodVectorPathInput),
                0U,
                INKPOD_VECTOR_PATH_CLOSED,
                trace.id,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    20U,
                    40U,
                    220U,
                    255U},
                segments.data(),
                segments.size(),
                sizeof(InkpodVectorCubicSegment)};
            status = inkpod_core_vector_add_path(
                core, &path, &result, &vector_path_id);
            const InkpodVectorFillInput topology{
                sizeof(InkpodVectorFillInput),
                0U,
                0U,
                fill.id,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    240U,
                    120U,
                    20U,
                    180U},
                &vector_path_id,
                1U};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_vector_add_fill(
                    core, &topology, &result, &vector_fill_id);
            }
            const InkpodSnapshotOptions snapshot_options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_build_snapshot(
                    core, &snapshot_options, &snapshot);
            }
            InkpodSnapshotVectorView vectors{};
            vectors.struct_size = sizeof(vectors);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_vectors(snapshot, &vectors);
            }
            if (status == INKPOD_STATUS_OK
                && (vectors.segment_count != 4U || vectors.fill_count != 1U
                    || vectors.boundary_path_count != 1U || vectors.segments == nullptr
                    || vectors.fills == nullptr || vectors.boundary_path_ids == nullptr
                    || vectors.segments->path_id != vector_path_id
                    || vectors.fills->fill_id != vector_fill_id
                    || *vectors.boundary_path_ids != vector_path_id)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status == INKPOD_STATUS_OK) {
                geometry_before = *vectors.segments;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        true,
        true);
    if (vector_status != INKPOD_STATUS_OK || vector_path_id == 0U || vector_fill_id == 0U) {
        return 502;
    }
    if (ApplyView(state, INKPOD_VIEW_ZOOM_AT, 2.0, 32.0, 32.0)
        != INKPOD_STATUS_OK) {
        return 503;
    }
    bool geometry_unchanged{};
    const InkpodStatus zoom_status = state.engine->Invoke(
        [&geometry_before, &geometry_unchanged](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
            InkpodSnapshotVectorView vectors{};
            vectors.struct_size = sizeof(vectors);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_vectors(snapshot, &vectors);
            }
            if (status == INKPOD_STATUS_OK && vectors.segment_count != 0U
                && vectors.segments != nullptr) {
                const InkpodSnapshotVectorSegment& after = *vectors.segments;
                geometry_unchanged = after.path_id == geometry_before.path_id
                    && after.p0.x == geometry_before.p0.x
                    && after.p0.y == geometry_before.p0.y
                    && after.p1.x == geometry_before.p1.x
                    && after.p1.y == geometry_before.p1.y
                    && after.p2.x == geometry_before.p2.x
                    && after.p2.y == geometry_before.p2.y
                    && after.p3.x == geometry_before.p3.x
                    && after.p3.y == geometry_before.p3.y
                    && after.width_start == geometry_before.width_start
                    && after.width_end == geometry_before.width_end;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (zoom_status != INKPOD_STATUS_OK || !geometry_unchanged) {
        return 504;
    }
    return SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) == 1
        ? 0
        : 505;
}

int RunM6Smoke(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return 600;
    }
    bool preview_cancelled{};
    bool undo_restored{};
    bool adjustment_preserved_source{};
    bool effect_connected{};
    const InkpodStatus status = state.engine->Invoke(
        [&preview_cancelled, &undo_restored, &adjustment_preserved_source, &effect_connected](
            InkpodCore* core) {
            InkpodDocumentInfo document = EmptyDocumentInfo();
            InkpodStatus current = inkpod_core_get_document_info(core, &document);
            const std::array<InkpodStrokeSample, 1U> samples{
                InkpodStrokeSample{
                    sizeof(InkpodStrokeSample), 0U, 32.0F, 32.0F, 1.0F, 0U}};
            InkpodStrokeInput stroke{
                sizeof(InkpodStrokeInput),
                INKPOD_TOOL_PENCIL,
                INKPOD_PLANE_COLOR,
                INKPOD_COORDINATE_SPACE_DOCUMENT,
                0U,
                UINT32_C(0x204080ff),
                3.0F,
                samples.data(),
                samples.size(),
                sizeof(InkpodStrokeSample)};
            InkpodDispatchResult dispatch{};
            dispatch.struct_size = sizeof(dispatch);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_apply_stroke(core, &stroke, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
            }
            const std::uint64_t original = document.color_plane_checksum;
            InkpodFilterInput filter{};
            filter.struct_size = sizeof(filter);
            filter.kind = INKPOD_FILTER_INVERT;
            filter.plane_id = document.color_plane_id;
            filter.channel = INKPOD_FILTER_CHANNEL_RGB;
            InkpodFilterPreviewInfo preview{};
            preview.struct_size = sizeof(preview);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_begin(core, &filter, &preview);
            }
            if (current == INKPOD_STATUS_OK
                && (preview.base_checksum != original
                    || preview.preview_checksum == original)) {
                current = INKPOD_STATUS_INVALID_STATE;
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_cancel(core, &preview);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                preview_cancelled = document.color_plane_checksum == original
                    && preview.preview_checksum == original;
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_begin(core, &filter, &preview);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_apply(core, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_undo(core, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                undo_restored = document.color_plane_checksum == original;
            }
            filter.kind = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
            filter.parameter_0 = 100;
            filter.parameter_1 = 200;
            static constexpr std::array<std::uint8_t, 13U> name{
                'M', '6', ' ', 'A', 'd', 'j', 'u', 's', 't', 'm', 'e', 'n', 't'};
            std::uint64_t adjustment_id{};
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_adjustment_create(
                    core,
                    &filter,
                    name.data(),
                    name.size(),
                    &dispatch,
                    &adjustment_id);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                adjustment_preserved_source = adjustment_id != 0U
                    && document.color_plane_checksum == original;
            }
            const auto color16 = [](std::uint16_t red,
                                    std::uint16_t green,
                                    std::uint16_t blue,
                                    std::uint16_t alpha) noexcept {
                InkpodColorValue color{};
                color.struct_size = sizeof(color);
                color.depth = INKPOD_COLOR_DEPTH_16;
                color.red = red;
                color.green = green;
                color.blue = blue;
                color.alpha = alpha;
                return color;
            };
            std::array<InkpodGradientStop, 3U> stops{};
            for (auto& stop : stops) {
                stop.struct_size = sizeof(stop);
            }
            stops[0].position_milli = 0U;
            stops[0].color = color16(65535U, 0U, 0U, 65535U);
            stops[1].position_milli = 500U;
            stops[1].color = color16(0U, 65535U, 0U, 32768U);
            stops[2].position_milli = 1000U;
            stops[2].color = color16(0U, 0U, 65535U, 65535U);
            InkpodGradientInput gradient{};
            gradient.struct_size = sizeof(gradient);
            gradient.kind = INKPOD_GRADIENT_LINEAR;
            gradient.plane_id = document.color_plane_id;
            gradient.mode = INKPOD_GRADIENT_OVERWRITE;
            gradient.start_x_milli = 500;
            gradient.start_y_milli = 500;
            gradient.end_x_milli = 63500;
            gradient.end_y_milli = 500;
            gradient.stops = stops.data();
            gradient.stop_count = stops.size();
            gradient.stop_stride_bytes = sizeof(InkpodGradientStop);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_effect_gradient(core, &gradient, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                effect_connected = document.color_plane_checksum != original;
            }
            return current;
        },
        true,
        true);
    if (status != INKPOD_STATUS_OK || !preview_cancelled || !undo_restored
        || !adjustment_preserved_source || !effect_connected) {
        return 601;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_COLOR, 0);
    InkpodDocumentInfo before_menu = EmptyDocumentInfo();
    InkpodDocumentInfo after_menu = EmptyDocumentInfo();
    HMENU menu = GetMenu(state.window);
    if (menu == nullptr || !QueryDocument(state, before_menu)
        || GetMenuState(menu, IDM_FILTER_LAST, MF_BYCOMMAND) == static_cast<UINT>(-1)) {
        return 602;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_FILTER_LAST, 0);
    if (!QueryDocument(state, after_menu)
        || after_menu.color_plane_checksum == before_menu.color_plane_checksum) {
        return 603;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_CREATE, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_CREATE, 0);
    if (state.m6_adjustments.size() != 2U
        || state.m6_adjustments[0].id == state.m6_adjustments[1].id) {
        return 605;
    }
    const std::uint64_t newest_adjustment = state.m6_adjustment_id;
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_PREVIOUS, 0);
    if (state.m6_adjustment_id == newest_adjustment) {
        return 606;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_EDIT, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_TOGGLE, 0);
    if (state.m6_adjustment_visible) {
        return 607;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_MOVE_TOP, 0);
    InkpodDocumentInfo before_spray = EmptyDocumentInfo();
    InkpodDocumentInfo after_spray = EmptyDocumentInfo();
    inkpod::renderer::CanvasDocumentBounds spray_bounds{};
    if (!QueryDocument(state, before_spray)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&spray_bounds)) != 1) {
        return 608;
    }
    state.tool = kInteractionM6Airbrush;
    state.m6_options.parameters = {4000, 1000, 1000, 750, 0};
    state.m6_options.option = true;
    state.m6_options.option2 = true;
    const InkpodStrokeSample spray_sample{
        sizeof(InkpodStrokeSample),
        0U,
        static_cast<float>((spray_bounds.left + spray_bounds.right) / 2.0),
        static_cast<float>((spray_bounds.top + spray_bounds.bottom) / 2.0),
        1.0F,
        0U};
    const inkpod::renderer::CanvasStrokeEvent spray_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &spray_sample, 1U};
    const inkpod::renderer::CanvasStrokeEvent spray_end{
        inkpod::renderer::CanvasStrokeEventKind::End, &spray_sample, 1U};
    if (SendMessageW(
            state.window,
            inkpod::renderer::kCanvasStrokeReady,
            0,
            reinterpret_cast<LPARAM>(&spray_begin)) != 1
        || SendMessageW(state.window, WM_TIMER, kM6ContinuousSprayTimer, 0) != 0
        || state.m6_samples.size() < 2U
        || SendMessageW(
               state.window,
               inkpod::renderer::kCanvasStrokeReady,
               0,
               reinterpret_cast<LPARAM>(&spray_end)) != 1
        || state.engine->Invoke(
               [](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false)
            != INKPOD_STATUS_OK
        || !QueryDocument(state, after_spray)
        || after_spray.color_plane_checksum == before_spray.color_plane_checksum) {
        return 609;
    }
    return SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) == 1
        ? 0
        : 604;
}

InkpodStatus InitializeCore(AppState& state) noexcept {
    try {
        state.engine = std::make_unique<inkpod::app::CoreEngine>();
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Start(
        inkpod::renderer::GetCanvasSnapshotSink(state.canvas), state.window);
    return status;
}

InkpodStatus ShutdownCore(AppState& state) noexcept {
    const InkpodStatus clipboard_status =
        inkpod_clipboard_release(&state.clipboard);
    if (state.m6_task != nullptr) {
        inkpod_m6_task_cancel(state.m6_task);
    }
    if (state.engine != nullptr) {
        state.engine->Stop();
        state.engine.reset();
    }
    if (state.m6_progress != nullptr) {
        DestroyWindow(state.m6_progress);
        state.m6_progress = nullptr;
    }
    const InkpodStatus task_status = inkpod_m6_task_release(&state.m6_task);
    return clipboard_status == INKPOD_STATUS_OK ? task_status : clipboard_status;
}

LRESULT CALLBACK MainWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<AppState*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));

    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
        state = static_cast<AppState*>(create->lpCreateParams);
        state->window = window;
        SetWindowLongPtrW(
            window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
    }

    switch (message) {
        case WM_CREATE:
            if (state == nullptr) {
                return -1;
            }
            state->canvas = inkpod::renderer::CreateCanvasWindow(
                state->instance, window);
            if (state->canvas == nullptr
                || SetTimer(
                       window,
                       kAutosaveTimer,
                       kAutosaveIntervalMilliseconds,
                       nullptr) == 0U) {
                return -1;
            }
            return 0;
        case WM_SIZE:
            if (state != nullptr && state->canvas != nullptr) {
                MoveWindow(
                    state->canvas,
                    0,
                    0,
                    LOWORD(lparam),
                    HIWORD(lparam),
                    TRUE);
            }
            return 0;
        case WM_DPICHANGED: {
            const auto* bounds = reinterpret_cast<const RECT*>(lparam);
            SetWindowPos(
                window,
                nullptr,
                bounds->left,
                bounds->top,
                bounds->right - bounds->left,
                bounds->bottom - bounds->top,
                SWP_NOACTIVATE | SWP_NOZORDER);
            return 0;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            switch (LOWORD(wparam)) {
                case IDM_FILE_NEW:
                    if (ConfirmDiscard(*state)) {
                        const InkpodStatus status = CreateDefaultCell(*state);
                        if (status != INKPOD_STATUS_OK) {
                            ShowCoreError(*state, window, L"新規セルの作成");
                        }
                        UpdateMenuState(*state);
                    }
                    return 0;
                case IDM_FILE_OPEN:
                    if (ConfirmDiscard(*state)) {
                        std::wstring path;
                        if (ChooseInkpodPath(window, false, path)) {
                            std::wstring recovery;
                            try {
                                recovery = path + L".recovery.inkpod";
                            } catch (const std::bad_alloc&) {
                                ShowCoreError(*state, window, L"Recovery path の作成");
                                return 0;
                            }
                            InkpodStatus status = INKPOD_STATUS_OK;
                            if (RecoveryIsNewer(path, recovery)) {
                                const int choice = MessageBoxW(
                                    window,
                                    L"通常保存より新しいRecoveryがあります。\n\n"
                                    L"はい: Recoveryを開く\nいいえ: Recoveryを破棄\n"
                                    L"キャンセル: 後で判断して通常保存を開く",
                                    L"inkpod Recovery",
                                    MB_YESNOCANCEL | MB_ICONQUESTION);
                                if (choice == IDYES) {
                                    status = OpenRecoveryFromPath(*state, recovery);
                                } else {
                                    if (choice == IDNO) {
                                        DeleteFileW(recovery.c_str());
                                    }
                                    status = OpenFromPath(*state, path);
                                }
                            } else {
                                status = OpenFromPath(*state, path);
                            }
                            if (status != INKPOD_STATUS_OK) {
                                ShowCoreError(*state, window, L"開く");
                            }
                        }
                    }
                    return 0;
                case IDM_FILE_SAVE:
                case IDM_FILE_SAVE_AS: {
                    const InkpodStatus status = SaveDocument(
                        *state, LOWORD(wparam) == IDM_FILE_SAVE_AS);
                    if (status != INKPOD_STATUS_OK
                        && status != INKPOD_STATUS_INVALID_STATE) {
                        ShowCoreError(*state, window, L"保存");
                    }
                    return 0;
                }
                case IDM_FILE_REVERT: {
                    const InkpodStatus revert_status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  InkpodDocumentInfo info = EmptyDocumentInfo();
                                  return inkpod_core_revert(core, &info);
                              },
                              false,
                              false);
                    if (revert_status != INKPOD_STATUS_OK
                        || FitCanvas(*state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"復帰");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_FILE_AUTOSAVE_NOW: {
                    std::wstring path = state->recovery_path;
                    if (path.empty() && !ChooseInkpodPath(window, true, path)) {
                        return 0;
                    }
                    if (!QueueAutosave(*state, path)) {
                        ShowCoreError(*state, window, L"Recovery保存の予約");
                    } else {
                        try {
                            state->recovery_path = path;
                        } catch (const std::bad_alloc&) {
                            ShowCoreError(*state, window, L"Recovery path の保持");
                        }
                    }
                    return 0;
                }
                case IDM_FILE_OPEN_RECOVERY: {
                    if (ConfirmDiscard(*state)) {
                        std::wstring path = state->recovery_path;
                        if (ChooseInkpodPath(window, false, path)
                            && OpenRecoveryFromPath(*state, path) != INKPOD_STATUS_OK) {
                            ShowCoreError(*state, window, L"Recoveryを開く");
                        }
                    }
                    return 0;
                }
                case IDM_EDIT_UNDO:
                case IDM_EDIT_REDO: {
                    const bool redo = LOWORD(wparam) == IDM_EDIT_REDO;
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [redo](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return redo ? inkpod_core_redo(core, &result)
                                              : inkpod_core_undo(core, &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"履歴操作");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_EDIT_COPY: {
                    InkpodClipboard* replacement{};
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [&replacement](InkpodCore* core) {
                                  return inkpod_core_clipboard_copy(
                                      core, &replacement);
                              },
                              false,
                              false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"コピー");
                    } else {
                        inkpod_clipboard_release(&state->clipboard);
                        state->clipboard = replacement;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_EDIT_PASTE: {
                    const InkpodClipboard* clipboard = state->clipboard;
                    const InkpodStatus status = clipboard == nullptr
                        || state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [clipboard](InkpodCore* core) {
                                  InkpodStatus inner = inkpod_core_paste_begin(
                                      core, clipboard);
                                  if (inner != INKPOD_STATUS_OK) {
                                      return inner;
                                  }
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  inner = inkpod_core_floating_commit(
                                      core, &result);
                                  if (inner != INKPOD_STATUS_OK) {
                                      inkpod_core_floating_cancel(core);
                                  }
                                  return inner;
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"貼り付け");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_EDIT_MIRROR_HORIZONTAL: {
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_mirror_document(
                                      core,
                                      INKPOD_MIRROR_HORIZONTAL,
                                      &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"画像の左右反転");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_FILTER_LAST: {
                    InkpodDocumentInfo document{};
                    const InkpodStatus status = state->plane != INKPOD_PLANE_COLOR
                            || !QueryDocument(*state, document)
                        ? INKPOD_STATUS_INVALID_STATE
                        : StartM6Task(
                              *state,
                              false,
                              [plane_id = document.color_plane_id](
                                  InkpodCore* core, InkpodM6Task* task) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_filter_apply_last_task(
                                      core, plane_id, task, &result);
                              });
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"直前のフィルタ");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_FILTER_INVERT:
                case IDM_FILTER_BLUR_WEAK:
                case IDM_FILTER_SHARPEN_WEAK:
                case IDM_FILTER_SHARPEN_STRONG:
                case IDM_FILTER_BLUR_STRONG:
                case IDM_FILTER_GAUSSIAN:
                case IDM_FILTER_AUTO_CONTRAST:
                case IDM_FILTER_BRIGHTNESS:
                case IDM_FILTER_TONE_CURVE:
                case IDM_FILTER_LEVELS:
                case IDM_FILTER_HSV:
                case IDM_FILTER_COLOR_BALANCE:
                case IDM_FILTER_UNSHARP: {
                    const UINT command = LOWORD(wparam);
                    InkpodDocumentInfo document{};
                    M6FilterJob job{};
                    if (state->plane != INKPOD_PLANE_COLOR
                        || !QueryDocument(*state, document)
                        || !ConfigureM6FilterEditor(*state, command, job)) {
                        return 0;
                    }
                    job.plane_id = document.color_plane_id;
                    const InkpodStatus status = QueueM6Filter(*state, std::move(job));
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"フィルタ");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_ADJUSTMENT_CREATE:
                case IDM_ADJUSTMENT_EDIT: {
                    const bool update = LOWORD(wparam) == IDM_ADJUSTMENT_EDIT;
                    M6FilterJob job{};
                    if (!ConfigureM6AdjustmentEditor(*state, job, update)) {
                        return 0;
                    }
                    const InkpodStatus status = CreateOrUpdateM6Adjustment(
                        *state, std::move(job), update);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"調整レイヤー");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_ADJUSTMENT_PREVIOUS:
                case IDM_ADJUSTMENT_NEXT:
                    SelectM6Adjustment(
                        *state, LOWORD(wparam) == IDM_ADJUSTMENT_NEXT);
                    UpdateMenuState(*state);
                    return 0;
                case IDM_ADJUSTMENT_TOGGLE: {
                    const bool visible = !state->m6_adjustment_visible;
                    const InkpodStatus status = SetM6AdjustmentVisibility(*state, visible);
                    if (status == INKPOD_STATUS_OK) {
                        state->m6_adjustment_visible = visible;
                    } else {
                        ShowCoreError(*state, window, L"調整レイヤー表示");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_ADJUSTMENT_MOVE_TOP: {
                    std::uint64_t ignored{};
                    const InkpodStatus status = ApplyTreeEdit(
                        *state,
                        INKPOD_TREE_REORDER_LAYER,
                        state->m6_adjustment_id,
                        0U,
                        ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"調整レイヤー移動");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_LAYER_DUPLICATE: {
                    InkpodDocumentInfo info{};
                    std::uint64_t duplicate_id{};
                    const InkpodStatus status = QueryDocument(*state, info)
                        ? ApplyTreeEdit(
                              *state,
                              INKPOD_TREE_DUPLICATE_LAYER,
                              info.layer_id,
                              0U,
                              duplicate_id)
                        : INKPOD_STATUS_INVALID_STATE;
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤーの複製");
                    } else {
                        state->m3_layer_id = duplicate_id;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_LAYER_DELETE: {
                    std::uint64_t ignored{};
                    const InkpodStatus status = state->m3_layer_id == 0U
                        ? INKPOD_STATUS_INVALID_STATE
                        : ApplyTreeEdit(
                              *state,
                              INKPOD_TREE_DELETE_LAYER,
                              state->m3_layer_id,
                              0U,
                              ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤーの削除");
                    } else {
                        state->m3_layer_id = 0U;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_LAYER_MOVE_TOP: {
                    InkpodDocumentInfo info{};
                    const std::uint64_t target = state->m3_layer_id != 0U
                        ? state->m3_layer_id
                        : (QueryDocument(*state, info) ? info.layer_id : 0U);
                    std::uint64_t ignored{};
                    const InkpodStatus status = target == 0U
                        ? INKPOD_STATUS_INVALID_STATE
                        : ApplyTreeEdit(
                              *state,
                              INKPOD_TREE_REORDER_LAYER,
                              target,
                              0U,
                              ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤーの移動");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SELECTION_ALL: {
                    InkpodDocumentInfo info{};
                    InkpodSelectionInput input{};
                    input.struct_size = sizeof(input);
                    input.shape = INKPOD_SELECTION_RECTANGLE;
                    input.operation = INKPOD_SELECTION_NEW;
                    const bool queried = QueryDocument(*state, info);
                    if (queried) {
                        input.bounds = {
                            0,
                            0,
                            static_cast<std::int32_t>(info.width),
                            static_cast<std::int32_t>(info.height)};
                    }
                    const InkpodStatus status = !queried || state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [input](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_apply_selection(
                                      core, &input, &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"すべて選択");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SELECTION_INVERT:
                case IDM_SELECTION_EXPAND:
                case IDM_SELECTION_SHRINK: {
                    const std::uint32_t operation =
                        LOWORD(wparam) == IDM_SELECTION_INVERT
                        ? INKPOD_SELECTION_ADJUST_INVERT
                        : (LOWORD(wparam) == IDM_SELECTION_EXPAND
                                  ? INKPOD_SELECTION_ADJUST_EXPAND
                                  : INKPOD_SELECTION_ADJUST_SHRINK);
                    const InkpodStatus status = AdjustSelection(
                        *state,
                        operation,
                        operation == INKPOD_SELECTION_ADJUST_INVERT ? 0U : 1U);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"選択範囲の変更");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_ZOOM_IN:
                case IDM_VIEW_ZOOM_OUT: {
                    RECT client{};
                    GetClientRect(state->canvas, &client);
                    const double factor = LOWORD(wparam) == IDM_VIEW_ZOOM_IN ? 1.2 : 1.0 / 1.2;
                    if (ApplyView(
                            *state,
                            INKPOD_VIEW_ZOOM_AT,
                            factor,
                            static_cast<double>(client.right) / 2.0,
                            static_cast<double>(client.bottom) / 2.0)
                        != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"表示倍率の変更");
                    }
                    return 0;
                }
                case IDM_VIEW_FIT:
                case IDM_VIEW_ONE_TO_ONE:
                    if (FitCanvas(
                            *state,
                            LOWORD(wparam) == IDM_VIEW_FIT ? INKPOD_VIEW_FIT
                                                          : INKPOD_VIEW_ONE_TO_ONE)
                        != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"表示の変更");
                    }
                    return 0;
                case IDM_VIEW_FLIP_HORIZONTAL:
                case IDM_VIEW_FLIP_VERTICAL: {
                    const bool horizontal =
                        LOWORD(wparam) == IDM_VIEW_FLIP_HORIZONTAL;
                    const InkpodStatus status = ApplyView(
                        *state,
                        horizontal ? INKPOD_VIEW_FLIP_HORIZONTAL
                                   : INKPOD_VIEW_FLIP_VERTICAL,
                        0.0,
                        0.0);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"表示反転");
                    } else if (horizontal) {
                        state->view_flip_horizontal =
                            !state->view_flip_horizontal;
                    } else {
                        state->view_flip_vertical = !state->view_flip_vertical;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_GRID: {
                    const bool visible = !state->grid_visible;
                    const InkpodStatus status = ApplyView(
                        *state,
                        INKPOD_VIEW_SET_GRID_VISIBLE,
                        visible ? 1.0 : 0.0,
                        0.0);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"グリッド表示");
                    } else {
                        state->grid_visible = visible;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_NEW: {
                    std::uint64_t view_id{};
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [&view_id](InkpodCore* core) {
                                  return inkpod_core_view_create(
                                      core, &view_id);
                              },
                              false,
                              false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ビューの作成");
                    } else {
                        state->secondary_view_id = view_id;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_TOOL_PENCIL:
                    state->tool = INKPOD_TOOL_PENCIL;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_TOOL_BRUSH:
                    state->tool = INKPOD_TOOL_BRUSH;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_TOOL_ERASER:
                    state->tool = INKPOD_TOOL_ERASER;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_TOOL_FILL:
                    state->tool = kInteractionFill;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_TOOL_EYEDROPPER:
                    state->tool = kInteractionEyedropper;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_EFFECT_GRADIENT:
                case IDM_EFFECT_AIRBRUSH:
                case IDM_EFFECT_BOUNDARY_AIRBRUSH:
                case IDM_EFFECT_BLUR:
                case IDM_EFFECT_STAMP:
                case IDM_EFFECT_DUST:
                case IDM_EFFECT_ALPHA_GRADIENT:
                    if (state->plane == INKPOD_PLANE_COLOR
                        && ConfigureM6Effect(*state, LOWORD(wparam))) {
                        UpdateMenuState(*state);
                    }
                    return 0;
                case IDM_EFFECT_ALPHA_VIEW: {
                    const bool enabled = !state->alpha_view;
                    const InkpodStatus status = ApplyView(
                        *state,
                        INKPOD_VIEW_SET_ALPHA_VISIBLE,
                        enabled ? 1.0 : 0.0,
                        0.0);
                    if (status == INKPOD_STATUS_OK) {
                        state->alpha_view = enabled;
                    } else {
                        ShowCoreError(*state, window, L"アルファ表示");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_PLANE_MAIN_LINE:
                case IDM_PLANE_COLOR: {
                    state->plane = LOWORD(wparam) == IDM_PLANE_MAIN_LINE
                        ? INKPOD_PLANE_MAIN_LINE
                        : INKPOD_PLANE_COLOR;
                    const InkpodPlaneKind plane = state->plane;
                    const InkpodStatus plane_status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [plane](InkpodCore* core) {
                                  return inkpod_core_set_active_plane(core, plane);
                              },
                              false,
                              true);
                    if (plane_status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"プレーン切替");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_COLOR_CHOOSE: {
                    static std::array<COLORREF, 16> custom_colors{};
                    CHOOSECOLORW choose{};
                    choose.lStructSize = sizeof(choose);
                    choose.hwndOwner = window;
                    choose.rgbResult = RGB(
                        (state->color_rgba >> 24) & 0xffU,
                        (state->color_rgba >> 16) & 0xffU,
                        (state->color_rgba >> 8) & 0xffU);
                    choose.lpCustColors = custom_colors.data();
                    choose.Flags = CC_FULLOPEN | CC_RGBINIT;
                    if (ChooseColorW(&choose) != FALSE) {
                        state->color_rgba = (GetRValue(choose.rgbResult) << 24)
                            | (GetGValue(choose.rgbResult) << 16)
                            | (GetBValue(choose.rgbResult) << 8) | 0xffU;
                    }
                    return 0;
                }
                case IDM_COLOR_CHECK_OFF:
                case IDM_COLOR_CHECK_LEGACY:
                case IDM_COLOR_CHECK_NATIVE: {
                    const InkpodColorCheckMode mode = LOWORD(wparam) == IDM_COLOR_CHECK_LEGACY
                        ? INKPOD_COLOR_CHECK_LEGACY_WHITE
                        : (LOWORD(wparam) == IDM_COLOR_CHECK_NATIVE
                                  ? INKPOD_COLOR_CHECK_NATIVE_ALPHA
                                  : INKPOD_COLOR_CHECK_OFF);
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [mode](InkpodCore* core) {
                                  return inkpod_core_set_color_check(core, mode);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"彩色チェック表示");
                    } else {
                        state->color_check_mode = mode;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SHORTCUT_RESET: {
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  return inkpod_core_shortcut_reset(core);
                              },
                              false,
                              false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ショートカットの初期化");
                    }
                    return 0;
                }
                case IDM_SHORTCUT_EDIT: {
                    ShortcutDialogState dialog_state{};
                    if (ShowShortcutEditor(
                            state->instance,
                            window,
                            state->smoke_test,
                            dialog_state) != IDOK) {
                        return 0;
                    }
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [dialog_state](InkpodCore* core) {
                                  return inkpod_core_shortcut_rebind(
                                      core,
                                      dialog_state.command_id,
                                      dialog_state.virtual_key,
                                      dialog_state.modifiers);
                              },
                              false,
                              false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ショートカット編集");
                        return 0;
                    }
                    return 1;
                }
                case IDM_HELP_ABOUT:
                    return ShowAboutDialog(
                               state->instance,
                               window,
                               state->smoke_test)
                            == IDOK
                        ? 1
                        : 0;
                case IDM_APP_EXIT:
                    SendMessageW(window, WM_CLOSE, 0, 0);
                    return 0;
                default:
                    break;
            }
            break;
        case WM_KEYDOWN:
        case WM_SYSKEYDOWN:
            if (state != nullptr) {
                const std::uint32_t modifiers = CurrentShortcutModifiers(lparam);
                UINT menu_command{};
                if (ResolveConfiguredShortcut(
                        *state,
                        static_cast<std::uint32_t>(wparam),
                        modifiers,
                        menu_command)) {
                    SendMessageW(window, WM_COMMAND, menu_command, 0);
                    return 0;
                }
                if (modifiers == INKPOD_SHORTCUT_MODIFIER_CONTROL && wparam == 'S') {
                    SendMessageW(window, WM_COMMAND, IDM_FILE_SAVE, 0);
                    return 0;
                }
                if (modifiers == INKPOD_SHORTCUT_MODIFIER_CONTROL && wparam == 'N') {
                    SendMessageW(window, WM_COMMAND, IDM_FILE_NEW, 0);
                    return 0;
                }
                if (modifiers == INKPOD_SHORTCUT_MODIFIER_CONTROL && wparam == 'O') {
                    SendMessageW(window, WM_COMMAND, IDM_FILE_OPEN, 0);
                    return 0;
                }
            }
            break;
        case inkpod::renderer::kCanvasStrokeReady:
            if (state != nullptr) {
                const auto* input = reinterpret_cast<
                    const inkpod::renderer::CanvasStrokeEvent*>(lparam);
                if (input == nullptr || state->engine == nullptr
                    || input->sample_count > UINT64_C(1048576)
                    || (input->sample_count != 0U && input->samples == nullptr)) {
                    return 0;
                }
                if ((state->tool == kInteractionFill
                        || state->tool == kInteractionEyedropper)
                    && input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                    && input->sample_count != 0U) {
                    const InkpodStatus status = state->tool == kInteractionFill
                        ? ApplyFillAtDevicePoint(
                              *state, input->samples[0].x, input->samples[0].y)
                        : EyedropAtDevicePoint(
                              *state, input->samples[0].x, input->samples[0].y);
                    if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                        ShowCoreError(
                            *state,
                            window,
                            state->tool == kInteractionFill ? L"フィル" : L"スポイト");
                    }
                    UpdateMenuState(*state);
                    return 1;
                }
                if (state->tool == kInteractionFill
                    || state->tool == kInteractionEyedropper) {
                    return 1;
                }
                const bool m6_interaction = state->tool >= kInteractionM6Gradient
                    && state->tool <= kInteractionM6AlphaGradient;
                if (m6_interaction) {
                    if (state->tool == kInteractionM6Stamp
                        && input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                        && input->sample_count != 0U
                        && (GetKeyState(VK_MENU) & 0x8000) != 0) {
                        state->stamp_source = input->samples[0];
                        state->stamp_source_valid = true;
                        state->m6_samples.clear();
                        return 1;
                    }
                    if (state->m6_task != nullptr) {
                        return 1;
                    }
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->m6_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            if (state->m6_samples.size()
                                > UINT64_C(1048576) - input->sample_count) {
                                state->m6_samples.clear();
                                state->m6_airbrush_active = false;
                                KillTimer(window, kM6ContinuousSprayTimer);
                                return 0;
                            }
                            state->m6_samples.insert(
                                state->m6_samples.end(),
                                input->samples,
                                input->samples + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        state->m6_samples.clear();
                        state->m6_airbrush_active = false;
                        KillTimer(window, kM6ContinuousSprayTimer);
                        return 0;
                    }
                    if (state->tool == kInteractionM6Airbrush
                        && input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                        && input->sample_count != 0U) {
                        state->m6_airbrush_last =
                            input->samples[static_cast<std::size_t>(input->sample_count - 1U)];
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->m6_airbrush_active = true;
                            SetTimer(
                                window,
                                kM6ContinuousSprayTimer,
                                kM6ContinuousSprayIntervalMilliseconds,
                                nullptr);
                        }
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->m6_samples.clear();
                        state->m6_airbrush_active = false;
                        KillTimer(window, kM6ContinuousSprayTimer);
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        state->m6_airbrush_active = false;
                        KillTimer(window, kM6ContinuousSprayTimer);
                        const InkpodStatus status = FinishM6CanvasGesture(*state);
                        if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                            ShowCoreError(*state, window, L"M6 Canvas効果");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                inkpod::app::StrokeEvent event{};
                switch (input->kind) {
                    case inkpod::renderer::CanvasStrokeEventKind::Begin:
                        event.kind = inkpod::app::StrokeEventKind::Begin;
                        break;
                    case inkpod::renderer::CanvasStrokeEventKind::Append:
                        event.kind = inkpod::app::StrokeEventKind::Append;
                        break;
                    case inkpod::renderer::CanvasStrokeEventKind::End:
                        event.kind = inkpod::app::StrokeEventKind::End;
                        break;
                    case inkpod::renderer::CanvasStrokeEventKind::Cancel:
                        event.kind = inkpod::app::StrokeEventKind::Cancel;
                        break;
                }
                event.style = inkpod::app::StrokeStyle{
                    static_cast<InkpodPaintTool>(state->tool),
                    state->plane,
                    INKPOD_COORDINATE_SPACE_DEVICE,
                    state->tool == INKPOD_TOOL_PENCIL ? INKPOD_STROKE_FLAG_AUTO_ERASE
                                                      : INKPOD_STROKE_FLAG_PRESSURE_SIZE,
                    state->color_rgba,
                    state->tool == INKPOD_TOOL_PENCIL ? 1.0F : state->diameter};
                try {
                    if (input->sample_count != 0U) {
                        event.samples.assign(
                            input->samples,
                            input->samples + static_cast<std::size_t>(input->sample_count));
                    }
                } catch (const std::bad_alloc&) {
                    return 0;
                }
                return state->engine->EnqueueStroke(std::move(event)) ? 1 : 0;
            }
            return 0;
        case inkpod::renderer::kCanvasViewGesture:
            if (state != nullptr) {
                const auto* gesture = reinterpret_cast<
                    const inkpod::renderer::CanvasViewGesture*>(lparam);
                if (gesture != nullptr
                    && ApplyView(
                           *state,
                           gesture->kind,
                           gesture->value1,
                           gesture->value2,
                           gesture->value3) == INKPOD_STATUS_OK) {
                    return 1;
                }
            }
            return 0;
        case inkpod::renderer::kCanvasViewportChanged:
            if (state != nullptr && state->engine != nullptr && wparam != 0U && lparam != 0) {
                ApplyView(
                    *state,
                    INKPOD_VIEW_VIEWPORT_RESIZED,
                    static_cast<double>(wparam),
                    static_cast<double>(lparam));
            }
            return 0;
        case inkpod::app::kCoreStateChanged:
            if (state != nullptr) {
                UpdateMenuState(*state);
            }
            return 0;
        case kM6TaskCompleted:
            if (state != nullptr) {
                const InkpodStatus status = static_cast<InkpodStatus>(wparam);
                const bool prompt = state->m6_preview_prompt;
                state->m6_preview_prompt = false;
                if (state->m6_progress != nullptr) {
                    DestroyWindow(state->m6_progress);
                    state->m6_progress = nullptr;
                }
                inkpod_m6_task_release(&state->m6_task);
                if (status == INKPOD_STATUS_OK && prompt && state->engine != nullptr) {
                    const int choice = MessageBoxW(
                        window,
                        L"Canvasのプレビューを適用しますか？\nキャンセルすると元の状態へ完全に戻ります。",
                        L"M6 プレビュー",
                        MB_OKCANCEL | MB_ICONQUESTION);
                    const InkpodStatus preview_status = state->engine->Invoke(
                        [choice](InkpodCore* core) {
                            if (choice == IDOK) {
                                InkpodDispatchResult result{};
                                result.struct_size = sizeof(result);
                                return inkpod_core_filter_preview_apply(core, &result);
                            }
                            InkpodFilterPreviewInfo info{};
                            info.struct_size = sizeof(info);
                            return inkpod_core_filter_preview_cancel(core, &info);
                        },
                        true,
                        true);
                    if (preview_status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"M6プレビューの確定");
                    }
                }
                UpdateMenuState(*state);
            }
            return 0;
        case inkpod::app::kCoreAsyncFailed:
            if (state != nullptr && !state->smoke_test) {
                ShowCoreError(*state, window, L"非同期処理");
            }
            return 0;
        case WM_TIMER:
            if (state != nullptr && wparam == kM6ContinuousSprayTimer) {
                if (state->m6_airbrush_active && state->tool == kInteractionM6Airbrush
                    && state->m6_task == nullptr) {
                    try {
                        if (state->m6_samples.size() < UINT64_C(1048576)) {
                            state->m6_samples.push_back(state->m6_airbrush_last);
                        } else {
                            state->m6_airbrush_active = false;
                            KillTimer(window, kM6ContinuousSprayTimer);
                        }
                    } catch (const std::bad_alloc&) {
                        state->m6_airbrush_active = false;
                        KillTimer(window, kM6ContinuousSprayTimer);
                    }
                }
                return 0;
            }
            if (state != nullptr && wparam == kAutosaveTimer && !state->recovery_path.empty()) {
                InkpodDocumentInfo info{};
                if (QueryDocument(*state, info)
                    && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
                    QueueAutosave(*state, state->recovery_path);
                }
                return 0;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr && !state->smoke_test && !ConfirmDiscard(*state)) {
                return 0;
            }
            if (state != nullptr && state->m6_task != nullptr) {
                inkpod_m6_task_cancel(state->m6_task);
            }
            if (state != nullptr) {
                state->m6_airbrush_active = false;
                KillTimer(window, kM6ContinuousSprayTimer);
            }
            ShowWindow(window, SW_HIDE);
            PostQuitMessage(0);
            return 0;
        case inkpod::renderer::kCanvasRenderFailed:
            if (state == nullptr || !state->smoke_test) {
                MessageBoxW(
                    window,
                    L"Canvas renderer の描画に失敗しました。",
                    L"inkpod",
                    MB_OK | MB_ICONERROR);
            }
            if (state == nullptr || !state->smoke_test) {
                SendMessageW(window, WM_CLOSE, 0, 0);
            }
            return 0;
        case WM_NCDESTROY:
            KillTimer(window, kAutosaveTimer);
            KillTimer(window, kM6ContinuousSprayTimer);
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            return DefWindowProcW(window, message, wparam, lparam);
        default:
            break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

bool RegisterMainWindowClass(
    HINSTANCE instance, const wchar_t* class_name) noexcept {
    const auto app_icon = LoadIconW(instance, MAKEINTRESOURCEW(IDI_APP_ICON));
    const auto small_icon = reinterpret_cast<HICON>(LoadImageW(
        instance,
        MAKEINTRESOURCEW(IDI_APP_ICON),
        IMAGE_ICON,
        GetSystemMetrics(SM_CXSMICON),
        GetSystemMetrics(SM_CYSMICON),
        LR_DEFAULTCOLOR | LR_SHARED));
    if (app_icon == nullptr) {
        return false;
    }
    WNDCLASSEXW window_class{};
    window_class.cbSize = sizeof(window_class);
    window_class.style = CS_HREDRAW | CS_VREDRAW;
    window_class.lpfnWndProc = MainWindowProcedure;
    window_class.hInstance = instance;
    window_class.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    window_class.hIcon = app_icon;
    window_class.hbrBackground = nullptr;
    window_class.lpszMenuName = MAKEINTRESOURCEW(IDR_MAIN_MENU);
    window_class.lpszClassName = class_name;
    window_class.hIconSm = small_icon != nullptr ? small_icon : app_icon;
    return RegisterClassExW(&window_class) != 0;
}

}  // namespace

int APIENTRY wWinMain(
    HINSTANCE instance,
    HINSTANCE,
    wchar_t* command_line,
    int show_command) {
    if (command_line != nullptr
        && std::wcsstr(command_line, L"--abi-smoke-test") != nullptr) {
        return InkpodRunAbiSmoke();
    }
    INITCOMMONCONTROLSEX controls{};
    controls.dwSize = sizeof(controls);
    controls.dwICC = ICC_STANDARD_CLASSES | ICC_BAR_CLASSES;
    if (!InitCommonControlsEx(&controls)) {
        MessageBoxW(
            nullptr,
            L"Common Controls の初期化に失敗しました。",
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 10;
    }

    ComApartment com;
    if (FAILED(com.Initialize())) {
        MessageBoxW(
            nullptr,
            L"COM の初期化に失敗しました。",
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 11;
    }

    std::array<wchar_t, 128> title{};
    std::array<wchar_t, 128> class_name{};
    if (LoadStringW(
            instance,
            IDS_APP_TITLE,
            title.data(),
            static_cast<int>(title.size())) == 0
        || LoadStringW(
               instance,
               IDS_MAIN_WINDOW_CLASS,
               class_name.data(),
               static_cast<int>(class_name.size())) == 0) {
        return 12;
    }
    if (!inkpod::renderer::RegisterCanvasClass(instance)
        || !RegisterMainWindowClass(instance, class_name.data())) {
        return 13;
    }

    AppState state{};
    state.instance = instance;
    state.smoke_test = command_line != nullptr
        && std::wcsstr(command_line, L"--smoke-test") != nullptr;
    HWND window = CreateWindowExW(
        0,
        class_name.data(),
        title.data(),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        1024,
        720,
        nullptr,
        nullptr,
        instance,
        &state);
    if (window == nullptr) {
        return 14;
    }

    InkpodStatus core_status = InitializeCore(state);
    if (core_status != INKPOD_STATUS_OK) {
        if (!state.smoke_test) {
            ShowCoreError(state, window, L"Rust Core の初期化");
        }
        ShutdownCore(state);
        DestroyWindow(window);
        return 15;
    }

    bool document_initialized{};
    if (!state.smoke_test) {
        std::wstring recovery;
        if (NewestPrivateRecovery(recovery)) {
            const int choice = MessageBoxW(
                window,
                L"未処理のRecoveryがあります。\n\n"
                L"はい: Recoveryを開く\nいいえ: Recoveryを破棄\n"
                L"キャンセル: 後で判断して新規セルを開く",
                L"inkpod Recovery",
                MB_YESNOCANCEL | MB_ICONQUESTION);
            if (choice == IDYES) {
                core_status = OpenRecoveryFromPath(state, recovery);
                document_initialized = core_status == INKPOD_STATUS_OK;
                if (!document_initialized) {
                    ShowCoreError(state, window, L"起動時Recoveryを開く");
                    core_status = INKPOD_STATUS_OK;
                }
            } else if (choice == IDNO
                && DeleteFileW(recovery.c_str()) == FALSE
                && GetLastError() != ERROR_FILE_NOT_FOUND) {
                MessageBoxW(
                    window,
                    L"Recoveryを削除できませんでした。ファイルを残して新規セルを開きます。",
                    L"inkpod Recovery",
                    MB_OK | MB_ICONWARNING);
            }
        }
    }
    if (core_status == INKPOD_STATUS_OK && !document_initialized) {
        core_status = CreateDefaultCell(state);
        document_initialized = core_status == INKPOD_STATUS_OK;
    }
    if (core_status != INKPOD_STATUS_OK || !document_initialized) {
        if (!state.smoke_test) {
            ShowCoreError(state, window, L"セルまたはRecoveryの初期化");
        }
        ShutdownCore(state);
        DestroyWindow(window);
        return 16;
    }
    UpdateMenuState(state);

    int exit_code = 0;
    if (state.smoke_test) {
        exit_code = RunM1Smoke(state);
        if (exit_code == 0) {
            exit_code = RunM2Smoke(state);
        }
        if (exit_code == 0) {
            exit_code = RunM3Smoke(state);
        }
        if (exit_code == 0) {
            exit_code = RunM5Smoke(state);
        }
        if (exit_code == 0) {
            exit_code = RunM6Smoke(state);
        }
    } else {
        ShowWindow(window, show_command);
        UpdateWindow(window);
        MSG message{};
        BOOL result{};
        while ((result = GetMessageW(&message, nullptr, 0, 0)) > 0) {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        exit_code = result == -1 ? 17 : static_cast<int>(message.wParam);
    }

    core_status = ShutdownCore(state);
    DestroyWindow(window);
    if (core_status != INKPOD_STATUS_OK && exit_code == 0) {
        return 18;
    }
    return exit_code;
}
