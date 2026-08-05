#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>

#include "document_identity.h"
#include "frontend_state.h"

namespace inkpod::app {

class CoreHost;

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
    DocumentIdentity identity{};
    std::uint32_t untitled_number{};
    DocumentShellState shell{};
    InkpodEditorStateInfo editor_presentation{sizeof(InkpodEditorStateInfo)};
    bool has_editor_presentation{};

    void BindCore(CoreHost* host) noexcept;
    [[nodiscard]] CoreHost* Core() const noexcept;

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
    [[nodiscard]] DocumentView* ViewAt(std::size_t index) noexcept;
    [[nodiscard]] const DocumentView* ViewAt(std::size_t index) const noexcept;
    [[nodiscard]] std::size_t ViewCount() const noexcept;

private:
    CoreHost* core_{};
    std::array<DocumentView, kMaximumViews> views_{};
    std::array<bool, kMaximumViews> view_used_{};
    std::size_t view_count_{};
    DocumentViewId active_view_{};
};

class DocumentRegistry final {
public:
    static constexpr std::size_t kMaximumSessions = 64U;

    [[nodiscard]] bool InitializePlaceholder(Generation generation) noexcept;
    [[nodiscard]] bool Replace(
        DocumentSessionId id,
        Generation generation,
        DocumentViewId initial_view,
        CoreHost* core) noexcept;
    [[nodiscard]] bool Add(
        DocumentSessionId id,
        Generation generation,
        DocumentViewId initial_view,
        CoreHost* core) noexcept;
    [[nodiscard]] bool Remove(DocumentSessionId id) noexcept;
    [[nodiscard]] bool Activate(DocumentSessionId id) noexcept;
    [[nodiscard]] DocumentSession* Find(DocumentSessionId id) noexcept;
    [[nodiscard]] const DocumentSession* Find(DocumentSessionId id) const noexcept;
    [[nodiscard]] DocumentSession* FindByView(DocumentViewId view) noexcept;
    [[nodiscard]] const DocumentSession* FindByView(
        DocumentViewId view) const noexcept;
    [[nodiscard]] DocumentSession* FindByIdentity(
        const DocumentIdentity& identity) noexcept;
    [[nodiscard]] const DocumentSession* FindByIdentity(
        const DocumentIdentity& identity) const noexcept;
    [[nodiscard]] bool AssignIdentity(
        DocumentSessionId id,
        const DocumentIdentity& identity) noexcept;
    void ClearCoreBindings() noexcept;
    void Clear() noexcept;
    [[nodiscard]] DocumentSession* Current() noexcept;
    [[nodiscard]] const DocumentSession* Current() const noexcept;
    [[nodiscard]] DocumentSession* SessionAt(std::size_t index) noexcept;
    [[nodiscard]] const DocumentSession* SessionAt(
        std::size_t index) const noexcept;
    [[nodiscard]] std::size_t Count() const noexcept;

private:
    std::array<std::unique_ptr<DocumentSession>, kMaximumSessions> sessions_{};
    std::size_t current_index_{kMaximumSessions};
    std::size_t count_{};
};

}  // namespace inkpod::app
