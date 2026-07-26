#pragma once

#include <cstdint>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreEngine;
}

namespace inkpod::windows::ui::tools {

class ViewController final {
public:
    explicit ViewController(app::CoreEngine& engine) noexcept;

    InkpodStatus Apply(
        std::uint64_t view_id, const InkpodViewInput& input) noexcept;
    InkpodStatus AddGuide(
        std::uint32_t axis, std::int32_t position_milli,
        std::uint64_t& guide_id) noexcept;
    InkpodStatus MoveGuide(
        std::uint64_t guide_id, std::int32_t position_milli) noexcept;
    InkpodStatus DeleteGuide(std::uint64_t guide_id) noexcept;
    InkpodStatus DeleteAllGuides() noexcept;
    InkpodStatus SetGrid(const InkpodGridInput& input) noexcept;

private:
    app::CoreEngine& engine_;
};

} // namespace inkpod::windows::ui::tools
