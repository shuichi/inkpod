#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>

#include "frontend_state.h"

namespace inkpod::app {

class CoreEngine;

struct DocumentView final {
    DocumentViewId id{};
    std::uint64_t core_view_id{};
    Generation generation{};
    ViewUiState presentation{};
};

class DocumentSession final {
public:
    static constexpr std::size_t kMaximumViews = 64U;

    DocumentSessionId id{};
    Generation generation{};
    DocumentShellState shell{};

    void BindCore(CoreEngine* engine) noexcept;
    [[nodiscard]] CoreEngine* Core() const noexcept;

    void ResetViews(
        DocumentViewId initial_view,
        Generation view_generation,
        std::uint64_t core_view_id = 0U) noexcept;
    [[nodiscard]] bool AddView(
        DocumentViewId view,
        Generation view_generation,
        std::uint64_t core_view_id) noexcept;
    [[nodiscard]] bool RemoveView(DocumentViewId view) noexcept;
    [[nodiscard]] bool ActivateView(DocumentViewId view) noexcept;
    [[nodiscard]] bool ActivateCoreView(std::uint64_t core_view_id) noexcept;
    [[nodiscard]] DocumentView* FindView(DocumentViewId view) noexcept;
    [[nodiscard]] const DocumentView* FindView(DocumentViewId view) const noexcept;
    [[nodiscard]] DocumentView* FindCoreView(std::uint64_t core_view_id) noexcept;
    [[nodiscard]] const DocumentView* FindCoreView(
        std::uint64_t core_view_id) const noexcept;
    [[nodiscard]] DocumentView* ActiveView() noexcept;
    [[nodiscard]] const DocumentView* ActiveView() const noexcept;
    [[nodiscard]] std::size_t ViewCount() const noexcept;

private:
    CoreEngine* core_{};
    std::array<DocumentView, kMaximumViews> views_{};
    std::array<bool, kMaximumViews> view_used_{};
    std::size_t view_count_{};
    DocumentViewId active_view_{};
};

class DocumentRegistry final {
public:
    [[nodiscard]] bool InitializePlaceholder(Generation generation) noexcept;
    [[nodiscard]] bool Replace(
        DocumentSessionId id,
        Generation generation,
        DocumentViewId initial_view,
        CoreEngine* core) noexcept;
    void Clear() noexcept;
    [[nodiscard]] DocumentSession* Current() noexcept;
    [[nodiscard]] const DocumentSession* Current() const noexcept;

private:
    std::unique_ptr<DocumentSession> current_;
};

}  // namespace inkpod::app
