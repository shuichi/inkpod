#pragma once

#include <cstdint>
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
        std::uint64_t layer_id,
        std::uint64_t plane_id,
        InkpodSelectionInput input,
        const std::vector<InkpodSelectionPoint>& points) noexcept;
    InkpodStatus ApplyEmpty(InkpodSelectionOperation operation) noexcept;
    InkpodStatus SelectColor(
        std::uint64_t layer_id,
        std::uint64_t plane_id,
        const InkpodColorValue& color,
        bool different,
        InkpodSelectionOperation operation) noexcept;

private:
    app::CoreHost& engine_;
};

} // namespace inkpod::windows::ui::tools
