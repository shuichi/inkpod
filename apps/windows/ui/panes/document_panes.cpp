#include "document_panes.h"

#include <array>
#include <cstddef>
#include <new>
#include <utility>

#include "app/core_host.h"

namespace inkpod::windows::ui::panes {
namespace {

constexpr std::uint32_t kLayerThumbnailWidth = 80U;
constexpr std::uint32_t kLayerThumbnailHeight = 60U;
constexpr std::size_t kLayerThumbnailBytes =
    static_cast<std::size_t>(kLayerThumbnailWidth) * kLayerThumbnailHeight * 4U;

InkpodStatus LoadLayerThumbnail(
    InkpodCore* core,
    TreePaneNode& node) {
    std::array<std::uint8_t, kLayerThumbnailBytes> rgba{};
    InkpodLayerThumbnailBuffer output{};
    output.struct_size = sizeof(output);
    output.layer_id = node.id;
    output.maximum_width = kLayerThumbnailWidth;
    output.maximum_height = kLayerThumbnailHeight;
    output.pixels_rgba8 = rgba.data();
    output.pixel_capacity = static_cast<std::uint64_t>(rgba.size());
    const InkpodStatus status = inkpod_core_layer_thumbnail(core, &output);
    if (status != INKPOD_STATUS_OK || output.width == 0U || output.height == 0U
        || output.width > kLayerThumbnailWidth
        || output.height > kLayerThumbnailHeight
        || output.required_bytes > static_cast<std::uint64_t>(rgba.size())
        || output.stride_bytes != output.width * 4U
        || output.required_bytes
            != static_cast<std::uint64_t>(output.stride_bytes) * output.height) {
        return status == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : status;
    }
    node.thumbnail_width = output.width;
    node.thumbnail_height = output.height;
    node.thumbnail_stride_bytes = output.stride_bytes;
    node.thumbnail_revision = output.revision;
    node.thumbnail_bgra.resize(static_cast<std::size_t>(output.required_bytes));
    for (std::uint32_t y = 0U; y < output.height; ++y) {
        for (std::uint32_t x = 0U; x < output.width; ++x) {
            const std::size_t offset = static_cast<std::size_t>(y) * output.stride_bytes
                + static_cast<std::size_t>(x) * 4U;
            const std::uint32_t alpha = rgba[offset + 3U];
            const std::uint32_t checker = ((x / 8U) + (y / 8U)) % 2U == 0U ? 248U : 224U;
            const auto composite = [alpha, checker](std::uint8_t channel) {
                return static_cast<std::uint8_t>(
                    (std::uint32_t{channel} * alpha + checker * (255U - alpha) + 127U)
                    / 255U);
            };
            node.thumbnail_bgra[offset] = composite(rgba[offset + 2U]);
            node.thumbnail_bgra[offset + 1U] = composite(rgba[offset + 1U]);
            node.thumbnail_bgra[offset + 2U] = composite(rgba[offset]);
            node.thumbnail_bgra[offset + 3U] = 255U;
        }
    }
    return INKPOD_STATUS_OK;
}

} // namespace

DocumentPanesController::DocumentPanesController(app::CoreHost& engine) noexcept
    : engine_(engine) {}

InkpodStatus DocumentPanesController::LoadTree(
    std::uint64_t requested_layer_id,
    bool include_thumbnails,
    std::vector<TreePaneNode>& layers,
    std::vector<TreePaneNode>& planes,
    std::uint32_t& selected_layer_index) noexcept {
    return engine_.Invoke(
        [&layers, &planes, requested_layer_id, include_thumbnails, &selected_layer_index](
            InkpodCore* core) {
            try {
                layers.clear();
                planes.clear();
                layers.reserve(64U);
                planes.reserve(64U);
                for (std::uint32_t layer_index = 0U; layer_index < 1024U; ++layer_index) {
                    std::array<std::uint8_t, 256U> name{};
                    InkpodNodeInfo info{};
                    info.struct_size = sizeof(info);
                    info.name_utf8 = name.data();
                    info.name_capacity = name.size();
                    if (inkpod_core_node_get(core, layer_index, UINT32_MAX, &info)
                        != INKPOD_STATUS_OK) {
                        break;
                    }
                    TreePaneNode node{
                        info.id,
                        info.parent_id,
                        info.index,
                        info.kind,
                        info.pixel_format,
                        info.opacity_milli,
                        info.child_count,
                        info.flags,
                        std::string(
                            reinterpret_cast<const char*>(name.data()),
                            static_cast<std::size_t>(info.name_bytes))};
                    if (include_thumbnails) {
                        const InkpodStatus thumbnail_status = LoadLayerThumbnail(core, node);
                        if (thumbnail_status != INKPOD_STATUS_OK) {
                            return thumbnail_status;
                        }
                    }
                    layers.push_back(std::move(node));
                    if (info.id == requested_layer_id) {
                        selected_layer_index = layer_index;
                    }
                }
                if (layers.empty()) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
                if (requested_layer_id == 0U) {
                    selected_layer_index = 0U;
                }
                for (std::uint32_t plane_index = 0U; plane_index < 1024U; ++plane_index) {
                    std::array<std::uint8_t, 256U> name{};
                    InkpodNodeInfo info{};
                    info.struct_size = sizeof(info);
                    info.name_utf8 = name.data();
                    info.name_capacity = name.size();
                    if (inkpod_core_node_get(
                            core, selected_layer_index, plane_index, &info)
                        != INKPOD_STATUS_OK) {
                        break;
                    }
                    planes.push_back(TreePaneNode{
                        info.id,
                        info.parent_id,
                        info.index,
                        info.kind,
                        info.pixel_format,
                        info.opacity_milli,
                        info.child_count,
                        info.flags,
                        std::string(
                            reinterpret_cast<const char*>(name.data()),
                            static_cast<std::size_t>(info.name_bytes))});
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return INKPOD_STATUS_OK;
        },
        false,
        false);
}

InkpodStatus DocumentPanesController::LoadLightTable(
    app::DocumentSessionId session,
    app::Generation generation,
    std::vector<LightTablePaneSet>& sets,
    std::vector<LightTablePaneItem>& items) noexcept {
    return engine_.Invoke(
        session,
        generation,
        [&sets, &items](InkpodCore* core) {
            try {
                sets.clear();
                items.clear();
                for (std::uint32_t index = 0U; index < 256U; ++index) {
                    std::array<std::uint8_t, 1024U> name{};
                    InkpodLightTableSetInfo info{};
                    info.struct_size = sizeof(info);
                    info.name_utf8 = name.data();
                    info.name_capacity = name.size();
                    if (inkpod_core_light_table_set_get(core, index, &info)
                        != INKPOD_STATUS_OK) {
                        break;
                    }
                    sets.push_back(LightTablePaneSet{
                        info.id,
                        info.flags,
                        info.opacity_milli,
                        info.item_count,
                        std::string(
                            reinterpret_cast<const char*>(name.data()),
                            static_cast<std::size_t>(info.name_bytes))});
                }
                for (std::uint32_t index = 0U; index < 4096U; ++index) {
                    std::array<std::uint8_t, 1024U> name{};
                    LightTablePaneItem item{};
                    item.info.struct_size = sizeof(item.info);
                    item.info.display_color.struct_size = sizeof(item.info.display_color);
                    item.info.name_utf8 = name.data();
                    item.info.name_capacity = name.size();
                    if (inkpod_core_light_table_item_get(core, index, &item.info)
                        != INKPOD_STATUS_OK) {
                        break;
                    }
                    item.name.assign(
                        reinterpret_cast<const char*>(name.data()),
                        static_cast<std::size_t>(item.info.name_bytes));
                    item.info.name_utf8 = nullptr;
                    item.info.name_capacity = 0U;
                    items.push_back(std::move(item));
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return sets.empty() ? INKPOD_STATUS_INVALID_STATE : INKPOD_STATUS_OK;
        },
        false,
        false);
}

InkpodStatus DocumentPanesController::LoadSequence(
    app::DocumentSessionId session,
    app::Generation generation,
    std::vector<SequencePaneCell>& cells) noexcept {
    return engine_.Invoke(
        session,
        generation,
        [&cells](InkpodCore* core) {
            try {
                cells.clear();
                for (std::uint32_t index = 0U; index < 10000U; ++index) {
                    std::array<std::uint8_t, 1024U> name{};
                    SequencePaneCell cell{};
                    cell.info.struct_size = sizeof(cell.info);
                    cell.info.name_utf8 = name.data();
                    cell.info.name_capacity = name.size();
                    const InkpodStatus status =
                        inkpod_core_sequence_cell_get(core, index, &cell.info);
                    if (status == INKPOD_STATUS_INVALID_STATE && index == 0U) {
                        return INKPOD_STATUS_OK;
                    }
                    if (status == INKPOD_STATUS_INVALID_ARGUMENT) {
                        break;
                    }
                    if (status != INKPOD_STATUS_OK) {
                        return status;
                    }
                    cell.name.assign(
                        reinterpret_cast<const char*>(name.data()),
                        static_cast<std::size_t>(cell.info.name_bytes));
                    cell.info.name_utf8 = nullptr;
                    cell.info.name_capacity = 0U;
                    InkpodSequenceThumbnailBuffer thumbnail{};
                    thumbnail.struct_size = sizeof(thumbnail);
                    InkpodStatus thumbnail_status =
                        inkpod_core_sequence_thumbnail_get(core, index, &thumbnail);
                    if (thumbnail_status != INKPOD_STATUS_OK
                        || thumbnail.required_bytes == 0U
                        || thumbnail.required_bytes > 64U * 64U * 4U
                        || thumbnail.stride_bytes != thumbnail.width * 4U
                        || thumbnail.required_bytes
                            != static_cast<std::uint64_t>(thumbnail.stride_bytes)
                                * thumbnail.height) {
                        return thumbnail_status == INKPOD_STATUS_OK
                            ? INKPOD_STATUS_INVALID_STATE
                            : thumbnail_status;
                    }
                    cell.thumbnail_rgba.resize(
                        static_cast<std::size_t>(thumbnail.required_bytes));
                    thumbnail.pixels_rgba8 = cell.thumbnail_rgba.data();
                    thumbnail.pixel_capacity = cell.thumbnail_rgba.size();
                    thumbnail_status =
                        inkpod_core_sequence_thumbnail_get(core, index, &thumbnail);
                    if (thumbnail_status != INKPOD_STATUS_OK) {
                        return thumbnail_status;
                    }
                    cell.thumbnail_stride_bytes = thumbnail.stride_bytes;
                    cells.push_back(std::move(cell));
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return INKPOD_STATUS_OK;
        },
        false,
        false);
}

InkpodStatus DocumentPanesController::SampleLocator(
    std::uint64_t view_id,
    int device_x,
    int device_y,
    InkpodLocatorOutput& output) noexcept {
    return engine_.Invoke(
        [view_id, device_x, device_y, &output](InkpodCore* core) {
            return inkpod_core_locator_sample(
                core,
                view_id,
                static_cast<double>(device_x),
                static_cast<double>(device_y),
                &output);
        },
        false,
        false);
}

} // namespace inkpod::windows::ui::panes
