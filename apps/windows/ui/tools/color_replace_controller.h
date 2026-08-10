#pragma once

#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreHost;
}

namespace inkpod::windows::ui::tools {

class ColorReplaceController final {
public:
    explicit ColorReplaceController(app::CoreHost& engine) noexcept;

    InkpodStatus Apply(
        InkpodScopedColorReplaceInput input,
        const std::vector<InkpodSelectionPoint>& points,
        InkpodDispatchResult& result) noexcept;

private:
    app::CoreHost& engine_;
};

} // namespace inkpod::windows::ui::tools
