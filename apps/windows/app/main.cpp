#include <windows.h>
#include <commctrl.h>
#include <objbase.h>

#include <array>
#include <climits>
#include <cstdint>
#include <cwchar>

#include "canvas.h"
#include "inkpod/core_ffi.h"
#include "resource.h"

namespace {

struct AppState {
    HINSTANCE instance{};
    HWND canvas{};
    InkpodCore* core{};
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

std::array<wchar_t, 512> ReadCoreError() noexcept {
    std::array<wchar_t, 512> wide{};
    std::array<std::uint8_t, 512> utf8{};
    std::uint64_t written{};
    if (inkpod_error_message_copy(
            utf8.data(), utf8.size(), &written) != INKPOD_STATUS_OK) {
        wcsncpy_s(
            wide.data(), wide.size(), L"Unknown Core error", _TRUNCATE);
        return wide;
    }
    const int source_length = static_cast<int>(
        written < static_cast<std::uint64_t>(INT_MAX) ? written : INT_MAX);
    const int converted = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(utf8.data()),
        source_length,
        wide.data(),
        static_cast<int>(wide.size() - 1U));
    if (converted <= 0) {
        wcsncpy_s(
            wide.data(), wide.size(), L"Invalid UTF-8 Core error", _TRUNCATE);
    } else {
        wide[static_cast<std::size_t>(converted)] = L'\0';
    }
    return wide;
}

void ShowCoreError(HWND owner, const wchar_t* operation) noexcept {
    const auto detail = ReadCoreError();
    std::array<wchar_t, 768> message{};
    _snwprintf_s(
        message.data(),
        message.size(),
        _TRUNCATE,
        L"%ls に失敗しました。\n\n%ls",
        operation,
        detail.data());
    MessageBoxW(owner, message.data(), L"inkpod", MB_OK | MB_ICONERROR);
}

LRESULT CALLBACK MainWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<AppState*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));

    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
        state = static_cast<AppState*>(create->lpCreateParams);
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
            return state->canvas == nullptr ? -1 : 0;
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
            if (LOWORD(wparam) == IDM_APP_EXIT) {
                SendMessageW(window, WM_CLOSE, 0, 0);
                return 0;
            }
            break;
        case WM_CLOSE:
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
            SendMessageW(window, WM_CLOSE, 0, 0);
            return 0;
        case WM_NCDESTROY:
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            return DefWindowProcW(window, message, wparam, lparam);
        default:
            break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

bool RegisterMainWindowClass(
    HINSTANCE instance, const wchar_t* class_name) noexcept {
    WNDCLASSEXW window_class{};
    window_class.cbSize = sizeof(window_class);
    window_class.style = CS_HREDRAW | CS_VREDRAW;
    window_class.lpfnWndProc = MainWindowProcedure;
    window_class.hInstance = instance;
    window_class.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    window_class.hIcon = LoadIconW(nullptr, IDI_APPLICATION);
    window_class.hbrBackground = nullptr;
    window_class.lpszMenuName = MAKEINTRESOURCEW(IDR_MAIN_MENU);
    window_class.lpszClassName = class_name;
    return RegisterClassExW(&window_class) != 0;
}

InkpodStatus InitializeCore(AppState& state) noexcept {
    const InkpodCoreConfig config{
        sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
    InkpodStatus status = inkpod_core_create(&config, &state.core);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }

    const InkpodSnapshotOptions options{
        sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
    InkpodSnapshot* snapshot{};
    status = inkpod_core_build_snapshot(state.core, &options, &snapshot);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }

    InkpodSnapshotView view{};
    view.struct_size = sizeof(view);
    status = inkpod_snapshot_get_view(snapshot, &view);
    if (status == INKPOD_STATUS_OK) {
        SendMessageW(
            state.canvas,
            inkpod::renderer::kCanvasSetSnapshotRevision,
            static_cast<WPARAM>(view.revision),
            0);
    }
    const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
    return status == INKPOD_STATUS_OK ? release_status : status;
}

InkpodStatus ShutdownCore(AppState& state) noexcept {
    return inkpod_core_destroy(&state.core);
}

}  // namespace

int APIENTRY wWinMain(
    HINSTANCE instance,
    HINSTANCE,
    wchar_t* command_line,
    int show_command) {
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
            ShowCoreError(window, L"Rust Core の初期化");
        }
        ShutdownCore(state);
        DestroyWindow(window);
        return 15;
    }

    int exit_code = 0;
    if (state.smoke_test) {
        const bool resized = MoveWindow(state.canvas, 0, 0, 640, 480, FALSE) != FALSE;
        const bool dpi_changed = SendMessageW(
                                     state.canvas,
                                     WM_DPICHANGED_AFTERPARENT,
                                     0,
                                     0) == 1;
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
        exit_code = resized && dpi_changed && device_recovered && rendered ? 0 : 16;
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
