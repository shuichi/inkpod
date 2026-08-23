#include "batch_color_editor_model.h"

#include "app/frontend_state.h"

namespace inkpod::windows::ui {
namespace {

InkpodColorValue* MutableBatchOperationColor(
    app::BatchOperationUi& operation,
    std::size_t row,
    BatchColorSlot slot) noexcept {
    if (operation.kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        if (row >= operation.color_pairs.size()) {
            return nullptr;
        }
        return slot == BatchColorSlot::Primary
            ? &operation.color_pairs[row].old_color
            : &operation.color_pairs[row].new_color;
    }
    if (slot != BatchColorSlot::Primary || row >= operation.colors.size()) {
        return nullptr;
    }
    return &operation.colors[row];
}

}  // namespace

const InkpodColorValue* BatchOperationColor(
    const app::BatchOperationUi& operation,
    std::size_t row,
    BatchColorSlot slot) noexcept {
    if (operation.kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        if (row >= operation.color_pairs.size()) {
            return nullptr;
        }
        return slot == BatchColorSlot::Primary
            ? &operation.color_pairs[row].old_color
            : &operation.color_pairs[row].new_color;
    }
    return slot == BatchColorSlot::Primary && row < operation.colors.size()
        ? &operation.colors[row]
        : nullptr;
}

bool SetBatchOperationColor(
    app::BatchOperationUi& operation,
    std::size_t row,
    BatchColorSlot slot,
    const InkpodColorValue& color) noexcept {
    InkpodColorValue* target = MutableBatchOperationColor(operation, row, slot);
    if (target == nullptr
        || (color.depth != INKPOD_COLOR_DEPTH_8
            && color.depth != INKPOD_COLOR_DEPTH_16)) {
        return false;
    }
    *target = color;
    target->struct_size = sizeof(InkpodColorValue);
    return true;
}

bool SetBatchOperationColorAlpha(
    app::BatchOperationUi& operation,
    std::size_t row,
    BatchColorSlot slot,
    std::uint32_t alpha) noexcept {
    InkpodColorValue* target = MutableBatchOperationColor(operation, row, slot);
    if (target == nullptr) {
        return false;
    }
    if (target->depth != INKPOD_COLOR_DEPTH_8
        && target->depth != INKPOD_COLOR_DEPTH_16) {
        return false;
    }
    const std::uint32_t maximum = target->depth == INKPOD_COLOR_DEPTH_16
        ? 65'535U
        : 255U;
    if (alpha > maximum) {
        return false;
    }
    target->alpha = static_cast<std::uint16_t>(alpha);
    return true;
}

}  // namespace inkpod::windows::ui
