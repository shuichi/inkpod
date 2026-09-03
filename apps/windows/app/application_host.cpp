#include "application_host.h"

#include <algorithm>
#include <array>
#include <cassert>
#include <iterator>
#include <limits>
#include <new>
#include <utility>

#include "application_owner_graph.h"
#include "renderer/canvas.h"
#include "ui/dialogs/history_visualization_dialog.h"

namespace inkpod::app {

namespace {

std::uint32_t ColorToRgba8(const InkpodColorValue& color) noexcept {
    const auto channel = [&color](std::uint16_t value) noexcept {
        return color.depth == INKPOD_COLOR_DEPTH_16
            ? static_cast<std::uint32_t>(
                  (static_cast<std::uint32_t>(value) + 128U) / 257U)
            : static_cast<std::uint32_t>(value & 0xffU);
    };
    return (channel(color.red) << 24U) | (channel(color.green) << 16U)
        | (channel(color.blue) << 8U) | channel(color.alpha);
}

float Q16ToFloat(std::int64_t value) noexcept {
    return static_cast<float>(
        static_cast<double>(value) / static_cast<double>(UINT64_C(1) << 16U));
}

bool ProjectEditorPresentation(
    const InkpodEditorStateInfo& editor,
    const InkpodDocumentInfo* document_info,
    DocumentSessionId session,
    Generation generation,
    WorkspaceWindow& workspace) noexcept {
    try {
        ToolUiState& tools = workspace.tools;
        tools.editor.session = session;
        tools.editor.generation = generation;
        tools.editor.editor_revision = editor.editor_revision;
        std::copy(
            std::begin(editor.editor_digest),
            std::end(editor.editor_digest),
            tools.editor.editor_digest.begin());
        tools.active_tool = editor.active_tool;
        tools.last_color_consuming_tool =
            (editor.flags & INKPOD_EDITOR_STATE_HAS_LAST_COLOR_TOOL) != 0U
            ? editor.last_color_consuming_tool
            : 0U;
        if ((editor.flags & INKPOD_EDITOR_STATE_HAS_CURRENT_COLOR) != 0U) {
            tools.drawing_color = editor.current_color;
            tools.color_rgba = ColorToRgba8(editor.current_color);
        }
        tools.diameter = Q16ToFloat(editor.current_diameter_q16);
        tools.brush = editor.brush;

        tools.fill_options.operation = editor.fill.operation;
        tools.fill_options.tolerance = editor.fill.tolerance;
        tools.fill_options.gap_close = editor.fill.gap_close;
        tools.fill_options.extension_distance = editor.fill.extension_distance;
        tools.fill_options.inclusion_mode = editor.fill.inclusion_mode;
        tools.fill_options.overflow_abort =
            (editor.fill.flags & INKPOD_EDITOR_FILL_OVERFLOW_ABORT) != 0U;
        tools.fill_options.detached_regions =
            (editor.fill.flags & INKPOD_EDITOR_FILL_DETACHED_REGIONS) != 0U;
        tools.fill_options.transparent_only =
            (editor.fill.flags & INKPOD_EDITOR_FILL_TRANSPARENT_ONLY) != 0U;
        tools.fill_options.use_document_selection =
            (editor.fill.flags & INKPOD_EDITOR_FILL_DOCUMENT_SELECTION) != 0U;
        tools.fill_options.light_table_boundary =
            (editor.fill.flags & INKPOD_EDITOR_FILL_LIGHT_TABLE_BOUNDARY) != 0U;
        tools.fill_options.light_table_color =
            (editor.fill.flags & INKPOD_EDITOR_FILL_LIGHT_TABLE_COLOR) != 0U;
        tools.fill_options.inclusion_colors.clear();
        tools.fill_options.inclusion_colors.reserve(editor.fill.inclusion_color_count);
        for (std::uint32_t index = 0U;
             index < editor.fill.inclusion_color_count
                 && index < INKPOD_EDITOR_MAX_INCLUSION_COLORS;
             ++index) {
            tools.fill_options.inclusion_colors.push_back(
                editor.fill.inclusion_colors[index]);
        }

        tools.selection_shape = editor.selection.shape;
        tools.selection_operation = editor.selection.operation;
        tools.selection_tolerance = editor.selection.tolerance;
        tools.selection_gap_close = editor.selection.gap_close;
        tools.selection_diameter = Q16ToFloat(editor.selection.diameter_q16);
        tools.selection_interpretation = editor.selection.interpretation;
        tools.selection_aspect_ratio_q16 = editor.selection.aspect_ratio_q16;
        tools.selection_construction_flags = editor.selection.construction_flags;
        tools.selection_rotation_turns = editor.selection.rotation_turns;
        tools.selection_trace_shape = editor.selection.trace_shape;

        if ((editor.flags & INKPOD_EDITOR_STATE_HAS_TARGET) != 0U) {
            workspace.panes.active_tree_layer_id = editor.active_layer_id;
            workspace.panes.active_tree_plane_id = editor.active_plane_id;
            if (document_info != nullptr) {
                tools.active_plane = document_info->active_plane;
            }
        } else {
            workspace.panes.active_tree_layer_id = 0U;
            workspace.panes.active_tree_plane_id = 0U;
        }
        if ((editor.flags & INKPOD_EDITOR_STATE_HAS_PALETTE_CURSOR) != 0U) {
            workspace.panes.palette_group = editor.palette_group;
            workspace.panes.selected_palette_index = editor.palette_index;
        } else {
            workspace.panes.palette_group = 0U;
            workspace.panes.selected_palette_index = 0U;
        }
        tools.editor.valid = true;
        return true;
    } catch (const std::bad_alloc&) {
        workspace.tools.editor.valid = false;
        return false;
    }
}

bool WorkspaceActivelyPresentsView(
    const WorkspaceWindow& workspace, DocumentViewId view) noexcept {
    const EditorGroup* group = workspace.editors.Active();
    return group != nullptr && group->ActiveView() == view;
}

std::uint64_t SaturatingPaneBytes(
    std::uint64_t current,
    std::size_t item_count,
    std::size_t item_size = 1U) noexcept {
    const std::uint64_t count = static_cast<std::uint64_t>(item_count);
    const std::uint64_t size = static_cast<std::uint64_t>(item_size);
    const std::uint64_t bytes = count != 0U
            && size > std::numeric_limits<std::uint64_t>::max() / count
        ? std::numeric_limits<std::uint64_t>::max()
        : count * size;
    return current > std::numeric_limits<std::uint64_t>::max() - bytes
        ? std::numeric_limits<std::uint64_t>::max()
        : current + bytes;
}

}  // namespace

bool ApplicationHost::InitializeOwners() noexcept {
    routing.targets.Initialize();
    const Generation generation = routing.targets.CurrentGeneration();
    if (!InitializeOwnerGraph(
        workspaces_,
        documents_,
        this,
        routing.targets.Workspace(),
        routing.targets.EditorGroup(),
        routing.targets.Canvas(),
        generation)) {
        return false;
    }
    if (!RegisterWorkspacePanes(Workspace())) {
        ClearOwnerGraph(documents_, workspaces_);
        return false;
    }
    return true;
}

void ApplicationHost::ClearOwners() noexcept {
    thumbnails_.Clear();
    ClearOwnerGraph(documents_, workspaces_);
}

WorkspaceWindow& ApplicationHost::Workspace() noexcept {
    assert(workspaces_.Current() != nullptr);
    return *workspaces_.Current();
}

const WorkspaceWindow& ApplicationHost::Workspace() const noexcept {
    assert(workspaces_.Current() != nullptr);
    return *workspaces_.Current();
}

WorkspaceWindow* ApplicationHost::FindWorkspace(
    WorkspaceWindowId id) noexcept {
    return workspaces_.Find(id);
}

const WorkspaceWindow* ApplicationHost::FindWorkspace(
    WorkspaceWindowId id) const noexcept {
    return workspaces_.Find(id);
}

WorkspaceWindow* ApplicationHost::WorkspaceForView(
    DocumentViewId view) noexcept {
    const WorkspaceWindowId workspace = routing.targets.WorkspaceForView(view);
    return workspaces_.Find(workspace);
}

const WorkspaceWindow* ApplicationHost::WorkspaceForView(
    DocumentViewId view) const noexcept {
    const WorkspaceWindowId workspace = routing.targets.WorkspaceForView(view);
    return workspaces_.Find(workspace);
}

WorkspaceWindow* ApplicationHost::WorkspaceForWindow(HWND window) noexcept {
    return const_cast<WorkspaceWindow*>(
        static_cast<const ApplicationHost&>(*this).WorkspaceForWindow(window));
}

const WorkspaceWindow* ApplicationHost::WorkspaceForWindow(
    HWND window) const noexcept {
    if (window == nullptr) {
        return nullptr;
    }
    const HWND root_owner = GetAncestor(window, GA_ROOTOWNER);
    for (std::size_t index = 0U; index < workspaces_.Count(); ++index) {
        const WorkspaceWindow* workspace = workspaces_.At(index);
        if (workspace == nullptr || workspace->windows.window == nullptr) {
            continue;
        }
        const std::array owned{
            workspace->windows.window,
            workspace->tools.palette,
            workspace->panes.layer_palette,
            workspace->batch_palette,
            workspace->locator_palette,
            workspace->sequence_palette,
            workspace->light_table_palette,
            workspace->subpalette_palette};
        for (const HWND candidate : owned) {
            if (candidate != nullptr
                && (window == candidate || root_owner == candidate
                    || IsChild(candidate, window) != FALSE)) {
                return workspace;
            }
        }
    }
    return nullptr;
}

WorkspaceWindowRegistry& ApplicationHost::Workspaces() noexcept {
    return workspaces_;
}

const WorkspaceWindowRegistry& ApplicationHost::Workspaces() const noexcept {
    return workspaces_;
}

bool ApplicationHost::GetPaneResourceUsage(
    PaneInstanceId pane,
    PaneResourceUsage& usage) const noexcept {
    usage = {};
    if (!pane) {
        return false;
    }
    for (std::size_t index = 0U; index < workspaces_.Count(); ++index) {
        const WorkspaceWindow* workspace = workspaces_.At(index);
        if (workspace == nullptr) {
            continue;
        }
        usage.workspace = workspace->id;
        usage.pane = pane;
        if (pane == workspace->pane_ids.layer
            || pane == workspace->pane_ids.sequence) {
            windows::ui::ThumbnailPaneUsage cache_usage{};
            if (!thumbnails_.GetPaneUsage(pane, cache_usage)) {
                usage = {};
                return false;
            }
            usage.thumbnail_bytes = cache_usage.resident_bytes;
            usage.cached_item_count = cache_usage.entry_count;
            return true;
        }
        if (pane == workspace->pane_ids.color) {
            const auto& color = workspace->panes.color_pane;
            for (const auto size : {
                     color.picker_ring_pixels.size(),
                     color.picker_triangle_pixels.size(),
                     color.picker_frame_pixels.size(),
                     color.picker_present_pixels.size()}) {
                usage.cpu_cache_bytes = SaturatingPaneBytes(
                    usage.cpu_cache_bytes, size, sizeof(std::uint32_t));
                usage.cached_item_count += size == 0U ? 0U : 1U;
            }
            return true;
        }
        const auto& ids = workspace->pane_ids;
        if (pane == ids.tool || pane == ids.tool_options || pane == ids.batch
            || pane == ids.locator || pane == ids.light_table
            || pane == ids.reference || pane == ids.subpalette) {
            return true;
        }
    }
    usage = {};
    return false;
}

ApplicationResourceUsage ApplicationHost::ResourceUsage() const noexcept {
    ApplicationResourceUsage usage{};
    usage.workspace_window_count = workspaces_.Count();
    usage.document_session_count = documents_.Count();
    for (std::size_t index = 0U; index < documents_.Count(); ++index) {
        const DocumentSession* document = documents_.SessionAt(index);
        if (document != nullptr) {
            usage.document_view_count += document->ViewCount();
        }
    }
    for (std::size_t index = 0U; index < workspaces_.Count(); ++index) {
        const WorkspaceWindow* workspace = workspaces_.At(index);
        if (workspace == nullptr) {
            continue;
        }
        usage.editor_group_count += workspace->editors.GroupCount();
        usage.editor_canvas_count += workspace->editors.GroupCount();
        usage.auxiliary_canvas_count += workspace->subpalette_canvas_id ? 1U : 0U;
        for (const PaneInstanceId pane : {
                 workspace->pane_ids.tool,
                 workspace->pane_ids.tool_options,
                 workspace->pane_ids.color,
                 workspace->pane_ids.layer,
                 workspace->pane_ids.batch,
                 workspace->pane_ids.locator,
                 workspace->pane_ids.sequence,
                 workspace->pane_ids.light_table,
                 workspace->pane_ids.reference,
                 workspace->pane_ids.subpalette}) {
            usage.pane_instance_count += pane ? 1U : 0U;
        }
    }
    if (engine != nullptr) {
        usage.registered_snapshot_sink_count = engine->SnapshotSinkCount();
    }
    usage.thumbnails = thumbnails_.Usage();
    if (renderer != nullptr) {
        usage.renderer = renderer->ResourceUsage();
    }
    return usage;
}

windows::ui::ThumbnailCache& ApplicationHost::Thumbnails() noexcept {
    return thumbnails_;
}

const windows::ui::ThumbnailCache& ApplicationHost::Thumbnails() const noexcept {
    return thumbnails_;
}

bool ApplicationHost::RegisterWorkspacePanes(
    WorkspaceWindow& workspace) noexcept {
    using Policy = PaneTargetPolicy;
    constexpr std::array policies{
        Policy::Application,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::Application};
    std::array<PaneInstanceId, policies.size()> panes{};
    std::size_t registered{};
    for (; registered < panes.size(); ++registered) {
        const auto pane = routing.targets.RegisterPane();
        if (!pane.has_value()
            || routing.pane_targets.Register(pane.value(), policies[registered])
                != PaneTargetStatus::Ok) {
            if (pane.has_value()) {
                (void)routing.targets.UnregisterPane(pane.value());
            }
            for (std::size_t index = 0U; index < registered; ++index) {
                (void)routing.pane_targets.Unregister(panes[index]);
                (void)routing.targets.UnregisterPane(panes[index]);
            }
            return false;
        }
        panes[registered] = pane.value();
    }
    workspace.pane_ids = WorkspacePaneIds{
        panes[0], panes[1], panes[2], panes[3], panes[4],
        panes[5], panes[6], panes[7], panes[8], panes[9]};
    BindWorkspacePaneAliases(workspace);
    return true;
}

void ApplicationHost::BindWorkspacePaneAliases(
    const WorkspaceWindow& workspace) noexcept {
    routing.tool_pane = workspace.pane_ids.tool;
    routing.tool_options_pane = workspace.pane_ids.tool_options;
    routing.color_pane = workspace.pane_ids.color;
    routing.layer_pane = workspace.pane_ids.layer;
    routing.batch_pane = workspace.pane_ids.batch;
    routing.locator_pane = workspace.pane_ids.locator;
    routing.sequence_pane = workspace.pane_ids.sequence;
    routing.light_table_pane = workspace.pane_ids.light_table;
    routing.reference_pane = workspace.pane_ids.reference;
    routing.subpalette_pane = workspace.pane_ids.subpalette;
}

void ApplicationHost::UnregisterWorkspacePanes(
    WorkspaceWindow& workspace) noexcept {
    const std::array panes{
        workspace.pane_ids.tool,
        workspace.pane_ids.tool_options,
        workspace.pane_ids.color,
        workspace.pane_ids.layer,
        workspace.pane_ids.batch,
        workspace.pane_ids.locator,
        workspace.pane_ids.sequence,
        workspace.pane_ids.light_table,
        workspace.pane_ids.reference,
        workspace.pane_ids.subpalette};
    for (const PaneInstanceId pane : panes) {
        if (pane) {
            thumbnails_.RemovePane(pane);
            (void)routing.pane_targets.Unregister(pane);
            (void)routing.targets.UnregisterPane(pane);
        }
    }
    workspace.pane_ids = {};
}

WorkspaceWindow* ApplicationHost::AddWorkspaceWindow() noexcept {
    const WorkspaceWindowId previous = routing.targets.Workspace();
    const auto binding = routing.targets.AddWorkspace();
    if (!binding.has_value()) {
        return nullptr;
    }
    std::uint32_t slot{};
    for (; slot < WorkspaceWindowRegistry::kMaximumWindows; ++slot) {
        bool used{};
        for (std::size_t index = 0U; index < workspaces_.Count(); ++index) {
            const WorkspaceWindow* workspace = workspaces_.At(index);
            used = used || (workspace != nullptr
                && workspace->persistence_slot == slot);
        }
        if (!used) {
            break;
        }
    }
    if (slot >= WorkspaceWindowRegistry::kMaximumWindows) {
        (void)routing.targets.RemoveWorkspace(binding->workspace);
        (void)routing.targets.ActivateWorkspace(previous);
        return nullptr;
    }
    if (!workspaces_.Add(
            this,
            binding->workspace,
            binding->editor_group,
            binding->canvas,
            routing.targets.CurrentGeneration(),
            slot)) {
        (void)routing.targets.RemoveWorkspace(binding->workspace);
        (void)routing.targets.ActivateWorkspace(previous);
        return nullptr;
    }
    WorkspaceWindow* workspace = workspaces_.Find(binding->workspace);
    if (workspace == nullptr || !RegisterWorkspacePanes(*workspace)) {
        (void)workspaces_.Remove(binding->workspace);
        (void)routing.targets.RemoveWorkspace(binding->workspace);
        (void)routing.targets.ActivateWorkspace(previous);
        (void)workspaces_.Activate(previous, false);
        return nullptr;
    }
    return workspace;
}

bool ApplicationHost::ActivateWorkspaceWindow(
    WorkspaceWindowId id, bool record_focus) noexcept {
    const bool already_current = routing.targets.Workspace() == id;
    const auto previous_active_view = routing.targets.ActiveDocumentView();
    WorkspaceWindow* workspace = workspaces_.Find(id);
    if (workspace == nullptr
        || !routing.targets.ActivateWorkspace(id)
        || !workspaces_.Activate(id, record_focus)) {
        return false;
    }
    BindWorkspacePaneAliases(*workspace);
    EditorGroup* group = workspace->editors.Active();
    const DocumentViewId view = group == nullptr
        ? DocumentViewId{}
        : group->ActiveView();
    DocumentSession* document = documents_.FindByView(view);
    DocumentView* document_view = document == nullptr
        ? nullptr
        : document->FindView(view);
    if (document == nullptr || document_view == nullptr) {
        return document == nullptr && document_view == nullptr
            && group != nullptr && ActivateEmptyEditorGroup(group->id);
    }
    if (!documents_.Activate(document->id) || !document->ActivateView(view)) {
        return false;
    }
    if (engine == nullptr) {
        return true;
    }
    if (!engine->SetActiveSession(document->id, document->generation)
        || engine->SetActiveView(document_view->core_view_id)
            != INKPOD_STATUS_OK) {
        return false;
    }
    return (already_current && previous_active_view == view)
        || RefreshEditorPresentation(document->id, document->generation);
}

bool ApplicationHost::RemoveWorkspaceWindow(WorkspaceWindowId id) noexcept {
    WorkspaceWindow* workspace = workspaces_.Find(id);
    if (workspace == nullptr || workspaces_.Count() <= 1U) {
        return false;
    }
    for (std::size_t index = 0U; index < workspace->editors.GroupCount(); ++index) {
        const EditorGroup* group = workspace->editors.GroupAt(index);
        if (group == nullptr || group->ViewCount() != 0U) {
            return false;
        }
    }
    UnregisterWorkspacePanes(*workspace);
    if (!routing.targets.RemoveWorkspace(id)
        || !workspaces_.Remove(id)) {
        return false;
    }
    WorkspaceWindow* current = workspaces_.Current();
    return current != nullptr
        && ActivateWorkspaceWindow(current->id, false);
}

bool ApplicationHost::MoveDocumentViewToWorkspace(
    DocumentViewId view, WorkspaceWindowId destination) noexcept {
    WorkspaceWindow* target = workspaces_.Find(destination);
    EditorGroup* target_group = target == nullptr
        ? nullptr
        : target->editors.Active();
    return target_group != nullptr
        && MoveDocumentView(
            view,
            destination,
            target_group->id,
            target_group->ViewCount());
}

bool ApplicationHost::MoveDocumentView(
    DocumentViewId view,
    WorkspaceWindowId destination_workspace,
    EditorGroupId destination_group,
    std::size_t insertion_index) noexcept {
    WorkspaceWindow* source = WorkspaceForView(view);
    WorkspaceWindow* target = workspaces_.Find(destination_workspace);
    EditorGroup* source_group = source == nullptr
        ? nullptr
        : source->editors.FindByView(view);
    EditorGroup* target_group = target == nullptr
        ? nullptr
        : target->editors.Find(destination_group);
    DocumentSession* document = documents_.FindByView(view);
    if (source == nullptr || target == nullptr || source_group == nullptr
        || target_group == nullptr || document == nullptr
        || (target_group->ViewCount() >= EditorGroup::kMaximumViews
            && source_group != target_group)
        || insertion_index > target_group->ViewCount()) {
        return false;
    }
    const WorkspaceWindowId source_workspace = source->id;
    const EditorGroupId source_group_id = source_group->id;
    const EditorArea source_before = source->editors;
    const EditorArea target_before = target->editors;
    const CommandContext previous = routing.targets.Capture();
    const bool placed = source == target
        ? source->editors.MoveView(view, destination_group, insertion_index)
        : source->editors.RemoveView(view)
            && target_group->InsertView(view, insertion_index);
    if (!placed) {
        source->editors = source_before;
        if (source != target) {
            target->editors = target_before;
        }
        return false;
    }
    if (source_group == target_group) {
        if (ActivateDocumentView(view)) {
            return true;
        }
        source->editors = source_before;
        return false;
    }
    if (!routing.targets.MoveDocumentView(view, destination_group)) {
        source->editors = source_before;
        if (source != target) {
            target->editors = target_before;
        }
        return false;
    }
    const auto bind_group = [this](WorkspaceWindow& workspace, EditorGroupId id) {
        EditorGroup* group = workspace.editors.Find(id);
        if (group == nullptr || group->canvas == nullptr) {
            return group != nullptr;
        }
        const DocumentSession* active = documents_.FindByView(group->ActiveView());
        return active == nullptr
            ? UnbindDocumentCanvas(group->canvas)
            : BindDocumentCanvas(group->canvas, *active, group->ActiveView());
    };
    if (bind_group(*source, source_group_id)
        && bind_group(*target, destination_group)
        && ActivateDocumentView(view)) {
        return true;
    }

    (void)routing.targets.MoveDocumentView(view, source_group_id);
    source->editors = source_before;
    if (source != target) {
        target->editors = target_before;
    }
    (void)bind_group(*source, source_group_id);
    if (source != target || source_group_id != destination_group) {
        (void)bind_group(*target, destination_group);
    }
    if (previous.workspace.has_value()) {
        (void)ActivateWorkspaceWindow(previous.workspace.value(), false);
    }
    if (previous.document_view.has_value()) {
        (void)ActivateDocumentView(previous.document_view.value());
    } else {
        (void)ActivateWorkspaceWindow(source_workspace, false);
    }
    return false;
}

TabDragCoordinator& ApplicationHost::TabDrag() noexcept {
    return tab_drag_;
}

const TabDragCoordinator& ApplicationHost::TabDrag() const noexcept {
    return tab_drag_;
}

DocumentSession& ApplicationHost::Document() noexcept {
    assert(documents_.Current() != nullptr);
    return *documents_.Current();
}

const DocumentSession& ApplicationHost::Document() const noexcept {
    assert(documents_.Current() != nullptr);
    return *documents_.Current();
}

DocumentView& ApplicationHost::ActiveView() noexcept {
    assert(Document().ActiveView() != nullptr);
    return *Document().ActiveView();
}

const DocumentView& ApplicationHost::ActiveView() const noexcept {
    assert(Document().ActiveView() != nullptr);
    return *Document().ActiveView();
}

bool ApplicationHost::ReplaceDocumentSession(
    DocumentSessionId id,
    Generation generation,
    DocumentViewId initial_view) noexcept {
    if (id != routing.targets.DocumentSession()
        || generation != routing.targets.CurrentGeneration()
        || initial_view != routing.targets.ActiveDocumentView()
        || engine == nullptr) {
        return false;
    }
    DocumentSession& current = Document();
    const DocumentSessionId old_id = current.id;
    const Generation old_generation = current.generation;
    const bool had_core = old_id && old_generation
        && engine->HasSession(old_id, old_generation);
    const InkpodStatus binding_status = had_core
        ? engine->RebindSession(old_id, old_generation, id, generation)
        : engine->CreateSession(id, generation);
    if (binding_status != INKPOD_STATUS_OK) {
        return false;
    }
    if (!engine->RegisterDocumentView(id, generation, initial_view, 0U)
        || !documents_.Replace(id, generation, initial_view, engine.get())
        || !Workspace().editors.ResetViews(initial_view)) {
        (void)engine->UnregisterDocumentView(id, generation, initial_view);
        if (had_core) {
            (void)engine->RebindSession(id, generation, old_id, old_generation);
        } else {
            (void)engine->CloseSession(id, generation);
        }
        return false;
    }
    if (!engine->SetActiveSession(id, generation)) {
        return false;
    }
    return Workspace().windows.canvas == nullptr
        || BindDocumentCanvas(Workspace().windows.canvas, Document(), initial_view);
}

DocumentRegistry& ApplicationHost::Documents() noexcept {
    return documents_;
}

const DocumentRegistry& ApplicationHost::Documents() const noexcept {
    return documents_;
}

std::optional<ApplicationHost::DocumentBinding>
ApplicationHost::AddDocumentSession() noexcept {
    if (engine == nullptr) {
        return std::nullopt;
    }
    const CommandContext previous = routing.targets.Capture();
    const auto issued = routing.targets.AddDocument();
    if (!issued.has_value()) {
        return std::nullopt;
    }
    const DocumentBinding binding{
        issued.value(),
        routing.targets.ActiveDocumentView(),
        routing.targets.CurrentGeneration()};
    if (engine->CreateSession(binding.session, binding.generation)
            != INKPOD_STATUS_OK
        || !engine->RegisterDocumentView(
            binding.session, binding.generation, binding.view, 0U)
        || !documents_.Add(
            binding.session,
            binding.generation,
            binding.view,
            engine.get())
        || !Workspace().editors.AddView(
            routing.targets.EditorGroup(), binding.view)) {
        (void)engine->UnregisterDocumentView(
            binding.session, binding.generation, binding.view);
        (void)documents_.Remove(binding.session);
        (void)engine->CloseSession(binding.session, binding.generation);
        (void)routing.targets.RemoveDocument(binding.session);
        if (previous.document_session.has_value()
            && previous.document_view.has_value()) {
            (void)routing.targets.ActivateDocument(
                previous.document_session.value(),
                previous.document_view.value());
        }
        return std::nullopt;
    }
    if (!ActivateDocumentView(binding.view)) {
        (void)Workspace().editors.RemoveView(binding.view);
        (void)documents_.Remove(binding.session);
        (void)engine->CloseSession(binding.session, binding.generation);
        (void)routing.targets.RemoveDocument(binding.session);
        if (previous.document_session.has_value()
            && previous.document_view.has_value()) {
            (void)ActivateDocumentView(previous.document_view.value());
        }
        return std::nullopt;
    }
    return binding;
}

std::optional<ApplicationHost::DocumentBinding>
ApplicationHost::PrepareDocumentSession() noexcept {
    if (engine == nullptr) {
        return std::nullopt;
    }
    const CommandContext previous = routing.targets.Capture();
    const auto issued = routing.targets.AddDocument();
    if (!issued.has_value()) {
        return std::nullopt;
    }
    const DocumentBinding binding{
        issued.value(),
        routing.targets.ActiveDocumentView(),
        routing.targets.CurrentGeneration()};
    if (engine->CreateSession(binding.session, binding.generation)
            != INKPOD_STATUS_OK
        || !engine->RegisterDocumentView(
            binding.session, binding.generation, binding.view, 0U)) {
        (void)engine->UnregisterDocumentView(
            binding.session, binding.generation, binding.view);
        (void)engine->CloseSession(binding.session, binding.generation);
        (void)routing.targets.RemoveDocument(binding.session);
        if (previous.document_session.has_value()
            && previous.document_view.has_value()) {
            (void)routing.targets.ActivateDocument(
                previous.document_session.value(),
                previous.document_view.value());
        }
        return std::nullopt;
    }
    if (previous.document_session.has_value()
        && previous.document_view.has_value()
        && !routing.targets.ActivateDocument(
            previous.document_session.value(), previous.document_view.value())) {
        (void)DiscardPreparedDocumentSession(binding);
        return std::nullopt;
    }
    if (!previous.document_view.has_value()) {
        routing.targets.ClearActiveDocument();
        engine->ClearActiveSession();
    }
    return binding;
}

std::optional<ApplicationHost::DocumentBinding>
ApplicationHost::PrepareBatchResultSession(
    InkpodBatchReport* report,
    std::uint64_t result_index) noexcept {
    if (engine == nullptr || report == nullptr) {
        return std::nullopt;
    }
    const CommandContext previous = routing.targets.Capture();
    const auto issued = routing.targets.AddDocument();
    if (!issued.has_value()) {
        return std::nullopt;
    }
    const DocumentBinding binding{
        issued.value(),
        routing.targets.ActiveDocumentView(),
        routing.targets.CurrentGeneration()};
    if (engine->AdoptBatchResult(
            binding.session, binding.generation, report, result_index)
            != INKPOD_STATUS_OK
        || !engine->RegisterDocumentView(
            binding.session, binding.generation, binding.view, 0U)) {
        (void)engine->UnregisterDocumentView(
            binding.session, binding.generation, binding.view);
        (void)engine->CloseSession(binding.session, binding.generation);
        (void)routing.targets.RemoveDocument(binding.session);
        if (previous.document_session.has_value()
            && previous.document_view.has_value()) {
            (void)routing.targets.ActivateDocument(
                previous.document_session.value(), previous.document_view.value());
        }
        return std::nullopt;
    }
    if (previous.document_session.has_value()
        && previous.document_view.has_value()
        && !routing.targets.ActivateDocument(
            previous.document_session.value(), previous.document_view.value())) {
        (void)DiscardPreparedDocumentSession(binding);
        return std::nullopt;
    }
    if (!previous.document_view.has_value()) {
        routing.targets.ClearActiveDocument();
        engine->ClearActiveSession();
    }
    return binding;
}

bool ApplicationHost::PublishPreparedDocumentSession(
    const DocumentBinding& binding,
    EditorGroupId destination_group) noexcept {
    if (engine == nullptr
        || !engine->HasSession(binding.session, binding.generation)
        || documents_.Find(binding.session) != nullptr
        || !documents_.Add(
            binding.session,
            binding.generation,
            binding.view,
            engine.get())) {
        return false;
    }
    if (!Workspace().editors.AddView(destination_group, binding.view)) {
        (void)documents_.Remove(binding.session);
        return false;
    }
    return true;
}

bool ApplicationHost::DiscardPreparedDocumentSession(
    const DocumentBinding& binding) noexcept {
    if (engine == nullptr || documents_.Find(binding.session) != nullptr) {
        return false;
    }
    const bool unregistered = engine->UnregisterDocumentView(
        binding.session, binding.generation, binding.view);
    const bool closed = engine->CloseSession(
        binding.session, binding.generation) == INKPOD_STATUS_OK;
    const bool removed = routing.targets.RemoveDocument(binding.session);
    return unregistered && closed && removed;
}

bool ApplicationHost::ActivateDocumentView(DocumentViewId view) noexcept {
    DocumentSession* document = documents_.FindByView(view);
    DocumentView* target = document == nullptr ? nullptr : document->FindView(view);
    WorkspaceWindow* target_workspace = WorkspaceForView(view);
    EditorGroup* target_group = target_workspace == nullptr
        ? nullptr
        : target_workspace->editors.FindByView(view);
    if (document == nullptr || target == nullptr || target_workspace == nullptr
        || target_group == nullptr
        || engine == nullptr) {
        return false;
    }
    const auto* target_sink = target_group->canvas == nullptr
        ? nullptr : renderer::GetCanvasSnapshotSink(target_group->canvas);
    const renderer::SnapshotRoute target_route{
        document->id,
        target->id,
        target_group->canvas_id,
        document->generation,
        target_group->generation};
    // Inserting a new view updates model selection before its Canvas binding.
    // Only the actual matching destination can take the repeated-view path.
    if (routing.targets.ActiveDocumentView() == view
        && routing.targets.Workspace() == target_workspace->id
        && documents_.Current() == document
        && target_group->ActiveView() == view
        && target_sink != nullptr && target_sink->Route() == target_route) {
        return engine->SetActiveSession(document->id, document->generation)
            && engine->SetActiveView(target->core_view_id) == INKPOD_STATUS_OK;
    }
    WorkspaceWindow* previous_workspace = workspaces_.Current();
    DocumentSession* previous_document = documents_.Current();
    DocumentView* previous_view = previous_document == nullptr
        ? nullptr
        : previous_document->ActiveView();
    EditorGroup* previous_group = previous_workspace == nullptr
        ? nullptr
        : previous_workspace->editors.Active();
    renderer::CancelCanvasStroke(
        previous_group == nullptr ? nullptr : previous_group->canvas);
    if (!engine->SetActiveSession(document->id, document->generation)
        || (target_group->canvas != nullptr
            && !BindDocumentCanvas(target_group->canvas, *document, target->id))
        || engine->SetActiveView(target->core_view_id) != INKPOD_STATUS_OK) {
        if (previous_document != nullptr && previous_view != nullptr) {
            (void)engine->SetActiveSession(
                previous_document->id, previous_document->generation);
            if (previous_group != nullptr && previous_group->canvas != nullptr) {
                (void)BindDocumentCanvas(
                    previous_group->canvas, *previous_document, previous_view->id);
            }
            (void)engine->SetActiveView(previous_view->core_view_id);
        }
        return false;
    }
    const bool activated = workspaces_.Activate(target_workspace->id, false)
        && target_workspace->editors.Activate(target_group->id)
        && target_group->ActivateView(view)
        && documents_.Activate(document->id)
        && document->ActivateView(view)
        && routing.targets.ActivateDocument(document->id, view);
    if (activated) {
        BindWorkspacePaneAliases(*target_workspace);
        target_workspace->windows.canvas = target_group->canvas;
        target_workspace->windows.document_tabs = target_group->document_tabs;
    }
    if (!activated) {
        return false;
    }
    return RefreshEditorPresentation(document->id, document->generation);
}

bool ApplicationHost::ActivateEmptyEditorGroup(EditorGroupId group_id) noexcept {
    WorkspaceWindow* workspace = workspaces_.Find(
        routing.targets.WorkspaceForGroup(group_id));
    EditorGroup* group = workspace == nullptr ? nullptr : workspace->editors.Find(group_id);
    if (group == nullptr || group->ActiveView()
        || !workspaces_.Activate(workspace->id, false)
        || !workspace->editors.Activate(group_id)
        || !routing.targets.ActivateEditorGroup(group_id)) {
        return false;
    }
    routing.targets.ClearActiveDocument();
    documents_.ClearActive();
    if (engine != nullptr) {
        engine->ClearActiveSession();
    }
    BindWorkspacePaneAliases(*workspace);
    workspace->windows.canvas = group->canvas;
    workspace->windows.document_tabs = group->document_tabs;
    workspace->tools.editor = {};
    workspace->tools.procedure = {};
    return true;
}

bool ApplicationHost::RefreshEditorPresentation(
    DocumentSessionId session, Generation generation, bool refresh_core) noexcept {
    DocumentSession* document = documents_.Find(session);
    if (engine == nullptr || document == nullptr
        || document->generation != generation) {
        return false;
    }
    const InkpodStatus refresh_status = refresh_core
        ? engine->RefreshEditorState(session, generation) : INKPOD_STATUS_OK;
    if (refresh_status == INKPOD_STATUS_NO_DOCUMENT) {
        document->editor_presentation = {};
        document->editor_presentation.struct_size =
            sizeof(document->editor_presentation);
        document->has_editor_presentation = false;
        for (std::size_t index = 0U; index < document->ViewCount(); ++index) {
            const DocumentView* view = document->ViewAt(index);
            WorkspaceWindow* workspace = view == nullptr
                ? nullptr
                : WorkspaceForView(view->id);
            if (workspace != nullptr
                && WorkspaceActivelyPresentsView(*workspace, view->id)) {
                workspace->tools.editor = {};
                workspace->tools.procedure = {};
            }
        }
        return true;
    }
    if (refresh_status != INKPOD_STATUS_OK) {
        return false;
    }
    InkpodEditorStateInfo editor{};
    editor.struct_size = sizeof(editor);
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    if (!engine->GetEditorState(session, generation, editor)) {
        return false;
    }
    const InkpodDocumentInfo* info_ptr =
        engine->GetDocumentInfo(session, generation, info) ? &info : nullptr;
    document->editor_presentation = editor;
    document->has_editor_presentation = true;

    bool projected = true;
    for (std::size_t index = 0U; index < document->ViewCount(); ++index) {
        const DocumentView* view = document->ViewAt(index);
        WorkspaceWindow* workspace = view == nullptr
            ? nullptr
            : WorkspaceForView(view->id);
        if (workspace != nullptr
            && WorkspaceActivelyPresentsView(*workspace, view->id)) {
            projected = ProjectEditorPresentation(
                            editor, info_ptr, session, generation, *workspace)
                && projected;
        }
    }
    return projected;
}

void ApplicationHost::ObserveCanvasSequencePresentation(HWND canvas) noexcept {
    if (renderer == nullptr) {
        return;
    }
    const auto* sink = renderer::GetCanvasSnapshotSink(canvas);
    const auto route = sink == nullptr ? renderer::SnapshotRoute{} : sink->Route();
    if (!route || route.owner_kind != renderer::SnapshotOwnerKind::Document) {
        return;
    }
    auto* document = documents_.Find(route.document_session);
    if (document == nullptr || document->generation != route.document_generation
        || document->sequence_activation_pending
        || document->sequence_required_present_epoch == 0U
        || document->HasSequencePresentationAcknowledgement()) {
        return;
    }
    renderer::RendererSurfaceResourceUsage presented{};
    if (renderer->GetSurfaceResourceUsage(route.canvas, route.surface_generation, presented)
        && presented.route == route) {
        (void)document->AcknowledgeSequencePresentation(
            route.document_session, route.document_generation, route.document_view,
            presented.last_presented_document_revision,
            presented.last_presented_presentation_epoch);
    }
}

bool ApplicationHost::BindDocumentCanvas(
    HWND canvas, const DocumentSession& document, DocumentViewId view) noexcept {
    const auto* sink = renderer::GetCanvasSnapshotSink(canvas);
    const auto previous = sink == nullptr ? renderer::SnapshotRoute{} : sink->Route();
    const bool changed = previous.document_session != document.id
        || previous.document_generation != document.generation
        || previous.document_view != view;
    if (changed) {
        ObserveCanvasSequencePresentation(canvas);
    }
    return renderer::BindCanvasSnapshotSink(
               canvas, document.id, view, document.generation)
        && renderer::SetCanvasSequenceFence(canvas,
            document.sequence_activation_pending,
            document.sequence_required_present_revision,
            document.sequence_required_present_epoch)
        && (!changed || document.Core() == nullptr
            || document.Core()->InvalidateViewPublication(document.id, document.generation));
}

bool ApplicationHost::UnbindDocumentCanvas(HWND canvas) noexcept {
    ObserveCanvasSequencePresentation(canvas);
    return renderer::UnbindCanvasSnapshotSink(canvas);
}

bool ApplicationHost::SequenceEditReady(const CommandContext& context) const noexcept {
    if (!context.document_session.has_value()) {
        return true;
    }
    if (routing.targets.Resolve(context, CommandTargetScope::DocumentSession)
        != CommandResolveStatus::Ok) {
        return false;
    }
    const DocumentSession* document = documents_.Find(context.document_session.value());
    if (document == nullptr || document->generation != context.generation
        || document->sequence_activation_pending) {
        return false;
    }
    if ((document->sequence_required_present_revision == 0U
            && document->sequence_required_present_epoch == 0U)
        || document->HasSequencePresentationAcknowledgement()) {
        return true;
    }
    if (!context.document_view.has_value() || renderer == nullptr) {
        return false;
    }
    const WorkspaceWindow* workspace = WorkspaceForView(context.document_view.value());
    const EditorGroup* group = workspace == nullptr ? nullptr
        : workspace->editors.FindByView(context.document_view.value());
    renderer::RendererSurfaceResourceUsage presented{};
    return group != nullptr && group->ActiveView() == context.document_view.value()
        && renderer->GetSurfaceResourceUsage(group->canvas_id, group->generation, presented)
        && presented.route.document_session == document->id
        && presented.route.document_generation == document->generation
        && presented.route.document_view == context.document_view.value()
        && presented.last_presented_document_revision
            >= document->sequence_required_present_revision
        && (document->sequence_required_present_epoch == 0U
            || presented.last_presented_presentation_epoch
                == document->sequence_required_present_epoch);
}

InkpodStatus ApplicationHost::UpdateEditorState(
    const InkpodEditorStateUpdate& update) noexcept {
    if (!SequenceEditReady(routing.targets.Capture())) {
        return INKPOD_STATUS_CANCELLED;
    }
    DocumentSession* document = documents_.Current();
    WorkspaceWindow* workspace = workspaces_.Current();
    if (engine == nullptr || document == nullptr || workspace == nullptr
        || !workspace->tools.editor.valid
        || workspace->tools.editor.session != document->id
        || workspace->tools.editor.generation != document->generation
        || workspace->tools.editor.editor_revision
            != update.expected_editor_revision) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const DocumentSessionId session = document->id;
    const Generation generation = document->generation;
    const InkpodStatus status = engine->UpdateEditorState(session, generation, update);
    if (status != INKPOD_STATUS_OK) {
        // A rejected stale edit must leave the request rejected, but expose
        // the authoritative editor state to the same session's UI. Repeated
        // activation no longer supplies that refresh as an incidental effect.
        (void)RefreshEditorPresentation(session, generation);
        return status;
    }
    return RefreshEditorPresentation(session, generation)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

bool ApplicationHost::CloseDocumentView(DocumentViewId view) noexcept {
    DocumentSession* document = documents_.FindByView(view);
    if (document == nullptr || document->ViewCount() <= 1U || engine == nullptr) {
        return false;
    }
    const DocumentView* closing = document->FindView(view);
    WorkspaceWindow* source_workspace = WorkspaceForView(view);
    EditorGroup* closing_group = source_workspace == nullptr
        ? nullptr : source_workspace->editors.FindByView(view);
    if (closing == nullptr || closing_group == nullptr) {
        return false;
    }
    const bool was_visible = closing_group->ActiveView() == view;
    if (was_visible) {
        ObserveCanvasSequencePresentation(closing_group->canvas);
    }
    const InkpodStatus status = engine->Invoke(
        document->id,
        document->generation,
        [core_view_id = closing->core_view_id](InkpodCore* core) {
            return inkpod_core_view_close(core, core_view_id);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK
        || !engine->UnregisterDocumentView(document->id, document->generation, view)
        || !source_workspace->editors.RemoveView(view)
        || !document->RemoveView(view)
        || !routing.targets.RemoveDocumentView(view)) {
        return false;
    }
    if (was_visible && closing_group->canvas != nullptr) {
        const auto* remaining = documents_.FindByView(closing_group->ActiveView());
        const bool rebound = remaining == nullptr
            ? UnbindDocumentCanvas(closing_group->canvas)
            : BindDocumentCanvas(
                closing_group->canvas, *remaining, closing_group->ActiveView());
        if (!rebound) {
            return false;
        }
    }
    EditorGroup* active_group = source_workspace->editors.Active();
    DocumentViewId replacement = active_group == nullptr
        ? DocumentViewId{} : active_group->ActiveView();
    if (!replacement && active_group != nullptr) {
        const EditorGroup* other = source_workspace->editors.Other(active_group->id);
        if (other != nullptr) {
            replacement = other->ActiveView();
        }
    }
    return replacement ? ActivateDocumentView(replacement)
        : active_group != nullptr && ActivateEmptyEditorGroup(active_group->id);
}

bool ApplicationHost::CloseDocumentSession(DocumentSessionId session) noexcept {
    DocumentSession* document = documents_.Find(session);
    if (document == nullptr || engine == nullptr) {
        return false;
    }
    const Generation generation = document->generation;
    windows::ui::CloseHistoryVisualizationDialog(*document);
    std::array<EditorGroup*,
        WorkspaceWindowRegistry::kMaximumWindows * EditorArea::kMaximumGroups> affected{};
    std::size_t affected_count{};
    for (std::size_t index = 0U; index < workspaces_.Count(); ++index) {
        WorkspaceWindow* workspace = workspaces_.At(index);
        for (std::size_t group_index = 0U;
             workspace != nullptr && group_index < workspace->editors.GroupCount();
             ++group_index) {
            EditorGroup* group = workspace->editors.GroupAt(group_index);
            if (group != nullptr && document->FindView(group->ActiveView()) != nullptr) {
                renderer::CancelCanvasStroke(group->canvas);
                affected[affected_count++] = group;
            }
        }
    }
    if (engine->CloseSession(session, generation) != INKPOD_STATUS_OK) {
        return false;
    }
    thumbnails_.RemoveDocument(session, generation);
    routing.pane_targets.DocumentClosed(session);
    for (std::size_t view_index = document->ViewCount(); view_index > 0U; --view_index) {
        const DocumentView* view = document->ViewAt(view_index - 1U);
        WorkspaceWindow* workspace = view == nullptr ? nullptr : WorkspaceForView(view->id);
        if (view != nullptr && workspace != nullptr) {
            (void)workspace->editors.RemoveView(view->id);
        }
    }
    if (!documents_.Remove(session) || !routing.targets.RemoveDocument(session)) {
        return false;
    }
    for (std::size_t index = 0U; index < affected_count; ++index) {
        EditorGroup* group = affected[index];
        const DocumentSession* remaining = documents_.FindByView(group->ActiveView());
        if (group->canvas != nullptr) {
            const bool rebound = remaining == nullptr
                ? UnbindDocumentCanvas(group->canvas)
                : BindDocumentCanvas(group->canvas, *remaining, group->ActiveView());
            if (!rebound) {
                return false;
            }
        }
    }
    WorkspaceWindow* workspace = workspaces_.Current();
    EditorGroup* active_group = workspace == nullptr ? nullptr : workspace->editors.Active();
    DocumentViewId replacement = active_group == nullptr
        ? DocumentViewId{} : active_group->ActiveView();
    if (!replacement && workspace != nullptr) {
        for (std::size_t index = 0U; index < workspace->editors.GroupCount(); ++index) {
            const EditorGroup* group = workspace->editors.GroupAt(index);
            if (group != nullptr && group->ActiveView()) {
                replacement = group->ActiveView();
                break;
            }
        }
    }
    return replacement ? ActivateDocumentView(replacement)
        : active_group != nullptr && ActivateEmptyEditorGroup(active_group->id);
}

std::uint32_t ApplicationHost::IssueUntitledNumber() noexcept {
    const std::uint32_t result = next_untitled_number_++;
    if (next_untitled_number_ == 0U) {
        next_untitled_number_ = 1U;
    }
    return result == 0U ? next_untitled_number_++ : result;
}

bool ApplicationHost::RecordRecentDocument(
    std::wstring path,
    DocumentIdentity identity) noexcept {
    return recent_documents_.Record(
        std::move(path), std::move(identity));
}

bool ApplicationHost::RecordRecentDocumentReplacing(
    std::wstring path,
    DocumentIdentity identity,
    const DocumentIdentity& previous_identity) noexcept {
    return recent_documents_.RecordReplacing(
        std::move(path), std::move(identity), previous_identity);
}

bool ApplicationHost::RemoveRecentDocument(std::size_t index) noexcept {
    return recent_documents_.Remove(index);
}

const RecentDocumentEntry* ApplicationHost::RecentDocumentAt(
    std::size_t index) const noexcept {
    return recent_documents_.At(index);
}

std::size_t ApplicationHost::RecentDocumentCount() const noexcept {
    return recent_documents_.Count();
}

void ApplicationHost::DetachCoreSessions() noexcept {
    documents_.ClearCoreBindings();
}

}  // namespace inkpod::app
