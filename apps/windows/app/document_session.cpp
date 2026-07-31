#include "document_session.h"

#include <new>

namespace inkpod::app {
namespace {

void ResetPresentation(ViewUiState& view) noexcept {
    view.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    view.secondary_view_id = 0U;
    view.active_view_id = 0U;
    view.flip_horizontal = false;
    view.flip_vertical = false;
    view.ruler_visible = false;
    view.guides_visible = true;
    view.grid_visible = false;
    view.snap_guides = false;
    view.snap_grid = false;
    view.transparent_visible = true;
    view.pointer_device_x = 0;
    view.pointer_device_y = 0;
    ++view.locator_generation;
    view.locator_pending_token = 0U;
    view.locator_valid = false;
    view.locator = {};
    view.gesture_samples.clear();
    view.guide_drag_active = false;
    view.guide_drag_axis = 0U;
    view.guide_drag_id = 0U;
    view.active_drag.reset();
    {
        std::lock_guard lock(view.locator_results_mutex);
        view.locator_results.clear();
    }
}

void InitializeView(
    DocumentView& view,
    DocumentViewId id,
    Generation generation,
    std::uint64_t core_view_id) noexcept {
    view.id = id;
    view.generation = generation;
    view.core_view_id = core_view_id;
    ResetPresentation(view.presentation);
}

}  // namespace

void DocumentSession::BindCore(CoreEngine* engine) noexcept {
    core_ = engine;
}

CoreEngine* DocumentSession::Core() const noexcept {
    return core_;
}

void DocumentSession::ResetViews(
    DocumentViewId initial_view,
    Generation view_generation,
    std::uint64_t core_view_id) noexcept {
    view_used_.fill(false);
    InitializeView(views_[0], initial_view, view_generation, core_view_id);
    view_used_[0] = true;
    view_count_ = 1U;
    active_view_ = initial_view;
}

bool DocumentSession::AddView(
    DocumentViewId view,
    Generation view_generation,
    std::uint64_t core_view_id) noexcept {
    if (!view || view_count_ >= views_.size() || FindView(view) != nullptr
        || FindCoreView(core_view_id) != nullptr) {
        return false;
    }
    for (std::size_t index = 0U; index < views_.size(); ++index) {
        if (!view_used_[index]) {
            InitializeView(views_[index], view, view_generation, core_view_id);
            view_used_[index] = true;
            ++view_count_;
            active_view_ = view;
            return true;
        }
    }
    return false;
}

bool DocumentSession::RemoveView(DocumentViewId view) noexcept {
    for (std::size_t index = 0U; index < views_.size(); ++index) {
        if (!view_used_[index] || views_[index].id != view) {
            continue;
        }
        view_used_[index] = false;
        --view_count_;
        if (active_view_ == view) {
            active_view_ = {};
            for (std::size_t candidate = 0U; candidate < views_.size(); ++candidate) {
                if (view_used_[candidate]) {
                    active_view_ = views_[candidate].id;
                    break;
                }
            }
        }
        return true;
    }
    return false;
}

bool DocumentSession::ActivateView(DocumentViewId view) noexcept {
    if (FindView(view) == nullptr) {
        return false;
    }
    active_view_ = view;
    return true;
}

bool DocumentSession::ActivateCoreView(std::uint64_t core_view_id) noexcept {
    const DocumentView* view = FindCoreView(core_view_id);
    return view != nullptr && ActivateView(view->id);
}

DocumentView* DocumentSession::FindView(DocumentViewId view) noexcept {
    return const_cast<DocumentView*>(
        static_cast<const DocumentSession&>(*this).FindView(view));
}

const DocumentView* DocumentSession::FindView(DocumentViewId view) const noexcept {
    for (std::size_t index = 0U; index < views_.size(); ++index) {
        if (view_used_[index] && views_[index].id == view) {
            return &views_[index];
        }
    }
    return nullptr;
}

DocumentView* DocumentSession::FindCoreView(std::uint64_t core_view_id) noexcept {
    return const_cast<DocumentView*>(
        static_cast<const DocumentSession&>(*this).FindCoreView(core_view_id));
}

const DocumentView* DocumentSession::FindCoreView(
    std::uint64_t core_view_id) const noexcept {
    for (std::size_t index = 0U; index < views_.size(); ++index) {
        if (view_used_[index]
            && views_[index].core_view_id == core_view_id) {
            return &views_[index];
        }
    }
    return nullptr;
}

DocumentView* DocumentSession::ActiveView() noexcept {
    return FindView(active_view_);
}

const DocumentView* DocumentSession::ActiveView() const noexcept {
    return FindView(active_view_);
}

std::size_t DocumentSession::ViewCount() const noexcept {
    return view_count_;
}

bool DocumentRegistry::InitializePlaceholder(Generation generation) noexcept {
    if (!generation) {
        return false;
    }
    try {
        current_ = std::make_unique<DocumentSession>();
    } catch (const std::bad_alloc&) {
        return false;
    }
    current_->generation = generation;
    current_->ResetViews({}, generation);
    return true;
}

bool DocumentRegistry::Replace(
    DocumentSessionId id,
    Generation generation,
    DocumentViewId initial_view,
    CoreEngine* core) noexcept {
    if (!id || !generation || !initial_view) {
        return false;
    }
    if (current_ == nullptr) {
        try {
            current_ = std::make_unique<DocumentSession>();
        } catch (const std::bad_alloc&) {
            return false;
        }
    }
    current_->id = id;
    current_->generation = generation;
    current_->BindCore(core);
    current_->ResetViews(initial_view, generation);
    return true;
}

void DocumentRegistry::Clear() noexcept {
    current_.reset();
}

DocumentSession* DocumentRegistry::Current() noexcept {
    return current_.get();
}

const DocumentSession* DocumentRegistry::Current() const noexcept {
    return current_.get();
}

}  // namespace inkpod::app
