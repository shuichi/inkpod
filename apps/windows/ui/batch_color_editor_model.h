#pragma once

#include <cstddef>
#include <cstdint>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
struct BatchOperationUi;
}

namespace inkpod::windows::ui {

enum class BatchColorSlot : std::uint8_t {
    Primary,
    Secondary,
};

[[nodiscard]] const InkpodColorValue* BatchOperationColor(
    const app::BatchOperationUi& operation,
    std::size_t row,
    BatchColorSlot slot) noexcept;

[[nodiscard]] bool SetBatchOperationColor(
    app::BatchOperationUi& operation,
    std::size_t row,
    BatchColorSlot slot,
    const InkpodColorValue& color) noexcept;

[[nodiscard]] bool SetBatchOperationColorAlpha(
    app::BatchOperationUi& operation,
    std::size_t row,
    BatchColorSlot slot,
    std::uint32_t alpha) noexcept;

}  // namespace inkpod::windows::ui
