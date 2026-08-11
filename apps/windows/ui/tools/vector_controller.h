#pragma once

#include <cstdint>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreHost;
}

namespace inkpod::windows::ui::tools {

class VectorController final {
public:
    explicit VectorController(app::CoreHost& engine) noexcept;

    InkpodStatus ResolveGeometryPoints(
        std::uint64_t view_id,
        std::uint64_t expected_view_revision,
        bool bypass_snap,
        const std::vector<InkpodStrokeSample>& samples,
        std::vector<InkpodGeometryPoint>& points,
        std::uint64_t& resolved_view_revision) noexcept;
    InkpodStatus BeginGeometry(
        const InkpodGeometryInput& input,
        InkpodGeometryPreviewInfo& info) noexcept;
    InkpodStatus UpdateGeometry(
        const InkpodGeometryInput& input,
        InkpodGeometryPreviewInfo& info) noexcept;
    InkpodStatus CommitGeometry() noexcept;
    InkpodStatus CancelGeometry() noexcept;
    InkpodStatus AddPath(const InkpodVectorPathInput& input) noexcept;
    InkpodStatus Erase(const InkpodVectorEraseInput& input) noexcept;
    InkpodStatus Select(
        InkpodVectorSelectionMode mode,
        std::vector<std::uint64_t>& selected_path_ids) noexcept;
    InkpodStatus Connect(std::uint64_t plane_id, float maximum_gap) noexcept;
    InkpodStatus CorrectWidth(const InkpodVectorWidthInput& input) noexcept;

private:
    app::CoreHost& engine_;
};

} // namespace inkpod::windows::ui::tools
