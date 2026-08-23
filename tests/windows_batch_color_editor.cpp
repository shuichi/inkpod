#include "app/frontend_state.h"
#include "ui/batch_color_editor_model.h"

int wmain() {
    using inkpod::app::BatchOperationUi;
    using inkpod::windows::ui::BatchColorSlot;
    using inkpod::windows::ui::BatchOperationColor;
    using inkpod::windows::ui::SetBatchOperationColor;
    using inkpod::windows::ui::SetBatchOperationColorAlpha;

    BatchOperationUi replace{};
    replace.kind = INKPOD_BATCH_OPERATION_COLOR_REPLACE;
    InkpodBatchColorPairInput pair{};
    pair.struct_size = sizeof(pair);
    pair.enabled = 1U;
    pair.old_color = {
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 1U, 2U, 3U, 0U};
    pair.new_color = {
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 4U, 5U, 6U, 255U};
    replace.color_pairs.push_back(pair);

    if (!SetBatchOperationColorAlpha(
            replace, 0U, BatchColorSlot::Primary, 128U)
        || BatchOperationColor(replace, 0U, BatchColorSlot::Primary)->alpha
            != 128U) {
        return 1;
    }
    if (SetBatchOperationColorAlpha(
            replace, 0U, BatchColorSlot::Primary, 256U)
        || BatchOperationColor(replace, 0U, BatchColorSlot::Primary)->alpha
            != 128U) {
        return 2;
    }

    BatchOperationUi malformed = replace;
    malformed.color_pairs[0].old_color.depth = 0U;
    if (SetBatchOperationColorAlpha(
            malformed, 0U, BatchColorSlot::Primary, 0U)) {
        return 6;
    }

    const InkpodColorValue drawing{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 8U, 9U, 10U, 77U};
    if (!SetBatchOperationColor(
            replace, 0U, BatchColorSlot::Secondary, drawing)
        || BatchOperationColor(replace, 0U, BatchColorSlot::Secondary)->alpha
            != 77U
        || BatchOperationColor(replace, 0U, BatchColorSlot::Primary)->alpha
            != 128U) {
        return 3;
    }
    const InkpodColorValue replacement_old{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 11U, 12U, 13U, 99U};
    if (!SetBatchOperationColor(
            replace, 0U, BatchColorSlot::Primary, replacement_old)
        || BatchOperationColor(replace, 0U, BatchColorSlot::Primary)->alpha
            != 99U
        || BatchOperationColor(replace, 0U, BatchColorSlot::Secondary)->alpha
            != 77U) {
        return 7;
    }

    BatchOperationUi erase{};
    erase.kind = INKPOD_BATCH_OPERATION_ERASE;
    erase.colors.push_back(InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_16, 1U, 2U, 3U, 4U});
    if (BatchOperationColor(erase, 0U, BatchColorSlot::Secondary) != nullptr
        || !SetBatchOperationColorAlpha(
            erase, 0U, BatchColorSlot::Primary, 65'535U)
        || BatchOperationColor(erase, 0U, BatchColorSlot::Primary)->alpha
            != 65'535U
        || SetBatchOperationColorAlpha(
            erase, 0U, BatchColorSlot::Primary, 65'536U)) {
        return 4;
    }

    const InkpodColorValue before = *BatchOperationColor(
        erase, 0U, BatchColorSlot::Primary);
    if (SetBatchOperationColor(
            erase, 1U, BatchColorSlot::Primary, drawing)
        || BatchOperationColor(erase, 0U, BatchColorSlot::Primary)->alpha
            != before.alpha) {
        return 5;
    }
    return 0;
}
