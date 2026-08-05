#pragma once

#include <cstdint>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreHost;
}

namespace inkpod::windows::ui::tools {

class FillController final {
public:
    explicit FillController(app::CoreHost& engine) noexcept;

    InkpodStatus Apply(
        std::uint64_t layer_id,
        std::uint64_t plane_id,
        InkpodFillInput input,
        const std::vector<InkpodColorValue>& inclusion_colors,
        InkpodFillResult& result) noexcept;

private:
    app::CoreHost& engine_;
};

} // namespace inkpod::windows::ui::tools
