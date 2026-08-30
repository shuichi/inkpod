#include <array>
#include <cstdlib>
#include <cstdint>
#include <cwchar>
#include <cwctype>
#include <iostream>
#include <new>
#include <string>

#include <windows.h>

#include "ui/dock_layout.h"
#include "ui/dock_host.h"
#include "ui/right_tool_tabs.h"

#include "app/application_owner_graph.h"
#include "app/document_session.h"
#include "app/recent_documents.h"
#include "app/workspace_window.h"
#include "ui/thumbnail_cache.h"

namespace {

std::int64_t g_allocations_before_failure{-1};
std::size_t g_live_allocations{};

}  // namespace

void* operator new(std::size_t size) {
    if (g_allocations_before_failure == 0) {
        g_allocations_before_failure = -1;
        throw std::bad_alloc{};
    }
    if (g_allocations_before_failure > 0) {
        --g_allocations_before_failure;
    }
    void* allocation = std::malloc(size);
    if (allocation == nullptr) {
        throw std::bad_alloc{};
    }
    ++g_live_allocations;
    return allocation;
}

void operator delete(void* allocation) noexcept {
    if (allocation != nullptr) {
        --g_live_allocations;
        std::free(allocation);
    }
}

void operator delete(void* allocation, std::size_t) noexcept {
    ::operator delete(allocation);
}

namespace {

using inkpod::app::ApplicationHost;
using inkpod::app::CoreHost;
using inkpod::app::DocumentRegistry;
using inkpod::app::DocumentIdentity;
using inkpod::app::DocumentSessionId;
using inkpod::app::DocumentViewId;
using inkpod::app::EditorArea;
using inkpod::app::EditorGroupId;
using inkpod::app::EditorSplitOrientation;
using inkpod::app::Generation;
using inkpod::app::RecentDocumentList;
using inkpod::app::WorkspaceWindowId;
using inkpod::app::WorkspaceWindowRegistry;
using inkpod::windows::ui::DockHost;
using inkpod::windows::ui::DockLayoutModel;
using inkpod::windows::ui::DockLayoutRecord;
using inkpod::windows::ui::DockPanePlacement;
using inkpod::windows::ui::DockPaneType;
using inkpod::windows::ui::DockResult;
using inkpod::windows::ui::DockZoneState;
using inkpod::windows::ui::RightToolTabsModel;
using inkpod::windows::ui::ThumbnailCache;
using inkpod::windows::ui::ThumbnailCacheKey;
using inkpod::windows::ui::ThumbnailImageView;
using inkpod::windows::ui::ThumbnailKind;
using inkpod::windows::ui::ThumbnailPaneUsage;
using inkpod::windows::ui::ThumbnailPixelLayout;
using inkpod::windows::ui::ToolTabId;
using inkpod::windows::ui::ToolTabResult;

// Explicit-instantiation access keeps this implementation-local regression on
// the real private command boundary without adding a production test accessor.
template <typename Tag, typename Tag::type Member>
struct DockHostPrivateAccess final {
    friend typename Tag::type AccessDockHostMember(Tag) noexcept {
        return Member;
    }
};

struct DockHostModelTag final {
    using type = DockLayoutModel* DockHost::*;
    friend type AccessDockHostMember(DockHostModelTag) noexcept;
};

struct DockHostRightToolTabsTag final {
    using type = RightToolTabsModel* DockHost::*;
    friend type AccessDockHostMember(DockHostRightToolTabsTag) noexcept;
};

struct DockHostMovePaneToToolTabTag final {
    using type = ToolTabResult (DockHost::*)(
        DockPaneType, ToolTabId) noexcept;
    friend type AccessDockHostMember(DockHostMovePaneToToolTabTag) noexcept;
};

template struct DockHostPrivateAccess<DockHostModelTag, &DockHost::model_>;
template struct DockHostPrivateAccess<
    DockHostRightToolTabsTag, &DockHost::right_tool_tabs_>;
template struct DockHostPrivateAccess<
    DockHostMovePaneToToolTabTag, &DockHost::MovePaneToToolTab>;

bool TestApplicationWideThumbnailLru() {
    g_allocations_before_failure = -1;
    ThumbnailCache cache;
    if (cache.SetBudgetBytes(0U) || !cache.SetBudgetBytes(32U)) {
        return false;
    }
    const ThumbnailCacheKey first{
        inkpod::app::PaneInstanceId{1U},
        DocumentSessionId{11U},
        Generation{21U},
        31U,
        41U,
        ThumbnailKind::Layer};
    const ThumbnailCacheKey second{
        inkpod::app::PaneInstanceId{2U},
        DocumentSessionId{12U},
        Generation{22U},
        32U,
        42U,
        ThumbnailKind::Sequence};
    const ThumbnailCacheKey third{
        inkpod::app::PaneInstanceId{3U},
        DocumentSessionId{13U},
        Generation{23U},
        33U,
        43U,
        ThumbnailKind::Layer};
    const std::vector<std::uint8_t> first_pixels(16U, 1U);
    const std::vector<std::uint8_t> second_pixels(16U, 2U);
    const std::vector<std::uint8_t> third_pixels(16U, 3U);
    const auto initial_sequence_generation =
        cache.InvalidationGeneration(ThumbnailKind::Sequence);
    const auto initial_layer_generation =
        cache.InvalidationGeneration(ThumbnailKind::Layer);
    if (!cache.Put(
            first, 2U, 2U, 8U, ThumbnailPixelLayout::Bgra8, first_pixels)
        || !cache.Put(
            second, 2U, 2U, 8U, ThumbnailPixelLayout::Rgba8, second_pixels)
        || cache.InvalidationGeneration(ThumbnailKind::Sequence)
            != initial_sequence_generation
        || cache.InvalidationGeneration(ThumbnailKind::Layer)
            != initial_layer_generation) {
        return false;
    }
    ThumbnailImageView image{};
    if (!cache.Get(first, image) || image.pixels.size() != 16U
        || image.pixels[0] != 1U
        || !cache.Put(
            third, 2U, 2U, 8U, ThumbnailPixelLayout::Bgra8, third_pixels)) {
        return false;
    }
    if (cache.Peek(second, image) || !cache.Peek(first, image)
        || !cache.Peek(third, image)) {
        return false;
    }
    const auto after_eviction = cache.Usage();
    if (after_eviction.budget_bytes != 32U
        || after_eviction.resident_bytes != 32U
        || after_eviction.entry_count != 2U
        || after_eviction.hit_count != 1U
        || after_eviction.eviction_count != 1U
        || cache.InvalidationGeneration(ThumbnailKind::Sequence)
            == initial_sequence_generation
        || cache.InvalidationGeneration(ThumbnailKind::Layer)
            != initial_layer_generation) {
        return false;
    }
    const auto evicted_sequence_generation =
        cache.InvalidationGeneration(ThumbnailKind::Sequence);

    // Re-inserting identical content is a touch/no-op and must not create a
    // duplicate or consume additional budget.
    if (!cache.Put(
            first, 2U, 2U, 8U, ThumbnailPixelLayout::Bgra8, first_pixels)
        || cache.Usage().resident_bytes != 32U
        || cache.Usage().entry_count != 2U
        || cache.Put(
            ThumbnailCacheKey{},
            2U,
            2U,
            8U,
            ThumbnailPixelLayout::Bgra8,
            first_pixels)
        || cache.Put(
            second,
            4U,
            4U,
            16U,
            ThumbnailPixelLayout::Rgba8,
            std::vector<std::uint8_t>(64U, 4U))) {
        return false;
    }

    ThumbnailPaneUsage pane_usage{};
    if (!cache.GetPaneUsage(first.pane, pane_usage)
        || pane_usage.resident_bytes != 16U || pane_usage.entry_count != 1U) {
        return false;
    }
    cache.RemoveDocument(first.document, first.document_generation);
    if (cache.Peek(first, image) || cache.Usage().resident_bytes != 16U
        || cache.InvalidationGeneration(ThumbnailKind::Layer)
            == initial_layer_generation
        || cache.InvalidationGeneration(ThumbnailKind::Sequence)
            != evicted_sequence_generation) {
        return false;
    }
    cache.RemovePane(third.pane);
    if (cache.Usage().resident_bytes != 0U || cache.Usage().entry_count != 0U) {
        return false;
    }
    if (!cache.Put(
            second, 2U, 2U, 8U, ThumbnailPixelLayout::Rgba8, second_pixels)) {
        return false;
    }
    cache.RemoveDocument(second.document, second.document_generation,
        ThumbnailKind::Layer);
    if (!cache.Peek(second, image)
        || cache.InvalidationGeneration(ThumbnailKind::Sequence)
            != evicted_sequence_generation
        || !cache.Put(
            second, 2U, 2U, 8U, ThumbnailPixelLayout::Rgba8, second_pixels)
        || cache.InvalidationGeneration(ThumbnailKind::Sequence)
            != evicted_sequence_generation
        || !cache.Put(
            second, 2U, 2U, 8U, ThumbnailPixelLayout::Rgba8, third_pixels)
        || cache.InvalidationGeneration(ThumbnailKind::Sequence)
            == evicted_sequence_generation) {
        return false;
    }
    const auto replaced_generation =
        cache.InvalidationGeneration(ThumbnailKind::Sequence);
    cache.RemoveDocument(second.document, second.document_generation,
        ThumbnailKind::Sequence);
    if (cache.Peek(second, image)
        || cache.InvalidationGeneration(ThumbnailKind::Sequence)
            == replaced_generation
        || !cache.Put(
            second, 2U, 2U, 8U, ThumbnailPixelLayout::Rgba8, second_pixels)) {
        return false;
    }
    const auto before_clear = cache.InvalidationGeneration(ThumbnailKind::Sequence);
    cache.Clear();
    return cache.Usage().resident_bytes == 0U
        && cache.InvalidationGeneration(ThumbnailKind::Sequence) != before_clear;
}

bool TestDocumentIdentityAndIndex() {
    struct PathPair final {
        const wchar_t* ordinary;
        const wchar_t* extended;
        bool equivalent;
    };
    constexpr std::array path_pairs{
        PathPair{L"C:\\inkpod\\cell.inkpod", L"\\\\?\\C:\\Inkpod\\CELL.inkpod", true},
        PathPair{L"C:\\inkpod\\a\\..\\cell.png", L"\\\\?\\C:\\inkpod\\cell.png", true},
        PathPair{L"C:\\", L"\\\\?\\c:\\", true},
        PathPair{L"\\\\server\\share\\cell.tif", L"\\\\?\\UNC\\SERVER\\Share\\cell.tif", true},
        PathPair{L"\\\\server\\share\\", L"\\\\?\\unc\\server\\share\\", true},
        PathPair{L"C:\\inkpod\\cell.inkpod", L"\\\\?\\C:\\inkpod\\cell.inkpod.", false},
        PathPair{L"C:\\inkpod\\cell.inkpod", L"\\\\?\\C:\\inkpod\\cell.inkpod ", false},
        PathPair{L"C:\\inkpod\\folder\\cell.png", L"\\\\?\\C:\\inkpod\\folder.\\cell.png", false},
        PathPair{L"C:\\inkpod\\folder\\cell.png", L"\\\\?\\C:\\inkpod\\folder \\cell.png", false},
        PathPair{L"C:\\inkpod\\cell.png", L"\\\\?\\C:\\inkpod\\.\\cell.png", false},
        PathPair{L"C:\\inkpod\\cell.png", L"\\\\?\\C:\\inkpod\\a\\..\\cell.png", false},
        PathPair{L"C:\\inkpod\\cell.png", L"\\\\?\\C:\\inkpod/cell.png", false},
        PathPair{L"C:\\inkpod\\NUL.png", L"\\\\?\\C:\\inkpod\\NUL.png", false},
        PathPair{L"C:\\inkpod\\COM1.png", L"\\\\?\\C:\\inkpod\\COM1.png", false},
        PathPair{L"C:\\inkpod\\LPT\u00b2.png", L"\\\\?\\C:\\inkpod\\LPT\u00b2.png", false},
        PathPair{L"\\\\server\\share\\cell.png", L"\\\\?\\UNC\\server\\share\\cell.png.", false},
    };
    for (const auto& pair : path_pairs) {
        std::wstring ordinary;
        std::wstring extended;
        if (!inkpod::app::NormalizeDocumentFilePath(pair.ordinary, ordinary)
            || !inkpod::app::NormalizeDocumentFilePath(pair.extended, extended)
            || (ordinary == extended) != pair.equivalent) {
            return false;
        }
    }
    const std::wstring long_path = L"C:\\inkpod\\" + std::wstring(240U, L'a')
        + L"\\cell.inkpod";
    std::wstring ordinary_long;
    std::wstring extended_long;
    if (!inkpod::app::NormalizeDocumentFilePath(long_path, ordinary_long)
        || !inkpod::app::NormalizeDocumentFilePath(L"\\\\?\\" + long_path, extended_long)
        || ordinary_long != extended_long) {
        return false;
    }

    struct FixtureIo final {
        InkpodIoManager* manager{};
        ~FixtureIo() {
            static_cast<void>(inkpod_io_manager_release(&manager));
        }
    } io;
    if (inkpod_io_manager_create(nullptr, &io.manager) != INKPOD_STATUS_OK) {
        return false;
    }
    std::array<wchar_t, MAX_PATH> directory{};
    std::array<wchar_t, MAX_PATH> file{};
    if (GetTempPathW(
            static_cast<DWORD>(directory.size()), directory.data()) == 0U
        || GetTempFileNameW(directory.data(), L"ipd", 0U, file.data()) == 0U) {
        return false;
    }
    const std::wstring path(file.data());
    const std::wstring hard_link = path + L".link";
    DeleteFileW(hard_link.c_str());
    if (CreateHardLinkW(hard_link.c_str(), path.c_str(), nullptr) == FALSE) {
        DeleteFileW(path.c_str());
        return false;
    }
    DocumentIdentity direct{};
    DocumentIdentity alias{};
    if (!inkpod::app::ResolveDocumentFileIdentity(io.manager, path, direct)
        || !inkpod::app::ResolveDocumentFileIdentity(io.manager, hard_link, alias)
        || !(direct == alias)) {
        DeleteFileW(hard_link.c_str());
        DeleteFileW(path.c_str());
        return false;
    }

    std::array<wchar_t, MAX_PATH> original_directory{};
    const DWORD original_directory_length = GetCurrentDirectoryW(
        static_cast<DWORD>(original_directory.size()),
        original_directory.data());
    const wchar_t* relative_name = wcsrchr(file.data(), L'\\');
    DocumentIdentity relative{};
    bool relative_equal = false;
    if (original_directory_length > 0U
        && original_directory_length < original_directory.size()
        && relative_name != nullptr
        && SetCurrentDirectoryW(directory.data()) != FALSE) {
        relative_equal = inkpod::app::ResolveDocumentFileIdentity(
                             io.manager, relative_name + 1, relative)
            && relative == direct;
        SetCurrentDirectoryW(original_directory.data());
    }
    if (!relative_equal) {
        DeleteFileW(hard_link.c_str());
        DeleteFileW(path.c_str());
        return false;
    }

    const std::wstring missing = path + L".missing";
    std::wstring case_variant = missing;
    for (wchar_t& character : case_variant) {
        character = static_cast<wchar_t>(towupper(character));
    }
    DocumentIdentity normalized{};
    DocumentIdentity normalized_case{};
    const bool normalized_equal =
        inkpod::app::ResolveDocumentFileIdentity(io.manager, missing, normalized)
        && inkpod::app::ResolveDocumentFileIdentity(
            io.manager, case_variant, normalized_case)
        && normalized == normalized_case;

    DocumentRegistry registry;
    auto* core = reinterpret_cast<CoreHost*>(static_cast<std::uintptr_t>(1U));
    const bool indexed = registry.InitializePlaceholder(Generation{1U})
        && registry.Replace(
            DocumentSessionId{11U},
            Generation{3U},
            DocumentViewId{17U},
            core)
        && registry.AssignIdentity(DocumentSessionId{11U}, direct)
        && registry.Add(
            DocumentSessionId{13U},
            Generation{3U},
            DocumentViewId{19U},
            core)
        && !registry.AssignIdentity(DocumentSessionId{13U}, alias)
        && registry.AssignIdentity(
            DocumentSessionId{13U},
            inkpod::app::UntitledDocumentIdentity(7U, 9U))
        && registry.FindByIdentity(direct) != nullptr
        && registry.FindByIdentity(direct)->id == DocumentSessionId{11U}
        && registry.FindByView(DocumentViewId{19U}) != nullptr
        && registry.FindByView(DocumentViewId{19U})->id
            == DocumentSessionId{13U};
    bool reservations = indexed;
    if (reservations) {
        const auto owner = DocumentSessionId{13U};
        const DocumentIdentity prior = registry.Find(owner)->identity;
        g_allocations_before_failure = 0;
        const bool failed_prepare = !registry.ReserveIdentity(owner, normalized, missing);
        g_allocations_before_failure = -1;
        reservations = failed_prepare && registry.Find(owner)->identity == prior
            && !registry.HasIdentityReservation(normalized)
            && registry.ReserveIdentity(owner, normalized, missing)
            && registry.Find(owner)->identity == prior
            && registry.FindByIdentity(normalized) == nullptr
            && registry.HasIdentityReservation(normalized)
            && registry.HasIdentityReservation(direct, normalized_case.normalized_path)
            && !registry.AssignIdentity(DocumentSessionId{11U}, normalized_case)
            && !registry.ReserveIdentity(DocumentSessionId{11U}, normalized_case)
            && !registry.ReserveIdentity(owner, prior);
        if (reservations) {
            g_allocations_before_failure = 0;
            const bool published = registry.PublishReservedIdentity(owner);
            const bool no_allocation = g_allocations_before_failure == 0;
            g_allocations_before_failure = -1;
            reservations = published && no_allocation
                && registry.Find(owner)->identity == normalized
                && !registry.HasIdentityReservation(normalized);
        }
        const DocumentIdentity next = inkpod::app::UntitledDocumentIdentity(71U, 91U);
        if (reservations) {
            reservations = registry.ReserveIdentity(owner, next, case_variant);
            registry.CancelIdentityReservation(owner);
            reservations = reservations && registry.Find(owner)->identity == normalized
                && !registry.HasIdentityReservation(next, normalized.normalized_path)
                && registry.ReserveIdentity(owner, next, missing)
                && registry.Remove(owner)
                && !registry.HasIdentityReservation(next, normalized.normalized_path)
                && registry.ReserveIdentity(DocumentSessionId{11U}, next)
                && registry.Replace(DocumentSessionId{17U}, Generation{5U},
                    DocumentViewId{23U}, core)
                && !registry.HasIdentityReservation(next);
        }
    }
    registry.Clear();
    DeleteFileW(hard_link.c_str());
    DeleteFileW(path.c_str());
    return normalized_equal && indexed && reservations;
}

bool TestRecentDocumentList() {
    RecentDocumentList recent;
    for (std::uint64_t index = 1U; index <= 10U; ++index) {
        if (!recent.Record(
                L"C:\\cells\\cell-" + std::to_wstring(index) + L".inkpod",
                inkpod::app::UntitledDocumentIdentity(index, index + 100U))) {
            return false;
        }
    }
    if (recent.Count() != RecentDocumentList::kCapacity
        || recent.At(0U) == nullptr
        || recent.At(0U)->path != L"C:\\cells\\cell-10.inkpod"
        || recent.At(RecentDocumentList::kCapacity) != nullptr) {
        return false;
    }
    const auto repeated = inkpod::app::UntitledDocumentIdentity(7U, 107U);
    if (!recent.Record(L"C:\\renamed\\cell-7.inkpod", repeated)
        || recent.Count() != RecentDocumentList::kCapacity
        || recent.At(0U) == nullptr
        || recent.At(0U)->identity != repeated
        || recent.At(0U)->path != L"C:\\renamed\\cell-7.inkpod"
        || !recent.Remove(0U)
        || recent.Count() != RecentDocumentList::kCapacity - 1U
        || recent.Remove(RecentDocumentList::kCapacity)
        || recent.Record(L"", repeated)) {
        return false;
    }
    recent.Clear();
    return recent.Count() == 0U && recent.At(0U) == nullptr;
}

bool TestOwnerGraphFailureUnwind() {
    auto* owner = reinterpret_cast<ApplicationHost*>(
        static_cast<std::uintptr_t>(1U));
    const std::size_t baseline = g_live_allocations;

    {
        WorkspaceWindowRegistry workspaces;
        DocumentRegistry documents;
        g_allocations_before_failure = 0;
        if (inkpod::app::InitializeOwnerGraph(
                workspaces,
                documents,
                owner,
                WorkspaceWindowId{3U},
                EditorGroupId{4U},
                inkpod::app::CanvasId{5U},
                Generation{5U})
            || workspaces.Current() != nullptr
            || documents.Current() != nullptr
            || g_live_allocations != baseline) {
            return false;
        }
    }

    {
        WorkspaceWindowRegistry workspaces;
        DocumentRegistry documents;
        g_allocations_before_failure = 1;
        if (inkpod::app::InitializeOwnerGraph(
                workspaces,
                documents,
                owner,
                WorkspaceWindowId{7U},
                EditorGroupId{8U},
                inkpod::app::CanvasId{9U},
                Generation{11U})
            || workspaces.Current() != nullptr
            || documents.Current() != nullptr
            || g_live_allocations != baseline) {
            return false;
        }
    }

    {
        WorkspaceWindowRegistry workspaces;
        DocumentRegistry documents;
        if (!inkpod::app::InitializeOwnerGraph(
                workspaces,
                documents,
                owner,
                WorkspaceWindowId{13U},
                EditorGroupId{14U},
                inkpod::app::CanvasId{15U},
                Generation{17U})
            || workspaces.Current() == nullptr
            || documents.Current() == nullptr
            || documents.Current()->ActiveView() == nullptr) {
            return false;
        }
        inkpod::app::ClearOwnerGraph(documents, workspaces);
        if (workspaces.Current() != nullptr
            || documents.Current() != nullptr
            || g_live_allocations != baseline) {
            return false;
        }
    }
    return true;
}

bool TestSequencePresentationAcknowledgement() {
    DocumentRegistry registry;
    auto* core = reinterpret_cast<CoreHost*>(static_cast<std::uintptr_t>(1U));
    const DocumentSessionId first_id{11U};
    const DocumentSessionId second_id{13U};
    const Generation generation{7U};
    const DocumentViewId first_view{17U};
    const DocumentViewId second_view{19U};
    if (!registry.InitializePlaceholder(generation)
        || !registry.Replace(first_id, generation, first_view, core)) {
        return false;
    }
    auto* first = registry.Current();
    first->sequence_required_present_revision = 41U;
    first->sequence_required_present_epoch = 101U;
    if (first->HasSequencePresentationAcknowledgement()
        || first->AcknowledgeSequencePresentation(second_id, generation, first_view, 41U, 101U)
        || first->AcknowledgeSequencePresentation(first_id, Generation{8U}, first_view, 41U, 101U)
        || first->AcknowledgeSequencePresentation(first_id, generation, second_view, 41U, 101U)
        || first->AcknowledgeSequencePresentation(first_id, generation, first_view, 40U, 101U)
        || first->AcknowledgeSequencePresentation(first_id, generation, first_view, 41U, 100U)
        || first->HasSequencePresentationAcknowledgement()) {
        return false;
    }
    first->sequence_activation_pending = true;
    if (first->AcknowledgeSequencePresentation(first_id, generation, first_view, 41U, 101U)) {
        return false;
    }
    first->sequence_activation_pending = false;
    if (!first->AcknowledgeSequencePresentation(first_id, generation, first_view, 41U, 101U)
        || !first->HasSequencePresentationAcknowledgement()) {
        return false;
    }

    // A pane retains A's target while B becomes the active tab. A's verified
    // one-shot acknowledgement belongs to its session, not to the reused Canvas.
    inkpod::app::CommandContext pinned{};
    pinned.document_session = first_id;
    pinned.document_view = first_view;
    pinned.generation = generation;
    if (!registry.Add(second_id, generation, second_view, core)
        || registry.Current()->id != second_id
        || !registry.Find(pinned.document_session.value())->HasSequencePresentationAcknowledgement()
        || registry.Current()->HasSequencePresentationAcknowledgement()) {
        return false;
    }
    first->sequence_activation_pending = true;
    if (first->HasSequencePresentationAcknowledgement()
        || first->AcknowledgeSequencePresentation(first_id, generation, first_view, 41U, 101U)) {
        return false;
    }
    // A failed switch restores the previous fence; a new committed epoch does
    // not inherit that acknowledgement, even if recovery lowers the revision.
    first->sequence_activation_pending = false;
    if (!first->HasSequencePresentationAcknowledgement()) {
        return false;
    }
    first->sequence_required_present_revision = 2U;
    first->sequence_required_present_epoch = 102U;
    if (first->HasSequencePresentationAcknowledgement()
        || first->AcknowledgeSequencePresentation(first_id, generation, first_view, 41U, 101U)
        || !first->AcknowledgeSequencePresentation(first_id, generation, first_view, 2U, 102U)
        || !first->HasSequencePresentationAcknowledgement()) {
        return false;
    }

    // DocumentRegistry can replace the current owner in place. Neither a new
    // generation nor another session may inherit its old successful Present.
    if (!registry.Activate(first_id)
        || !registry.Replace(first_id, Generation{8U}, first_view, core)
        || first->HasSequencePresentationAcknowledgement()
        || first->AcknowledgeSequencePresentation(first_id, generation, first_view, 2U, 102U)
        || !first->AcknowledgeSequencePresentation(first_id, Generation{8U}, first_view, 2U, 102U)
        || !registry.Replace(DocumentSessionId{23U}, Generation{8U}, first_view, core)
        || first->HasSequencePresentationAcknowledgement()) {
        return false;
    }
    first->sequence_required_present_revision = 0U;
    first->sequence_required_present_epoch = 0U;
    return !first->HasSequencePresentationAcknowledgement()
        && !first->AcknowledgeSequencePresentation(
            first->id, first->generation, first_view, 2U, 0U);
}

bool TestDocumentAndViewLifetime() {
    DocumentRegistry registry;
    auto* core = reinterpret_cast<CoreHost*>(static_cast<std::uintptr_t>(1U));
    if (!registry.InitializePlaceholder(Generation{1U})
        || registry.Current() == nullptr
        || registry.Current()->ViewCount() != 1U) {
        return false;
    }

    if (registry.Replace({}, Generation{3U}, DocumentViewId{17U}, nullptr)
        || registry.Replace(
            DocumentSessionId{11U}, {}, DocumentViewId{17U}, nullptr)
        || registry.Replace(
            DocumentSessionId{11U}, Generation{3U}, {}, nullptr)
        || registry.Current()->ViewCount() != 1U) {
        return false;
    }

    registry.Current()->shell.current_path = L"C:\\work\\cell.inkpod";
    if (!registry.Replace(
            DocumentSessionId{11U},
            Generation{3U},
            DocumentViewId{17U},
            core)) {
        return false;
    }
    auto* document = registry.Current();
    if (document == nullptr || document->id != DocumentSessionId{11U}
        || document->generation != Generation{3U}
        || document->ViewCount() != 1U
        || document->ActiveView() == nullptr
        || document->ActiveView()->id != DocumentViewId{17U}
        || document->ActiveView()->core_view_id != 0U
        || document->Core() != core
        || document->shell.current_path != L"C:\\work\\cell.inkpod") {
        return false;
    }
    inkpod::app::RecoveryMetadata sequence_metadata{};
    sequence_metadata.session = document->id;
    sequence_metadata.generation = document->generation;
    sequence_metadata.document_uuid_high = 71U;
    sequence_metadata.document_uuid_low = 73U;
    if (!document->PublishSequenceAutosave(
            71U, 73U, 5U, L"C:\\recovery\\cell.inkpod", sequence_metadata)
        || document->FindSequenceAutosave(71U, 73U, 4U) != nullptr) {
        return false;
    }
    const auto* sequence_autosave = document->FindSequenceAutosave(71U, 73U, 5U);
    if (sequence_autosave == nullptr
        || sequence_autosave->artifact_generation != 1U
        || sequence_autosave->recovery_path != L"C:\\recovery\\cell.inkpod"
        || !document->PublishSequenceAutosave(
            71U, 73U, 5U, L"C:\\recovery\\cell-2.inkpod", sequence_metadata)) {
        return false;
    }
    sequence_autosave = document->FindSequenceAutosave(71U, 73U, 5U);
    if (sequence_autosave == nullptr
        || sequence_autosave->artifact_generation != 2U
        || sequence_autosave->recovery_path != L"C:\\recovery\\cell-2.inkpod") {
        return false;
    }
    document->ClearSequenceAutosaves();
    if (document->FindSequenceAutosave(71U, 73U, 5U) != nullptr) {
        return false;
    }
    if (registry.Replace({}, Generation{4U}, DocumentViewId{21U}, nullptr)
        || registry.Current() != document
        || registry.Current()->id != DocumentSessionId{11U}
        || registry.Current()->shell.current_path != L"C:\\work\\cell.inkpod") {
        return false;
    }

    document->ActiveView()->presentation.flip_horizontal = true;
    if (!document->AddView(DocumentViewId{19U}, Generation{3U}, 41U)
        || document->ViewCount() != 2U
        || document->ActiveView() == nullptr
        || document->ActiveView()->id != DocumentViewId{19U}
        || document->ActiveView()->presentation.flip_horizontal
        || document->AddView(DocumentViewId{19U}, Generation{3U}, 42U)
        || document->AddView(DocumentViewId{23U}, Generation{3U}, 41U)) {
        return false;
    }
    if (!document->ActivateCoreView(0U)
        || document->ActiveView() == nullptr
        || document->ActiveView()->id != DocumentViewId{17U}
        || !document->ActiveView()->presentation.flip_horizontal
        || document->ActivateView(DocumentViewId{999U})) {
        return false;
    }
    if (!document->RemoveView(DocumentViewId{17U})
        || document->ViewCount() != 1U
        || document->ActiveView() == nullptr
        || document->ActiveView()->id != DocumentViewId{19U}
        || document->RemoveView(DocumentViewId{17U})) {
        return false;
    }
    if (!registry.Add(
            DocumentSessionId{13U},
            Generation{4U},
            DocumentViewId{23U},
            core)
        || registry.Count() != 2U
        || registry.Current() == nullptr
        || registry.Current()->id != DocumentSessionId{13U}
        || registry.Add(
            DocumentSessionId{13U},
            Generation{5U},
            DocumentViewId{29U},
            core)
        || !registry.Activate(DocumentSessionId{11U})
        || registry.Current() != document
        || registry.Find(DocumentSessionId{13U}) == nullptr) {
        return false;
    }
    registry.ClearCoreBindings();
    if (registry.Current()->Core() != nullptr
        || registry.Find(DocumentSessionId{13U})->Core() != nullptr
        || !registry.Remove(DocumentSessionId{13U})
        || registry.Count() != 1U
        || registry.Remove(DocumentSessionId{13U})) {
        return false;
    }
    registry.Clear();
    return registry.Current() == nullptr && registry.Count() == 0U;
}

bool TestWorkspaceLifetime() {
    WorkspaceWindowRegistry registry;
    auto* owner = reinterpret_cast<ApplicationHost*>(&registry);
    if (registry.Initialize(
            nullptr,
            WorkspaceWindowId{5U},
            EditorGroupId{6U},
            inkpod::app::CanvasId{7U},
            Generation{7U})
        || registry.Initialize(
            owner, {}, EditorGroupId{6U}, inkpod::app::CanvasId{7U}, Generation{7U})
        || registry.Initialize(
            owner, WorkspaceWindowId{5U}, {}, inkpod::app::CanvasId{7U}, Generation{7U})
        || registry.Initialize(
            owner, WorkspaceWindowId{5U}, EditorGroupId{6U}, {}, Generation{7U})
        || registry.Initialize(
            owner,
            WorkspaceWindowId{5U},
            EditorGroupId{6U},
            inkpod::app::CanvasId{7U},
            {})
        || registry.Current() != nullptr
        || !registry.Initialize(
            owner,
            WorkspaceWindowId{5U},
            EditorGroupId{6U},
            inkpod::app::CanvasId{7U},
            Generation{7U})
        || registry.Current() == nullptr
        || registry.Current()->application != owner
        || registry.Current()->id != WorkspaceWindowId{5U}
        || registry.Current()->generation != Generation{7U}) {
        return false;
    }
    registry.Current()->windows.window = reinterpret_cast<HWND>(
        static_cast<std::uintptr_t>(31U));
    if (registry.Initialize(
            owner, {}, EditorGroupId{8U}, inkpod::app::CanvasId{9U}, Generation{10U})
        || registry.Current()->id != WorkspaceWindowId{5U}
        || registry.Current()->windows.window == nullptr) {
        return false;
    }
    if (!registry.Initialize(
            owner,
            WorkspaceWindowId{9U},
            EditorGroupId{10U},
            inkpod::app::CanvasId{11U},
            Generation{10U})
        || registry.Current() == nullptr
        || registry.Current()->id != WorkspaceWindowId{9U}
        || registry.Current()->windows.window != nullptr) {
        return false;
    }
    registry.Clear();
    return registry.Current() == nullptr;
}

bool TestMultipleWorkspaceLifetimeAndFocus() {
    WorkspaceWindowRegistry registry;
    auto* owner = reinterpret_cast<ApplicationHost*>(&registry);
    if (!registry.Initialize(
            owner,
            WorkspaceWindowId{1U},
            EditorGroupId{2U},
            inkpod::app::CanvasId{3U},
            Generation{4U})
        || !registry.Add(
            owner,
            WorkspaceWindowId{5U},
            EditorGroupId{6U},
            inkpod::app::CanvasId{7U},
            Generation{4U},
            1U)
        || registry.Count() != 2U
        || registry.Current() == nullptr
        || registry.Current()->id != WorkspaceWindowId{5U}
        || registry.LastFocused() == nullptr
        || registry.LastFocused()->id != WorkspaceWindowId{1U}
        || registry.Find(WorkspaceWindowId{1U}) == nullptr
        || registry.Find(WorkspaceWindowId{5U}) == nullptr
        || registry.At(0U)->persistence_slot != 0U
        || registry.At(1U)->persistence_slot != 1U) {
        return false;
    }
    if (!registry.Activate(WorkspaceWindowId{5U}, true)
        || registry.LastFocused() == nullptr
        || registry.LastFocused()->id != WorkspaceWindowId{5U}
        || !registry.Activate(WorkspaceWindowId{1U}, false)
        || registry.Current()->id != WorkspaceWindowId{1U}
        || registry.LastFocused()->id != WorkspaceWindowId{5U}
        || registry.Activate(WorkspaceWindowId{99U}, true)
        || registry.Add(
            owner,
            WorkspaceWindowId{5U},
            EditorGroupId{8U},
            inkpod::app::CanvasId{9U},
            Generation{4U},
            2U)) {
        return false;
    }
    if (!registry.Remove(WorkspaceWindowId{5U})
        || registry.Count() != 1U
        || registry.Current() == nullptr
        || registry.Current()->id != WorkspaceWindowId{1U}
        || registry.LastFocused() == nullptr
        || registry.LastFocused()->id != WorkspaceWindowId{1U}
        || registry.Remove(WorkspaceWindowId{5U})) {
        return false;
    }
    registry.Clear();
    return registry.Count() == 0U && registry.Current() == nullptr
        && registry.LastFocused() == nullptr;
}

bool TestEditorAreaLifetimeAndSplit() {
    EditorArea editors;
    const EditorGroupId first_group{1U};
    const EditorGroupId second_group{2U};
    const DocumentViewId first_view{11U};
    const DocumentViewId second_view{12U};
    const DocumentViewId third_view{13U};
    if (editors.Initialize({}, inkpod::app::CanvasId{1U}, Generation{1U})
        || !editors.Initialize(
            first_group, inkpod::app::CanvasId{21U}, Generation{31U})
        || !editors.AddView(first_group, first_view)
        || !editors.AddView(first_group, second_view)
        || editors.AddView(first_group, first_view)
        || editors.Split(
            second_group,
            inkpod::app::CanvasId{22U},
            Generation{32U},
            EditorSplitOrientation::None)
        || !editors.Split(
            second_group,
            inkpod::app::CanvasId{22U},
            Generation{32U},
            EditorSplitOrientation::Vertical)
        || editors.GroupCount() != 2U
        || editors.Orientation() != EditorSplitOrientation::Vertical
        || !editors.AddView(second_group, third_view)
        || !editors.Activate(first_group)
        || editors.Active() == nullptr
        || editors.Active()->ActiveView() != second_view) {
        return false;
    }

    editors.SetSplitRatioMilli(1U);
    if (editors.SplitRatioMilli() != 200U
        || !editors.SetOrientation(EditorSplitOrientation::Horizontal)
        || editors.Orientation() != EditorSplitOrientation::Horizontal
        || !editors.ReorderView(second_view, 0U)
        || editors.Find(first_group)->ViewAt(0U) != second_view
        || editors.Find(first_group)->ViewAt(1U) != first_view
        || !editors.ReorderView(second_view, 1U)
        || editors.ReorderView(second_view, 3U)
        || !editors.MoveView(second_view, second_group, 0U)
        || editors.FindByView(second_view) == nullptr
        || editors.FindByView(second_view)->id != second_group
        || editors.Find(second_group)->ViewAt(0U) != second_view
        || editors.Find(second_group)->ViewAt(1U) != third_view
        || editors.MoveView(second_view, second_group)) {
        return false;
    }
    editors.SetSplitRatioMilli(999U);
    if (editors.SplitRatioMilli() != 800U) {
        return false;
    }

    EditorGroupId survivor{};
    if (!editors.MergeAndRemove(second_group, survivor)
        || survivor != first_group
        || editors.GroupCount() != 1U
        || editors.Orientation() != EditorSplitOrientation::None
        || editors.FindByView(first_view) == nullptr
        || editors.FindByView(second_view) == nullptr
        || editors.FindByView(third_view) == nullptr
        || editors.MergeAndRemove(first_group, survivor)) {
        return false;
    }
    editors.Clear();
    return editors.GroupCount() == 0U && editors.Active() == nullptr;
}

bool SameDockPanePlacement(
    const DockPanePlacement& left, const DockPanePlacement& right) noexcept {
    return left.type == right.type && left.zone == right.zone
        && left.restore_zone == right.restore_zone && left.order == right.order
        && left.stack == right.stack && left.tab_order == right.tab_order
        && left.split_weight == right.split_weight
        && left.floating.x_dip == right.floating.x_dip
        && left.floating.y_dip == right.floating.y_dip
        && left.floating.width_dip == right.floating.width_dip
        && left.floating.height_dip == right.floating.height_dip
        && left.present == right.present && left.active_tab == right.active_tab;
}

bool SameDockZoneState(
    const DockZoneState& left, const DockZoneState& right) noexcept {
    return left.mode == right.mode && left.active_tab == right.active_tab
        && left.extent_dip == right.extent_dip;
}

bool SameDockLayoutRecord(
    const DockLayoutRecord& left, const DockLayoutRecord& right) noexcept {
    if (left.version != right.version || left.pane_count != right.pane_count
        || left.mirrored != right.mirrored) {
        return false;
    }
    for (std::size_t index = 0U; index < left.panes.size(); ++index) {
        if (!SameDockPanePlacement(left.panes[index], right.panes[index])) {
            return false;
        }
    }
    for (std::size_t index = 0U; index < left.zones.size(); ++index) {
        if (!SameDockZoneState(left.zones[index], right.zones[index])) {
            return false;
        }
    }
    return true;
}

bool SameRightToolTabs(
    const RightToolTabsModel& left, const RightToolTabsModel& right) noexcept {
    const auto left_tabs = left.Tabs();
    const auto right_tabs = right.Tabs();
    if (left_tabs.size() != right_tabs.size()
        || left.Selected() != right.Selected()
        || left.NextStableId() != right.NextStableId()) {
        return false;
    }
    for (std::size_t tab_index = 0U; tab_index < left_tabs.size(); ++tab_index) {
        const auto& left_tab = left_tabs[tab_index];
        const auto& right_tab = right_tabs[tab_index];
        if (left_tab.id != right_tab.id
            || left_tab.pane_count != right_tab.pane_count) {
            return false;
        }
        for (std::size_t pane_index = 0U;
             pane_index < left_tab.pane_count;
             ++pane_index) {
            if (left_tab.panes[pane_index] != right_tab.panes[pane_index]) {
                return false;
            }
        }
    }
    return true;
}

bool TestDockHostCompoundMutationRollback() {
    DockLayoutModel model;
    RightToolTabsModel right_tool_tabs;
    constexpr DockPaneType pane = DockPaneType::Reference;
    constexpr ToolTabId destination{1U};
    if (model.RemovePane(pane) != DockResult::Ok
        || right_tool_tabs.Find(destination) == nullptr
        || right_tool_tabs.TabForPane(pane)) {
        return false;
    }
    const DockLayoutRecord before_model = model.ToRecord();
    const RightToolTabsModel before_tabs = right_tool_tabs;

    DockHost host;
    host.*AccessDockHostMember(DockHostModelTag{}) = &model;
    host.*AccessDockHostMember(DockHostRightToolTabsTag{}) = &right_tool_tabs;
    const ToolTabResult result =
        (host.*AccessDockHostMember(DockHostMovePaneToToolTabTag{}))(
            pane, destination);

    return result == ToolTabResult::InvalidPane
        && SameDockLayoutRecord(model.ToRecord(), before_model)
        && SameRightToolTabs(right_tool_tabs, before_tabs);
}

}  // namespace

int main() {
    if (!TestSequencePresentationAcknowledgement()) {
        std::cerr << "sequence presentation acknowledgement test failed\n";
        return 9;
    }
    if (!TestApplicationWideThumbnailLru()) {
        std::cerr << "application-wide thumbnail LRU test failed\n";
        return 8;
    }
    if (!TestOwnerGraphFailureUnwind()) {
        std::cerr << "owner graph failure unwind test failed\n";
        return 1;
    }
    if (!TestDocumentAndViewLifetime()) {
        std::cerr << "document/session/view ownership test failed\n";
        return 2;
    }
    if (!TestDocumentIdentityAndIndex()) {
        std::cerr << "document identity/index test failed\n";
        return 3;
    }
    if (!TestRecentDocumentList()) {
        std::cerr << "recent document list test failed\n";
        return 4;
    }
    if (!TestWorkspaceLifetime()) {
        std::cerr << "workspace ownership test failed\n";
        return 5;
    }
    if (!TestMultipleWorkspaceLifetimeAndFocus()) {
        std::cerr << "multiple workspace ownership/focus test failed\n";
        return 7;
    }
    if (!TestEditorAreaLifetimeAndSplit()) {
        std::cerr << "editor area split ownership test failed\n";
        return 6;
    }
    if (!TestDockHostCompoundMutationRollback()) {
        std::cerr << "DockHost compound mutation rollback test failed\n";
        return 10;
    }
    return 0;
}
