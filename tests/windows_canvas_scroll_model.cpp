#include "renderer/canvas_scroll_model.h"

#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <limits>

namespace {

using inkpod::renderer::CanvasScrollAxisInput;
using inkpod::renderer::CanvasScrollProjection;
using inkpod::renderer::CanvasScrollRangeLock;
using inkpod::renderer::CanvasScrollRangeUpdate;
using inkpod::renderer::CanvasScrollStatus;
using inkpod::renderer::CanvasScrollTargetKind;
using inkpod::renderer::CanvasScrollTargetRequest;
using inkpod::renderer::ProjectCanvasScrollAxis;
using inkpod::renderer::ResolveCanvasScrollTarget;

bool NearlyEqual(double left, double right) noexcept {
    return std::abs(left - right) <= 1.0e-12;
}

bool SmallDocumentUsesHalfViewportBase() noexcept {
    const auto result = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        150.0,
        1.0,
        100U,
        400U});
    return result.status == CanvasScrollStatus::Ok
        && result.projection.base_range.minimum_position == -200
        && result.projection.base_range.maximum_position == -100
        && result.projection.range == result.projection.base_range
        && result.projection.native.minimum == -200
        && result.projection.native.maximum == 299
        && result.projection.native.page == 400U
        && result.projection.native.position == -150
        && result.projection.coordinate_in_base_range
        && result.projection.range_changed;
}

bool LargeDocumentProducesScrollableRange() noexcept {
    const auto result = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -300.0,
        1.0,
        1000U,
        400U});
    return result.status == CanvasScrollStatus::Ok
        && result.projection.base_range.minimum_position == -200
        && result.projection.base_range.maximum_position == 800
        && result.projection.native.maximum == 1199
        && result.projection.native.position == 300
        && result.projection.fractional_residual == 0.0;
}

bool FractionalResidualRoundTripsThroughLineTargets() noexcept {
    const auto first = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -123.25,
        1.0,
        1000U,
        400U});
    if (first.status != CanvasScrollStatus::Ok
        || first.projection.native.position != 123
        || !NearlyEqual(first.projection.fractional_residual, 0.25)) {
        return false;
    }
    const auto forward = ResolveCanvasScrollTarget(
        first.projection,
        CanvasScrollTargetRequest{
            CanvasScrollTargetKind::LineForward,
            0,
            32U,
            0U});
    if (forward.status != CanvasScrollStatus::Ok || !forward.changed
        || forward.target_position != 155
        || !NearlyEqual(forward.target_coordinate, 155.25)
        || !NearlyEqual(forward.pan_by_delta, -32.0)) {
        return false;
    }
    const auto second = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -123.25 + forward.pan_by_delta,
        1.0,
        1000U,
        400U,
        first.projection.range});
    if (second.status != CanvasScrollStatus::Ok
        || second.projection.native.position != 155
        || !NearlyEqual(second.projection.fractional_residual, 0.25)) {
        return false;
    }
    const auto backward = ResolveCanvasScrollTarget(
        second.projection,
        CanvasScrollTargetRequest{
            CanvasScrollTargetKind::LineBackward,
            0,
            32U,
            0U});
    return backward.status == CanvasScrollStatus::Ok
        && backward.target_position == 123
        && NearlyEqual(backward.target_coordinate, 123.25)
        && NearlyEqual(backward.pan_by_delta, 32.0)
        && NearlyEqual(
            -123.25 + forward.pan_by_delta + backward.pan_by_delta,
            -123.25);
}

bool ExcursionExpandsAndPreserveKeepsStickyRange() noexcept {
    const auto expanded = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -1000.25,
        1.0,
        100U,
        400U});
    if (expanded.status != CanvasScrollStatus::Ok
        || expanded.projection.base_range.minimum_position != -200
        || expanded.projection.base_range.maximum_position != -100
        || expanded.projection.range.minimum_position != -200
        || expanded.projection.range.maximum_position != 1401
        || expanded.projection.native.position != 1000
        || expanded.projection.coordinate_in_base_range) {
        return false;
    }
    const auto preserved = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        150.0,
        1.0,
        100U,
        400U,
        expanded.projection.range,
        CanvasScrollRangeUpdate::Preserve});
    return preserved.status == CanvasScrollStatus::Ok
        && preserved.projection.range == expanded.projection.range
        && preserved.projection.coordinate_in_base_range
        && !preserved.projection.range_changed;
}

bool ResetDiscardsPriorExcursion() noexcept {
    const auto expanded = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -1000.0,
        1.0,
        100U,
        400U});
    if (expanded.status != CanvasScrollStatus::Ok) {
        return false;
    }
    const auto reset = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        150.0,
        1.0,
        100U,
        400U,
        expanded.projection.range,
        CanvasScrollRangeUpdate::ResetToBase});
    return reset.status == CanvasScrollStatus::Ok
        && reset.projection.range == reset.projection.base_range
        && reset.projection.range.minimum_position == -200
        && reset.projection.range.maximum_position == -100
        && reset.projection.range_changed;
}

bool ResetStillRepresentsCurrentOutsideCoordinate() noexcept {
    const auto previous = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        150.0,
        1.0,
        100U,
        400U});
    if (previous.status != CanvasScrollStatus::Ok) {
        return false;
    }
    const auto reset_outside = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -1000.25,
        1.0,
        100U,
        400U,
        previous.projection.range,
        CanvasScrollRangeUpdate::ResetToBase});
    return reset_outside.status == CanvasScrollStatus::Ok
        && reset_outside.projection.range.minimum_position == -200
        && reset_outside.projection.range.maximum_position == 1401
        && reset_outside.projection.native.position == 1000
        && NearlyEqual(reset_outside.projection.fractional_residual, 0.25);
}

bool ResizePreservesCoordinateAndUnionsRanges() noexcept {
    const auto original = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -300.0,
        1.0,
        1000U,
        400U});
    if (original.status != CanvasScrollStatus::Ok) {
        return false;
    }
    const auto resized = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -300.0,
        1.0,
        1000U,
        600U,
        original.projection.range,
        CanvasScrollRangeUpdate::Preserve});
    return resized.status == CanvasScrollStatus::Ok
        && resized.projection.scroll_coordinate
            == original.projection.scroll_coordinate
        && resized.projection.native.position == original.projection.native.position
        && resized.projection.native.page == 600U
        && resized.projection.base_range.minimum_position == -300
        && resized.projection.base_range.maximum_position == 700
        && resized.projection.range.minimum_position == -300
        && resized.projection.range.maximum_position == 800;
}

bool FrozenRangeStaysFixedAndRejectsExcursions() noexcept {
    const auto expanded = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -1000.0,
        1.0,
        100U,
        400U});
    if (expanded.status != CanvasScrollStatus::Ok) {
        return false;
    }
    const auto frozen = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -1200.0,
        1.0,
        100U,
        500U,
        expanded.projection.range,
        CanvasScrollRangeUpdate::Preserve,
        CanvasScrollRangeLock::Freeze});
    const auto outside = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -1500.0,
        1.0,
        100U,
        500U,
        expanded.projection.range,
        CanvasScrollRangeUpdate::Preserve,
        CanvasScrollRangeLock::Freeze});
    const auto contradictory = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -1200.0,
        1.0,
        100U,
        500U,
        expanded.projection.range,
        CanvasScrollRangeUpdate::ResetToBase,
        CanvasScrollRangeLock::Freeze});
    return frozen.status == CanvasScrollStatus::Ok
        && frozen.projection.range == expanded.projection.range
        && !frozen.projection.range_changed
        && outside.status == CanvasScrollStatus::FrozenRangeViolation
        && contradictory.status == CanvasScrollStatus::InvalidInput;
}

bool ExtremeSupportedValuesRemainScrollInfoCompatible() noexcept {
    constexpr std::uint32_t kMaximumDocumentExtent = 1'048'576U;
    constexpr double kMaximumZoom = 64.0;
    constexpr double kMaximumPan = -16'777'216.0;
    const auto result = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        kMaximumPan,
        kMaximumZoom,
        kMaximumDocumentExtent,
        16'384U});
    return result.status == CanvasScrollStatus::Ok
        && result.projection.native.minimum == -8192
        && result.projection.native.maximum == 67'117'055
        && result.projection.native.position == 16'777'216;
}

bool OverflowAndInvalidInputAreExplicit() noexcept {
    const auto extent_overflow = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        0.0,
        1.0,
        std::numeric_limits<std::uint32_t>::max(),
        400U});
    const auto pan_overflow = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        std::numeric_limits<double>::max(),
        1.0,
        100U,
        400U});
    const auto invalid_zoom = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        0.0,
        std::numeric_limits<double>::quiet_NaN(),
        100U,
        400U});
    const auto invalid_viewport = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        0.0,
        1.0,
        100U,
        0U});
    const auto invalid_range = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        0.0,
        1.0,
        100U,
        400U,
        {true, 10, -10}});
    const auto native_maximum_overflow = ProjectCanvasScrollAxis(
        CanvasScrollAxisInput{
            0.0,
            1.0,
            100U,
            400U,
            {true, 0, std::numeric_limits<std::int32_t>::max()},
            CanvasScrollRangeUpdate::Preserve,
            CanvasScrollRangeLock::Freeze});
    return extent_overflow.status == CanvasScrollStatus::Overflow
        && pan_overflow.status == CanvasScrollStatus::Overflow
        && invalid_zoom.status == CanvasScrollStatus::InvalidInput
        && invalid_viewport.status == CanvasScrollStatus::InvalidInput
        && invalid_range.status == CanvasScrollStatus::InvalidInput
        && native_maximum_overflow.status == CanvasScrollStatus::Overflow;
}

bool LinePageEndpointsAndThumbTargetsAreResolved() noexcept {
    const auto projected = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -123.25,
        1.0,
        1000U,
        400U});
    if (projected.status != CanvasScrollStatus::Ok) {
        return false;
    }
    const CanvasScrollProjection& value = projected.projection;
    const auto line_back = ResolveCanvasScrollTarget(
        value,
        {CanvasScrollTargetKind::LineBackward, 0, 32U, 0U});
    const auto line_forward = ResolveCanvasScrollTarget(
        value,
        {CanvasScrollTargetKind::LineForward, 0, 32U, 0U});
    const auto page_back = ResolveCanvasScrollTarget(
        value,
        {CanvasScrollTargetKind::PageBackward, 0, 0U, 100U});
    const auto page_forward = ResolveCanvasScrollTarget(
        value,
        {CanvasScrollTargetKind::PageForward, 0, 0U, 100U});
    const auto start = ResolveCanvasScrollTarget(
        value,
        {CanvasScrollTargetKind::Start});
    const auto end = ResolveCanvasScrollTarget(
        value,
        {CanvasScrollTargetKind::End});
    const auto thumb = ResolveCanvasScrollTarget(
        value,
        {CanvasScrollTargetKind::Thumb, 400});
    const auto invalid_thumb = ResolveCanvasScrollTarget(
        value,
        {CanvasScrollTargetKind::Thumb, 801});
    return line_back.status == CanvasScrollStatus::Ok
        && line_back.target_position == 91
        && NearlyEqual(line_back.pan_by_delta, 32.0)
        && line_forward.status == CanvasScrollStatus::Ok
        && line_forward.target_position == 155
        && NearlyEqual(line_forward.pan_by_delta, -32.0)
        && page_back.status == CanvasScrollStatus::Ok
        && page_back.target_position == 23
        && NearlyEqual(page_back.pan_by_delta, 100.0)
        && page_forward.status == CanvasScrollStatus::Ok
        && page_forward.target_position == 223
        && NearlyEqual(page_forward.pan_by_delta, -100.0)
        && start.status == CanvasScrollStatus::Ok
        && start.target_position == -200
        && NearlyEqual(start.target_coordinate, -200.0)
        && NearlyEqual(start.pan_by_delta, 323.25)
        && end.status == CanvasScrollStatus::Ok
        && end.target_position == 800
        && NearlyEqual(end.target_coordinate, 800.0)
        && NearlyEqual(end.pan_by_delta, -676.75)
        && thumb.status == CanvasScrollStatus::Ok
        && thumb.target_position == 400
        && NearlyEqual(thumb.target_coordinate, 400.25)
        && NearlyEqual(thumb.pan_by_delta, -277.0)
        && invalid_thumb.status == CanvasScrollStatus::InvalidInput;
}

bool EndpointAndSameThumbNoOpsAreStable() noexcept {
    const auto at_start = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        200.0,
        1.0,
        1000U,
        400U});
    if (at_start.status != CanvasScrollStatus::Ok) {
        return false;
    }
    const auto line = ResolveCanvasScrollTarget(
        at_start.projection,
        {CanvasScrollTargetKind::LineBackward, 0, 32U, 0U});
    const auto start = ResolveCanvasScrollTarget(
        at_start.projection,
        {CanvasScrollTargetKind::Start});

    const auto fractional = ProjectCanvasScrollAxis(CanvasScrollAxisInput{
        -123.25,
        1.0,
        1000U,
        400U});
    if (fractional.status != CanvasScrollStatus::Ok) {
        return false;
    }
    const auto same_thumb = ResolveCanvasScrollTarget(
        fractional.projection,
        {CanvasScrollTargetKind::Thumb, 123});
    return line.status == CanvasScrollStatus::Ok && !line.changed
        && line.pan_by_delta == 0.0
        && start.status == CanvasScrollStatus::Ok && !start.changed
        && start.pan_by_delta == 0.0
        && same_thumb.status == CanvasScrollStatus::Ok
        && !same_thumb.changed && same_thumb.pan_by_delta == 0.0
        && NearlyEqual(same_thumb.target_coordinate, 123.25);
}

struct TestCase {
    const char* name;
    bool (*run)() noexcept;
};

}  // namespace

int main() {
    constexpr std::array tests{
        TestCase{"small document", SmallDocumentUsesHalfViewportBase},
        TestCase{"large document", LargeDocumentProducesScrollableRange},
        TestCase{"fractional round trip", FractionalResidualRoundTripsThroughLineTargets},
        TestCase{"sticky expansion", ExcursionExpandsAndPreserveKeepsStickyRange},
        TestCase{"range reset", ResetDiscardsPriorExcursion},
        TestCase{"outside reset", ResetStillRepresentsCurrentOutsideCoordinate},
        TestCase{"manual resize", ResizePreservesCoordinateAndUnionsRanges},
        TestCase{"frozen range", FrozenRangeStaysFixedAndRejectsExcursions},
        TestCase{"extreme supported values", ExtremeSupportedValuesRemainScrollInfoCompatible},
        TestCase{"invalid and overflow", OverflowAndInvalidInputAreExplicit},
        TestCase{"scroll targets", LinePageEndpointsAndThumbTargetsAreResolved},
        TestCase{"no-op stability", EndpointAndSameThumbNoOpsAreStable},
    };
    for (const auto& test : tests) {
        if (!test.run()) {
            std::fprintf(stderr, "canvas scroll model test failed: %s\n", test.name);
            return EXIT_FAILURE;
        }
    }
    return EXIT_SUCCESS;
}
