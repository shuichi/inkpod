#pragma once

#include <cstdint>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreHost;
}

namespace inkpod::windows::ui::tools {

class FloatingPasteController final {
public:
    explicit FloatingPasteController(app::CoreHost& engine) noexcept;

    InkpodStatus Begin(
        const InkpodClipboard* clipboard, std::uint32_t mode) noexcept;
    InkpodStatus Transform(const InkpodFloatingTransform& transform) noexcept;
    InkpodStatus Finish(bool commit) noexcept;

private:
    app::CoreHost& engine_;
};

} // namespace inkpod::windows::ui::tools
