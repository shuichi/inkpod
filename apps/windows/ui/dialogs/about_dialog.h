#pragma once

#include <windows.h>

namespace inkpod::windows::ui {

INT_PTR ShowAboutDialog(
    HINSTANCE instance, HWND owner, bool close_immediately) noexcept;

}  // namespace inkpod::windows::ui
