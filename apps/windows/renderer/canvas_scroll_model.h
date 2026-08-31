#pragma once

#include <cstdint>

namespace inkpod::renderer {

enum class CanvasScrollStatus : std::uint8_t {
    Ok,
    InvalidInput,
    Overflow,
    FrozenRangeViolation,
};

// Preserve keeps the caller-owned, per-view range sticky while incorporating
// the current half-viewport base range. ResetToBase discards the old range
// before incorporating the current coordinate.
enum class CanvasScrollRangeUpdate : std::uint8_t {
    Preserve,
    ResetToBase,
};

// Freeze keeps both endpoints unchanged during native thumb tracking. The
// caller must provide an initialized range that already contains the accepted
// Core coordinate.
enum class CanvasScrollRangeLock : std::uint8_t {
    Expand,
    Freeze,
};

// Inclusive range of legal native thumb positions. This is frontend-only
// state and must be stored independently for every logical view.
struct CanvasScrollRange {
    bool initialized{};
    std::int64_t minimum_position{};
    std::int64_t maximum_position{};

    constexpr bool operator==(const CanvasScrollRange&) const noexcept = default;
};

struct CanvasScrollAxisInput {
    // Core uses device = document * zoom + pan. The projected scrollbar
    // coordinate is therefore q = -pan.
    double pan{};
    double zoom{1.0};
    std::uint32_t document_extent{};
    std::uint32_t viewport_extent{};
    CanvasScrollRange previous_range{};
    CanvasScrollRangeUpdate range_update{CanvasScrollRangeUpdate::Preserve};
    CanvasScrollRangeLock range_lock{CanvasScrollRangeLock::Expand};
};

// Value-equivalent fields for Win32 SCROLLINFO. maximum is the inclusive
// nMax value, not the maximum legal nPos. All conversions are checked before
// this structure is produced.
struct CanvasNativeScrollInfo {
    std::int32_t minimum{};
    std::int32_t maximum{};
    std::uint32_t page{};
    std::int32_t position{};
};

struct CanvasScrollProjection {
    CanvasScrollRange base_range{};
    CanvasScrollRange range{};
    CanvasNativeScrollInfo native{};
    double scroll_coordinate{};
    double fractional_residual{};
    bool coordinate_in_base_range{};
    bool range_changed{};
};

struct CanvasScrollProjectionResult {
    CanvasScrollStatus status{CanvasScrollStatus::InvalidInput};
    CanvasScrollProjection projection{};
};

// Builds one horizontal or vertical projection from an accepted Core
// transform. The base range lets either document edge reach the viewport
// center. Expansion adds one viewport of guard space beyond an excursion.
[[nodiscard]] CanvasScrollProjectionResult ProjectCanvasScrollAxis(
    const CanvasScrollAxisInput& input) noexcept;

enum class CanvasScrollTargetKind : std::uint8_t {
    LineBackward,
    LineForward,
    PageBackward,
    PageForward,
    Start,
    End,
    Thumb,
};

struct CanvasScrollTargetRequest {
    CanvasScrollTargetKind kind{CanvasScrollTargetKind::Thumb};
    // Used only by Thumb. Pass SCROLLINFO.nTrackPos for SB_THUMBTRACK.
    std::int32_t thumb_position{};
    // The caller owns the UX policy for line and page increments. Only the
    // increment selected by kind must be nonzero.
    std::uint32_t line_step{};
    std::uint32_t page_step{};
};

struct CanvasScrollTargetResult {
    CanvasScrollStatus status{CanvasScrollStatus::InvalidInput};
    std::int32_t target_position{};
    double target_coordinate{};
    // Add this value to the exact accepted Core pan. Integer navigation keeps
    // the fractional q residual unless an endpoint must be reached exactly.
    double pan_by_delta{};
    bool changed{};
};

[[nodiscard]] CanvasScrollTargetResult ResolveCanvasScrollTarget(
    const CanvasScrollProjection& projection,
    const CanvasScrollTargetRequest& request) noexcept;

}  // namespace inkpod::renderer
