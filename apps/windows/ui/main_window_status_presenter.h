#pragma once

#include <windows.h>

#include <array>
#include <cstddef>

namespace inkpod::windows::ui::runtime {

struct StatusBarPresentation final {
    std::array<const wchar_t*, 6U> parts{};
};

void PresentStatusBar(
    HWND status_bar, const StatusBarPresentation& presentation) noexcept;
void PresentStatusBarPart(
    HWND status_bar, std::size_t part, const wchar_t* text) noexcept;

}  // namespace inkpod::windows::ui::runtime
