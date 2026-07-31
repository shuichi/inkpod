#include "main_window_runtime.h"

#include "main_window_runtime_internal.h"

namespace inkpod::windows::ui::runtime {

InkpodStatus CreateDefaultCell(app::ApplicationHost& state) noexcept {
    return CreateDefaultCellImpl(state);
}

InkpodStatus OpenDocumentFromPath(
    app::ApplicationHost& state,
    const std::wstring& path) noexcept {
    return OpenDocumentFromPathImpl(state, path);
}

InkpodStatus OpenRecoveryFromPath(
    app::ApplicationHost& state,
    const std::wstring& path) noexcept {
    return OpenRecoveryFromPathImpl(state, path);
}

}  // namespace inkpod::windows::ui::runtime
