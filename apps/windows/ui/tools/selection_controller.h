#pragma once

#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreHost;
}

namespace inkpod::windows::ui::tools {

class SelectionController final {
public:
    explicit SelectionController(app::CoreHost& engine) noexcept;

    InkpodStatus Apply(
        InkpodSelectionInput input,
        const std::vector<InkpodSelectionPoint>& points) noexcept;
    InkpodStatus ApplyEmpty(InkpodSelectionOperation operation) noexcept;
    InkpodStatus SelectColor(
        const InkpodColorValue& color,
        bool different,
        InkpodSelectionOperation operation) noexcept;

private:
    app::CoreHost& engine_;
};

} // namespace inkpod::windows::ui::tools
