#include "palette_window.h"

#include <algorithm>
#include <cstdint>
#include <limits>

namespace inkpod::windows::ui {
namespace {

constexpr wchar_t kSettingsKey[] = L"Software\\Inkpod";
constexpr std::uint32_t kPlacementMagic = UINT32_C(0x50574b49);
constexpr std::uint32_t kPlacementVersion = 1U;

struct PersistedPlacement {
    std::uint32_t magic;
    std::uint32_t version;
    std::uint32_t struct_size;
    std::uint32_t dpi;
    WINDOWPLACEMENT placement;
};

bool LoadPlacement(
    const wchar_t* value_name,
    PersistedPlacement& output) noexcept {
    if (value_name == nullptr || *value_name == L'\0') {
        return false;
    }
    DWORD type{};
    DWORD size = sizeof(output);
    const LSTATUS status = RegGetValueW(
        HKEY_CURRENT_USER,
        kSettingsKey,
        value_name,
        RRF_RT_REG_BINARY,
        &type,
        &output,
        &size);
    return status == ERROR_SUCCESS && type == REG_BINARY
        && size == sizeof(output) && output.magic == kPlacementMagic
        && output.version == kPlacementVersion
        && output.struct_size == sizeof(output)
        && output.dpi >= 48U && output.dpi <= 960U
        && output.placement.length == sizeof(WINDOWPLACEMENT);
}

bool WorkAreaForRect(const RECT& bounds, RECT& work_area) noexcept {
    RECT candidate = bounds;
    const HMONITOR monitor = MonitorFromRect(&candidate, MONITOR_DEFAULTTONULL);
    if (monitor == nullptr) {
        return false;
    }
    MONITORINFO info{};
    info.cbSize = sizeof(info);
    if (GetMonitorInfoW(monitor, &info) == FALSE) {
        return false;
    }
    work_area = info.rcWork;
    return true;
}

bool NormalizeToWorkArea(RECT& bounds) noexcept {
    if (bounds.right <= bounds.left || bounds.bottom <= bounds.top) {
        return false;
    }
    RECT work{};
    if (!WorkAreaForRect(bounds, work)) {
        return false;
    }
    const LONG work_width = work.right - work.left;
    const LONG work_height = work.bottom - work.top;
    LONG width = bounds.right - bounds.left;
    LONG height = bounds.bottom - bounds.top;
    if (work_width <= 0 || work_height <= 0 || width <= 0 || height <= 0
        || width > std::numeric_limits<short>::max()
        || height > std::numeric_limits<short>::max()) {
        return false;
    }
    width = std::min(width, work_width);
    height = std::min(height, work_height);
    const LONG left = std::clamp(bounds.left, work.left, work.right - width);
    const LONG top = std::clamp(bounds.top, work.top, work.bottom - height);
    bounds = {left, top, left + width, top + height};
    return true;
}

bool PlaceBesideOwner(HWND palette, HWND owner) noexcept {
    RECT palette_bounds{};
    RECT owner_bounds{};
    if (GetWindowRect(palette, &palette_bounds) == FALSE
        || GetWindowRect(owner, &owner_bounds) == FALSE) {
        return false;
    }
    const HMONITOR monitor = MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST);
    MONITORINFO info{};
    info.cbSize = sizeof(info);
    if (monitor == nullptr || GetMonitorInfoW(monitor, &info) == FALSE) {
        return false;
    }
    const LONG width = std::min(
        palette_bounds.right - palette_bounds.left,
        info.rcWork.right - info.rcWork.left);
    const LONG height = std::min(
        palette_bounds.bottom - palette_bounds.top,
        info.rcWork.bottom - info.rcWork.top);
    if (width <= 0 || height <= 0) {
        return false;
    }
    constexpr LONG offset = 16;
    const LONG preferred_left = owner_bounds.right - width - offset;
    const LONG preferred_top = owner_bounds.top + offset;
    const LONG left = std::clamp(
        preferred_left,
        info.rcWork.left,
        info.rcWork.right - width);
    const LONG top = std::clamp(
        preferred_top,
        info.rcWork.top,
        info.rcWork.bottom - height);
    return SetWindowPos(
               palette,
               nullptr,
               left,
               top,
               width,
               height,
               SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER)
        != FALSE;
}

}  // namespace

bool RestorePaletteWindowPlacement(
    HWND palette,
    HWND owner,
    const wchar_t* value_name,
    bool load_persisted) noexcept {
    if (palette == nullptr || owner == nullptr) {
        return false;
    }
    PersistedPlacement saved{};
    if (load_persisted && LoadPlacement(value_name, saved)
        && NormalizeToWorkArea(saved.placement.rcNormalPosition)) {
        saved.placement.flags = 0U;
        saved.placement.showCmd = SW_HIDE;
        if (SetWindowPlacement(palette, &saved.placement) != FALSE) {
            return true;
        }
    }
    return PlaceBesideOwner(palette, owner);
}

bool SavePaletteWindowPlacement(
    HWND palette,
    const wchar_t* value_name) noexcept {
    if (palette == nullptr || value_name == nullptr || *value_name == L'\0') {
        return false;
    }
    PersistedPlacement saved{};
    saved.magic = kPlacementMagic;
    saved.version = kPlacementVersion;
    saved.struct_size = sizeof(saved);
    saved.dpi = GetDpiForWindow(palette);
    saved.placement.length = sizeof(saved.placement);
    if (saved.dpi == 0U || GetWindowPlacement(palette, &saved.placement) == FALSE
        || !PaletteRectIntersectsCurrentMonitor(
            saved.placement.rcNormalPosition)) {
        return false;
    }
    HKEY key{};
    if (RegCreateKeyExW(
            HKEY_CURRENT_USER,
            kSettingsKey,
            0,
            nullptr,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            nullptr,
            &key,
            nullptr)
        != ERROR_SUCCESS) {
        return false;
    }
    const LSTATUS status = RegSetValueExW(
        key,
        value_name,
        0,
        REG_BINARY,
        reinterpret_cast<const BYTE*>(&saved),
        sizeof(saved));
    RegCloseKey(key);
    return status == ERROR_SUCCESS;
}

bool PaletteRectIntersectsCurrentMonitor(const RECT& bounds) noexcept {
    RECT work{};
    return bounds.right > bounds.left && bounds.bottom > bounds.top
        && WorkAreaForRect(bounds, work);
}

bool PaletteWindowIsShown(HWND palette) noexcept {
    return palette != nullptr
        && (static_cast<DWORD>(GetWindowLongPtrW(palette, GWL_STYLE))
                & WS_VISIBLE)
            != 0U;
}

bool SetPaletteWindowShown(HWND palette, bool shown) noexcept {
    return palette != nullptr
        && SetWindowPos(
               palette,
               nullptr,
               0,
               0,
               0,
               0,
               SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
                   | SWP_NOOWNERZORDER | SWP_NOZORDER
                   | (shown ? SWP_SHOWWINDOW : SWP_HIDEWINDOW))
            != FALSE;
}

}  // namespace inkpod::windows::ui
