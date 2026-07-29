#pragma once

#include <cstdint>
#include <string>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreEngine;
}

namespace inkpod::windows::ui::panes {

struct TreePaneNode {
    std::uint64_t id{};
    std::uint64_t parent_id{};
    std::uint32_t index{};
    std::uint32_t kind{};
    std::uint32_t pixel_format{};
    std::uint32_t opacity_milli{};
    std::uint32_t child_count{};
    std::uint32_t flags{};
    std::string name;
    std::uint32_t thumbnail_width{};
    std::uint32_t thumbnail_height{};
    std::uint32_t thumbnail_stride_bytes{};
    std::vector<std::uint8_t> thumbnail_bgra;
};

struct LightTablePaneSet {
    std::uint64_t id{};
    std::uint32_t flags{};
    std::uint32_t opacity_milli{};
    std::uint32_t item_count{};
    std::string name;
};

struct LightTablePaneItem {
    InkpodLightTableItemInfo info{};
    std::string name;
};

struct SequencePaneCell {
    InkpodSequenceCellInfo info{};
    std::string name;
};

// Owns the Core-to-pane model adapter. Modeless palettes bind these records
// without coupling Core state to an HWND.
class DocumentPanesController final {
public:
    explicit DocumentPanesController(app::CoreEngine& engine) noexcept;

    InkpodStatus LoadTree(
        std::uint64_t requested_layer_id,
        bool include_thumbnails,
        std::vector<TreePaneNode>& layers,
        std::vector<TreePaneNode>& planes,
        std::uint32_t& selected_layer_index) noexcept;
    InkpodStatus LoadLightTable(
        std::vector<LightTablePaneSet>& sets,
        std::vector<LightTablePaneItem>& items) noexcept;
    InkpodStatus LoadSequence(std::vector<SequencePaneCell>& cells) noexcept;
    InkpodStatus SampleLocator(
        std::uint64_t view_id,
        int device_x,
        int device_y,
        InkpodLocatorOutput& output) noexcept;

private:
    app::CoreEngine& engine_;
};

} // namespace inkpod::windows::ui::panes
