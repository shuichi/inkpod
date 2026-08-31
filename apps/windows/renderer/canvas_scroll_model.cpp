#include "canvas_scroll_model.h"

#include <algorithm>
#include <cmath>
#include <limits>

namespace inkpod::renderer {
namespace {

constexpr double kInt64MinimumAsDouble = -9223372036854775808.0;
constexpr double kInt64UpperExclusiveAsDouble = 9223372036854775808.0;

bool IsRangeUpdateValid(CanvasScrollRangeUpdate value) noexcept {
    return value == CanvasScrollRangeUpdate::Preserve
        || value == CanvasScrollRangeUpdate::ResetToBase;
}

bool IsRangeLockValid(CanvasScrollRangeLock value) noexcept {
    return value == CanvasScrollRangeLock::Expand
        || value == CanvasScrollRangeLock::Freeze;
}

bool TryFloorToInt64(double value, std::int64_t& result) noexcept {
    if (!std::isfinite(value)) {
        return false;
    }
    const double rounded = std::floor(value);
    if (rounded < kInt64MinimumAsDouble
        || rounded >= kInt64UpperExclusiveAsDouble) {
        return false;
    }
    result = static_cast<std::int64_t>(rounded);
    return true;
}

bool TryCeilToInt64(double value, std::int64_t& result) noexcept {
    if (!std::isfinite(value)) {
        return false;
    }
    const double rounded = std::ceil(value);
    if (rounded < kInt64MinimumAsDouble
        || rounded >= kInt64UpperExclusiveAsDouble) {
        return false;
    }
    result = static_cast<std::int64_t>(rounded);
    return true;
}

bool TryRoundToInt64(double value, std::int64_t& result) noexcept {
    if (!std::isfinite(value)) {
        return false;
    }
    const double rounded = std::round(value);
    if (rounded < kInt64MinimumAsDouble
        || rounded >= kInt64UpperExclusiveAsDouble) {
        return false;
    }
    result = static_cast<std::int64_t>(rounded);
    return true;
}

bool TryAdd(
    std::int64_t left,
    std::int64_t right,
    std::int64_t& result) noexcept {
    if ((right > 0 && left > std::numeric_limits<std::int64_t>::max() - right)
        || (right < 0 && left < std::numeric_limits<std::int64_t>::min() - right)) {
        return false;
    }
    result = left + right;
    return true;
}

bool TrySubtract(
    std::int64_t left,
    std::int64_t right,
    std::int64_t& result) noexcept {
    if (right > 0 && left < std::numeric_limits<std::int64_t>::min() + right) {
        return false;
    }
    if (right < 0 && left > std::numeric_limits<std::int64_t>::max() + right) {
        return false;
    }
    result = left - right;
    return true;
}

bool FitsInt32(std::int64_t value) noexcept {
    return value >= std::numeric_limits<std::int32_t>::min()
        && value <= std::numeric_limits<std::int32_t>::max();
}

bool IsProjectionConsistent(const CanvasScrollProjection& projection) noexcept {
    if (!projection.range.initialized
        || projection.range.minimum_position > projection.range.maximum_position
        || !FitsInt32(projection.range.minimum_position)
        || !FitsInt32(projection.range.maximum_position)
        || projection.native.page == 0U
        || !std::isfinite(projection.scroll_coordinate)
        || !std::isfinite(projection.fractional_residual)) {
        return false;
    }
    std::int64_t expected_maximum{};
    if (!TryAdd(
            projection.range.maximum_position,
            static_cast<std::int64_t>(projection.native.page) - 1,
            expected_maximum)
        || !FitsInt32(expected_maximum)
        || projection.native.minimum != projection.range.minimum_position
        || projection.native.maximum != expected_maximum
        || projection.native.position < projection.range.minimum_position
        || projection.native.position > projection.range.maximum_position) {
        return false;
    }
    std::int64_t rounded_coordinate{};
    if (!TryRoundToInt64(projection.scroll_coordinate, rounded_coordinate)
        || rounded_coordinate != projection.native.position) {
        return false;
    }
    const double expected_residual = projection.scroll_coordinate
        - static_cast<double>(projection.native.position);
    return projection.fractional_residual == expected_residual;
}

}  // namespace

CanvasScrollProjectionResult ProjectCanvasScrollAxis(
    const CanvasScrollAxisInput& input) noexcept {
    CanvasScrollProjectionResult result{};
    if (!std::isfinite(input.pan) || !std::isfinite(input.zoom)
        || input.zoom <= 0.0 || input.document_extent == 0U
        || input.viewport_extent == 0U
        || !IsRangeUpdateValid(input.range_update)
        || !IsRangeLockValid(input.range_lock)
        || (input.previous_range.initialized
            && input.previous_range.minimum_position
                > input.previous_range.maximum_position)) {
        result.status = CanvasScrollStatus::InvalidInput;
        return result;
    }
    if (input.viewport_extent
        > static_cast<std::uint32_t>(std::numeric_limits<std::int32_t>::max())) {
        result.status = CanvasScrollStatus::Overflow;
        return result;
    }
    if (input.range_lock == CanvasScrollRangeLock::Freeze
        && (input.range_update != CanvasScrollRangeUpdate::Preserve
            || !input.previous_range.initialized)) {
        result.status = CanvasScrollStatus::InvalidInput;
        return result;
    }

    const double viewport = static_cast<double>(input.viewport_extent);
    const double content = static_cast<double>(input.document_extent) * input.zoom;
    if (!std::isfinite(content)) {
        result.status = CanvasScrollStatus::Overflow;
        return result;
    }
    const double coordinate = -input.pan;
    const double half_viewport = viewport * 0.5;
    std::int64_t base_minimum{};
    std::int64_t base_maximum{};
    if (!TryFloorToInt64(-half_viewport, base_minimum)
        || !TryCeilToInt64(content - half_viewport, base_maximum)) {
        result.status = CanvasScrollStatus::Overflow;
        return result;
    }
    if (base_minimum > base_maximum) {
        result.status = CanvasScrollStatus::InvalidInput;
        return result;
    }

    std::int64_t minimum = base_minimum;
    std::int64_t maximum = base_maximum;
    if (input.range_lock == CanvasScrollRangeLock::Freeze) {
        minimum = input.previous_range.minimum_position;
        maximum = input.previous_range.maximum_position;
        if (coordinate < static_cast<double>(minimum)
            || coordinate > static_cast<double>(maximum)) {
            result.status = CanvasScrollStatus::FrozenRangeViolation;
            return result;
        }
    } else {
        if (input.range_update == CanvasScrollRangeUpdate::Preserve
            && input.previous_range.initialized) {
            minimum = std::min(minimum, input.previous_range.minimum_position);
            maximum = std::max(maximum, input.previous_range.maximum_position);
        }
        const std::int64_t guard = static_cast<std::int64_t>(input.viewport_extent);
        if (coordinate < static_cast<double>(minimum)) {
            std::int64_t coordinate_floor{};
            if (!TryFloorToInt64(coordinate, coordinate_floor)
                || !TrySubtract(coordinate_floor, guard, minimum)) {
                result.status = CanvasScrollStatus::Overflow;
                return result;
            }
        }
        if (coordinate > static_cast<double>(maximum)) {
            std::int64_t coordinate_ceiling{};
            if (!TryCeilToInt64(coordinate, coordinate_ceiling)
                || !TryAdd(coordinate_ceiling, guard, maximum)) {
                result.status = CanvasScrollStatus::Overflow;
                return result;
            }
        }
    }

    std::int64_t position{};
    std::int64_t native_maximum{};
    if (!TryRoundToInt64(coordinate, position)
        || minimum > maximum || position < minimum || position > maximum
        || !FitsInt32(minimum) || !FitsInt32(maximum)
        || !TryAdd(
            maximum,
            static_cast<std::int64_t>(input.viewport_extent) - 1,
            native_maximum)
        || !FitsInt32(native_maximum)) {
        result.status = CanvasScrollStatus::Overflow;
        return result;
    }

    result.status = CanvasScrollStatus::Ok;
    result.projection.base_range = CanvasScrollRange{true, base_minimum, base_maximum};
    result.projection.range = CanvasScrollRange{true, minimum, maximum};
    result.projection.native.minimum = static_cast<std::int32_t>(minimum);
    result.projection.native.maximum = static_cast<std::int32_t>(native_maximum);
    result.projection.native.page = input.viewport_extent;
    result.projection.native.position = static_cast<std::int32_t>(position);
    result.projection.scroll_coordinate = coordinate;
    result.projection.fractional_residual = coordinate - static_cast<double>(position);
    result.projection.coordinate_in_base_range =
        coordinate >= static_cast<double>(base_minimum)
        && coordinate <= static_cast<double>(base_maximum);
    result.projection.range_changed = !input.previous_range.initialized
        || input.previous_range.minimum_position != minimum
        || input.previous_range.maximum_position != maximum;
    return result;
}

CanvasScrollTargetResult ResolveCanvasScrollTarget(
    const CanvasScrollProjection& projection,
    const CanvasScrollTargetRequest& request) noexcept {
    CanvasScrollTargetResult result{};
    if (!IsProjectionConsistent(projection)) {
        result.status = CanvasScrollStatus::InvalidInput;
        return result;
    }

    const std::int64_t minimum = projection.range.minimum_position;
    const std::int64_t maximum = projection.range.maximum_position;
    const std::int64_t current = projection.native.position;
    std::int64_t target = current;
    bool force_exact_endpoint = false;
    switch (request.kind) {
    case CanvasScrollTargetKind::LineBackward:
        if (request.line_step == 0U) {
            result.status = CanvasScrollStatus::InvalidInput;
            return result;
        }
        target = std::max(
            minimum,
            current - static_cast<std::int64_t>(request.line_step));
        break;
    case CanvasScrollTargetKind::LineForward:
        if (request.line_step == 0U) {
            result.status = CanvasScrollStatus::InvalidInput;
            return result;
        }
        target = std::min(
            maximum,
            current + static_cast<std::int64_t>(request.line_step));
        break;
    case CanvasScrollTargetKind::PageBackward:
        if (request.page_step == 0U) {
            result.status = CanvasScrollStatus::InvalidInput;
            return result;
        }
        target = std::max(
            minimum,
            current - static_cast<std::int64_t>(request.page_step));
        break;
    case CanvasScrollTargetKind::PageForward:
        if (request.page_step == 0U) {
            result.status = CanvasScrollStatus::InvalidInput;
            return result;
        }
        target = std::min(
            maximum,
            current + static_cast<std::int64_t>(request.page_step));
        break;
    case CanvasScrollTargetKind::Start:
        target = minimum;
        force_exact_endpoint = true;
        break;
    case CanvasScrollTargetKind::End:
        target = maximum;
        force_exact_endpoint = true;
        break;
    case CanvasScrollTargetKind::Thumb:
        target = request.thumb_position;
        if (target < minimum || target > maximum) {
            result.status = CanvasScrollStatus::InvalidInput;
            return result;
        }
        break;
    default:
        result.status = CanvasScrollStatus::InvalidInput;
        return result;
    }

    double target_coordinate = static_cast<double>(target);
    if (!force_exact_endpoint) {
        target_coordinate += projection.fractional_residual;
        target_coordinate = std::clamp(
            target_coordinate,
            static_cast<double>(minimum),
            static_cast<double>(maximum));
    }
    const double pan_by_delta = projection.scroll_coordinate - target_coordinate;
    if (!std::isfinite(target_coordinate) || !std::isfinite(pan_by_delta)) {
        result.status = CanvasScrollStatus::Overflow;
        return result;
    }
    result.status = CanvasScrollStatus::Ok;
    result.target_position = static_cast<std::int32_t>(target);
    result.target_coordinate = target_coordinate;
    result.pan_by_delta = pan_by_delta;
    result.changed = pan_by_delta != 0.0;
    return result;
}

}  // namespace inkpod::renderer
