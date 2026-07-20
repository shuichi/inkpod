#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
#include <objbase.h>
#include <windowsx.h>

#include <array>
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

#include "canvas.h"
#include "core_engine.h"
#include "inkpod/core_ffi.h"
#include "resource.h"

int InkpodRunAbiSmoke();

namespace {

constexpr std::uint32_t kInteractionFill = 1001U;
constexpr std::uint32_t kInteractionEyedropper = 1002U;
constexpr UINT_PTR kAutosaveTimer = 1U;
constexpr UINT kAutosaveIntervalMilliseconds = 60U * 1000U;

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
    bool smoke_test{};
};

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
    bool close_immediately{};
};

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
            std::array<wchar_t, 256> description{};
            if (LoadStringW(
                    state->instance,
                    IDS_APP_VERSION,
                    version.data(),
                    static_cast<int>(version.size())) == 0
                || LoadStringW(
                       state->instance,
                       IDS_ABOUT_DESCRIPTION,
                       description.data(),
                       static_cast<int>(description.size())) == 0) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            _snwprintf_s(
                version_label.data(),
                version_label.size(),
                _TRUNCATE,
                L"バージョン %ls",
                version.data());
            SetDlgItemTextW(dialog, IDC_ABOUT_VERSION, version_label.data());
            SetDlgItemTextW(dialog, IDC_ABOUT_DESCRIPTION, description.data());

            const HWND icon_control = GetDlgItem(dialog, IDC_ABOUT_ICON);
            RECT icon_bounds{};
            if (icon_control == nullptr
                || GetClientRect(icon_control, &icon_bounds) == FALSE) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            const int icon_width = icon_bounds.right - icon_bounds.left;
            const int icon_height = icon_bounds.bottom - icon_bounds.top;
            const int icon_size = icon_width < icon_height ? icon_width : icon_height;
            state->display_icon = reinterpret_cast<HICON>(LoadImageW(
                state->instance,
                MAKEINTRESOURCEW(IDI_APP_ICON),
                IMAGE_ICON,
                icon_size,
                icon_size,
                LR_DEFAULTCOLOR));
            if (state->display_icon == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SendMessageW(
                icon_control,
                STM_SETIMAGE,
                IMAGE_ICON,
                reinterpret_cast<LPARAM>(state->display_icon));

            const HWND name_control = GetDlgItem(dialog, IDC_ABOUT_NAME);
            const auto dialog_font = reinterpret_cast<HFONT>(
                SendMessageW(dialog, WM_GETFONT, 0, 0));
            LOGFONTW name_log_font{};
            if (name_control != nullptr && dialog_font != nullptr
                && GetObjectW(
                       dialog_font,
                       static_cast<int>(sizeof(name_log_font)),
                       &name_log_font) == static_cast<int>(sizeof(name_log_font))) {
                name_log_font.lfHeight = MulDiv(name_log_font.lfHeight, 17, 9);
                name_log_font.lfWeight = FW_SEMIBOLD;
                state->name_font = CreateFontIndirectW(&name_log_font);
                if (state->name_font != nullptr) {
                    SendMessageW(
                        name_control,
                        WM_SETFONT,
                        reinterpret_cast<WPARAM>(state->name_font),
                        TRUE);
                }
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
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
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
                if (state->name_font != nullptr) {
                    DeleteObject(state->name_font);
                    state->name_font = nullptr;
                }
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
    AboutDialogState state{instance, nullptr, nullptr, close_immediately};
    return DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_ABOUT),
        owner,
        AboutDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
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

InkpodDocumentInfo EmptyDocumentInfo() noexcept {
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    return info;
}

bool QueryDocument(AppState& state, InkpodDocumentInfo& info) noexcept {
    info = EmptyDocumentInfo();
    return state.engine != nullptr && state.engine->GetDocumentInfo(info);
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
    state.recovery_path.clear();
    state.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    state.plane = INKPOD_PLANE_MAIN_LINE;
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
        try {
            state.current_path = path;
            state.recovery_path = path + L".recovery.inkpod";
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
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
        false);
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

bool ConfirmDiscard(AppState& state) noexcept {
    InkpodDocumentInfo info{};
    if (!QueryDocument(state, info)
        || (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
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
    if (!QueryDocument(state, during_check)
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

InkpodStatus InitializeCore(AppState& state) noexcept {
    try {
        state.engine = std::make_unique<inkpod::app::CoreEngine>();
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Start(
        inkpod::renderer::GetCanvasSnapshotSink(state.canvas), state.window);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    return CreateDefaultCell(state);
}

InkpodStatus ShutdownCore(AppState& state) noexcept {
    if (state.engine != nullptr) {
        state.engine->Stop();
        state.engine.reset();
    }
    return INKPOD_STATUS_OK;
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
            if (state != nullptr && (GetKeyState(VK_CONTROL) & 0x8000) != 0) {
                if (wparam == 'S') {
                    SendMessageW(window, WM_COMMAND, IDM_FILE_SAVE, 0);
                    return 0;
                }
                if (wparam == 'N') {
                    SendMessageW(window, WM_COMMAND, IDM_FILE_NEW, 0);
                    return 0;
                }
                if (wparam == 'O') {
                    SendMessageW(window, WM_COMMAND, IDM_FILE_OPEN, 0);
                    return 0;
                }
                if (wparam == 'Z') {
                    SendMessageW(window, WM_COMMAND, IDM_EDIT_UNDO, 0);
                    return 0;
                }
                if (wparam == 'Y') {
                    SendMessageW(window, WM_COMMAND, IDM_EDIT_REDO, 0);
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
        case inkpod::app::kCoreAsyncFailed:
            if (state != nullptr && !state->smoke_test) {
                ShowCoreError(*state, window, L"非同期処理");
            }
            return 0;
        case WM_TIMER:
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
    UpdateMenuState(state);

    int exit_code = 0;
    if (state.smoke_test) {
        exit_code = RunM1Smoke(state);
        if (exit_code == 0) {
            exit_code = RunM2Smoke(state);
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
