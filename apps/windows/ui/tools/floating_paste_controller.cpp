#include "floating_paste_controller.h"

#include "app/core_engine.h"

namespace inkpod::windows::ui::tools {

FloatingPasteController::FloatingPasteController(app::CoreEngine& engine) noexcept
    : engine_(engine) {}

InkpodStatus FloatingPasteController::Begin(
    const InkpodClipboard* clipboard, std::uint32_t mode) noexcept {
    return engine_.Invoke(
        [clipboard, mode](InkpodCore* core) {
            return inkpod_core_paste_begin_mode(core, clipboard, mode);
        },
        false,
        false);
}

InkpodStatus FloatingPasteController::Transform(
    const InkpodFloatingTransform& transform) noexcept {
    return engine_.Invoke(
        [transform](InkpodCore* core) {
            return inkpod_core_floating_transform(core, &transform);
        },
        false,
        false);
}

InkpodStatus FloatingPasteController::Finish(bool commit) noexcept {
    return engine_.Invoke(
        [commit](InkpodCore* core) {
            if (!commit) {
                return inkpod_core_floating_cancel(core);
            }
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_floating_commit(core, &result);
        },
        commit,
        commit);
}

} // namespace inkpod::windows::ui::tools
