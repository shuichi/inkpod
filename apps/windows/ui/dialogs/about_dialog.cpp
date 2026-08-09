#include "about_dialog.h"

#include <commctrl.h>

#include <array>
#include <cwchar>

#include "app/resource.h"
#include "modal_dialog_position.h"

namespace inkpod::windows::ui {
namespace {

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
    HICON icon{};
    if (FAILED(LoadIconWithScaleDown(
            state.instance,
            MAKEINTRESOURCEW(IDI_APP_ICON),
            icon_size,
            icon_size,
            &icon))
        || icon == nullptr) {
        return false;
    }
    state.display_icon = icon;
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
    std::array<wchar_t, 16> build_number{};
    std::array<wchar_t, 128> expected_version{};
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
               IDS_APP_BUILD_NUMBER,
               build_number.data(),
               static_cast<int>(build_number.size())) == 0
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
    std::array<wchar_t, 128> version_label{};
    _snwprintf_s(
        version_label.data(),
        version_label.size(),
        _TRUNCATE,
        L"Version %ls (Build %ls)",
        version.data(),
        build_number.data());
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
            std::array<wchar_t, 16> build_number{};
            std::array<wchar_t, 128> version_label{};
            std::array<wchar_t, 512> description{};
            std::array<wchar_t, 64> copyright{};
            if (LoadStringW(
                    state->instance,
                    IDS_APP_VERSION,
                    version.data(),
                    static_cast<int>(version.size())) == 0
                || LoadStringW(
                       state->instance,
                       IDS_APP_BUILD_NUMBER,
                       build_number.data(),
                       static_cast<int>(build_number.size())) == 0
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
                L"Version %ls (Build %ls)",
                version.data(),
                build_number.data());
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
            static_cast<void>(CenterModalDialogOnOwner(dialog));
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

}  // namespace

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

}  // namespace inkpod::windows::ui
