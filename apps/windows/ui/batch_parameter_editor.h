#pragma once

#include <windows.h>

#include <cstdint>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
struct BatchUiState;
}

namespace inkpod::windows::ui {

using BatchDraftChangedCallback = void (*)(void* context) noexcept;

struct BatchParameterEditorBinding {
    void* context{};
    app::BatchUiState* draft{};
    const InkpodColorValue* drawing_color{};
    BatchDraftChangedCallback changed{};
};

HWND CreateBatchParameterEditor(
    HINSTANCE instance,
    HWND parent,
    BatchParameterEditorBinding& binding) noexcept;

void UpdateBatchParameterEditor(
    HWND editor,
    std::uint32_t selected_stage,
    bool enabled) noexcept;

}  // namespace inkpod::windows::ui
