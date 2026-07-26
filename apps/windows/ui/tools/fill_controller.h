#pragma once

#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreEngine;
}

namespace inkpod::windows::ui::tools {

class FillController final {
public:
    explicit FillController(app::CoreEngine& engine) noexcept;

    InkpodStatus Apply(
        InkpodFillInput input,
        const std::vector<InkpodColorValue>& inclusion_colors,
        InkpodFillResult& result) noexcept;

private:
    app::CoreEngine& engine_;
};

} // namespace inkpod::windows::ui::tools
