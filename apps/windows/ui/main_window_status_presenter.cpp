#include "main_window_status_presenter.h"

#include <commctrl.h>

namespace inkpod::windows::ui::runtime {

void PresentStatusBarPart(
    HWND status_bar, std::size_t part, const wchar_t* text) noexcept {
    if (status_bar == nullptr || part >= 6U || text == nullptr) {
        return;
    }
    SendMessageW(
        status_bar,
        SB_SETTEXTW,
        static_cast<WPARAM>(part),
        reinterpret_cast<LPARAM>(text));
}

void PresentStatusBar(
    HWND status_bar, const StatusBarPresentation& presentation) noexcept {
    for (std::size_t part = 0U; part < presentation.parts.size(); ++part) {
        if (presentation.parts[part] != nullptr) {
            PresentStatusBarPart(status_bar, part, presentation.parts[part]);
        }
    }
}

}  // namespace inkpod::windows::ui::runtime
