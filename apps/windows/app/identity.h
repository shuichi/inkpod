#pragma once

#include <compare>
#include <cstdint>
#include <functional>

namespace inkpod::app {

// Frontend IDs share one monotonically issued value namespace but remain
// non-interchangeable at compile time. They never alias Core-local IDs.
template <typename Tag>
class StrongFrontendId final {
public:
    constexpr StrongFrontendId() noexcept = default;
    explicit constexpr StrongFrontendId(std::uint64_t value) noexcept : value_(value) {}

    [[nodiscard]] constexpr std::uint64_t Value() const noexcept {
        return value_;
    }

    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        return value_ != 0U;
    }

    constexpr auto operator<=>(const StrongFrontendId&) const noexcept = default;

private:
    std::uint64_t value_{};
};

struct WorkspaceWindowIdTag;
struct DocumentSessionIdTag;
struct DocumentViewIdTag;
struct EditorGroupIdTag;
struct CanvasIdTag;
struct PaneInstanceIdTag;
struct JobSessionIdTag;
struct GenerationTag;

using WorkspaceWindowId = StrongFrontendId<WorkspaceWindowIdTag>;
using DocumentSessionId = StrongFrontendId<DocumentSessionIdTag>;
using DocumentViewId = StrongFrontendId<DocumentViewIdTag>;
using EditorGroupId = StrongFrontendId<EditorGroupIdTag>;
using CanvasId = StrongFrontendId<CanvasIdTag>;
using PaneInstanceId = StrongFrontendId<PaneInstanceIdTag>;
using JobSessionId = StrongFrontendId<JobSessionIdTag>;
using Generation = StrongFrontendId<GenerationTag>;

}  // namespace inkpod::app

namespace std {

template <typename Tag>
struct hash<inkpod::app::StrongFrontendId<Tag>> {
    [[nodiscard]] size_t operator()(
        inkpod::app::StrongFrontendId<Tag> value) const noexcept {
        return hash<std::uint64_t>{}(value.Value());
    }
};

}  // namespace std
