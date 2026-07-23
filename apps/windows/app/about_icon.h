#pragma once

#include <windows.h>

namespace inkpod::windows {

HICON LoadPngIconResource(
    HINSTANCE instance,
    int resource_id,
    int icon_size) noexcept;

}  // namespace inkpod::windows
