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
using inkpod::app::IdentityReservationToken;
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

InkpodIoRecoveryArtifactProof RecoveryProof(std::uint64_t seed) noexcept {
    InkpodIoRecoveryArtifactProof proof{};
    proof.struct_size = sizeof(proof);
    proof.native.struct_size = sizeof(proof.native);
    proof.native.identity.struct_size = sizeof(proof.native.identity);
    proof.native.identity.kind = 1U;
    proof.native.identity.volume = seed;
    proof.native.identity.object_low = seed + 1U;
    proof.native.length = seed + 2U;
    proof.metadata.struct_size = sizeof(proof.metadata);
    proof.metadata.identity.struct_size = sizeof(proof.metadata.identity);
    proof.metadata.identity.kind = 1U;
    proof.metadata.identity.volume = seed;
    proof.metadata.identity.object_low = seed + 3U;
    proof.metadata.length = seed + 4U;
    return proof;
}

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
    DocumentIdentity pair_member{};
    DocumentIdentity replacement_pair_member{};
    const std::wstring pair_member_path = missing + L".pair-raster";
    const std::wstring replacement_pair_member_path = missing + L".replacement-raster";
    const bool normalized_equal =
        inkpod::app::ResolveDocumentFileIdentity(io.manager, missing, normalized)
        && inkpod::app::ResolveDocumentFileIdentity(
            io.manager, case_variant, normalized_case)
        && normalized == normalized_case
        && inkpod::app::ResolveDocumentFileIdentity(
            io.manager, pair_member_path, pair_member)
        && inkpod::app::ResolveDocumentFileIdentity(
            io.manager, replacement_pair_member_path, replacement_pair_member);

    DocumentRegistry registry;
    auto* core = reinterpret_cast<CoreHost*>(static_cast<std::uintptr_t>(1U));
    const bool indexed = registry.InitializePlaceholder(Generation{1U})
        && registry.Replace(
            DocumentSessionId{11U},
            Generation{3U},
            DocumentViewId{17U},
            core)
        && registry.AssignIdentity(DocumentSessionId{11U}, direct)
        && !registry.Find(DocumentSessionId{11U})->HasExactPairIdentities(
            direct, pair_member)
        && registry.Add(
            DocumentSessionId{13U},
            Generation{3U},
            DocumentViewId{19U},
            core)
        && !registry.AssignIdentity(DocumentSessionId{13U}, alias)
        && registry.AssignIdentity(
            DocumentSessionId{13U},
            inkpod::app::UntitledDocumentIdentity(7U, 9U))
        && registry.AssignPairIdentities(
            DocumentSessionId{11U}, direct, pair_member)
        && registry.Find(DocumentSessionId{11U})->HasExactPairIdentities(
            direct, pair_member)
        && !registry.Find(DocumentSessionId{11U})->HasExactPairIdentities(
            direct, replacement_pair_member)
        && !registry.Find(DocumentSessionId{11U})->HasExactPairIdentities(
            pair_member, direct)
        && !registry.AssignIdentity(DocumentSessionId{13U}, pair_member)
        && registry.FindByIdentity(direct) != nullptr
        && registry.FindByIdentity(direct)->id == DocumentSessionId{11U}
        && registry.FindByIdentity(pair_member) != nullptr
        && registry.FindByIdentity(pair_member)->id == DocumentSessionId{11U}
        && registry.AssignPairIdentities(
            DocumentSessionId{11U}, direct, replacement_pair_member)
        && !registry.Find(DocumentSessionId{11U})->HasExactPairIdentities(
            direct, pair_member)
        && registry.Find(DocumentSessionId{11U})->HasExactPairIdentities(
            direct, replacement_pair_member)
        && registry.FindByIdentity(pair_member) == nullptr
        && registry.FindByIdentity(replacement_pair_member) != nullptr
        && registry.FindByIdentity(replacement_pair_member)->id == DocumentSessionId{11U}
        && registry.FindByView(DocumentViewId{19U}) != nullptr
        && registry.FindByView(DocumentViewId{19U})->id
            == DocumentSessionId{13U};
    bool reservations = indexed;
    IdentityReservationToken last_issued_token{};
    if (reservations) {
        const auto owner = DocumentSessionId{13U};
        const auto other_owner = DocumentSessionId{11U};
        const DocumentIdentity prior = registry.Find(owner)->identity;
        g_allocations_before_failure = 0;
        const auto failed_prepare =
            registry.ReserveIdentity(owner, normalized, missing);
        g_allocations_before_failure = -1;
        const auto first_token =
            registry.ReserveIdentity(owner, normalized, missing);
        reservations = !failed_prepare && first_token
            && registry.Find(owner)->identity == prior
            && registry.FindByIdentity(normalized) == nullptr
            && registry.HasIdentityReservation(normalized)
            && registry.HasIdentityReservation(direct, normalized_case.normalized_path)
            && !registry.AssignIdentity(other_owner, normalized_case)
            && !registry.ReserveIdentity(other_owner, normalized_case)
            && !registry.ReserveIdentity(owner, prior);
        if (reservations) {
            const IdentityReservationToken wrong_token{UINT64_MAX};
            reservations = !registry.PublishReservedIdentity(
                               other_owner, first_token)
                && !registry.CancelIdentityReservation(other_owner, first_token)
                && !registry.PublishReservedIdentity(owner, wrong_token)
                && !registry.CancelIdentityReservation(owner, wrong_token)
                && registry.HasIdentityReservation(normalized)
                && registry.Find(owner)->identity == prior;
        }
        if (reservations) {
            g_allocations_before_failure = 0;
            const bool published =
                registry.PublishReservedIdentity(owner, first_token);
            const bool no_allocation = g_allocations_before_failure == 0;
            g_allocations_before_failure = -1;
            reservations = published && no_allocation
                && registry.Find(owner)->identity == normalized
                && !registry.HasIdentityReservation(normalized)
                && !registry.PublishReservedIdentity(owner, first_token)
                && !registry.CancelIdentityReservation(owner, first_token);
        }
        const DocumentIdentity next = inkpod::app::UntitledDocumentIdentity(71U, 91U);
        const DocumentIdentity next_pair = inkpod::app::UntitledDocumentIdentity(72U, 92U);
        IdentityReservationToken cancelled_pair_token{};
        if (reservations) {
            cancelled_pair_token = registry.ReserveIdentityPair(
                owner, next, next_pair, case_variant, pair_member_path);
            reservations = cancelled_pair_token
                && cancelled_pair_token.value > first_token.value
                && registry.HasIdentityReservation(next)
                && registry.HasIdentityReservation(next_pair);
        }
        if (reservations) {
            g_allocations_before_failure = 0;
            const bool cancelled = registry.CancelIdentityReservation(
                owner, cancelled_pair_token);
            const bool no_allocation = g_allocations_before_failure == 0;
            g_allocations_before_failure = -1;
            reservations = cancelled && no_allocation
                && registry.Find(owner)->identity == normalized
                && !registry.Find(owner)->pair_raster_identity
                && !registry.HasIdentityReservation(
                    next, normalized.normalized_path);
        }
        const auto pair_publish_token = reservations
            ? registry.ReserveIdentityPair(owner, next, next_pair,
                  case_variant, pair_member_path)
            : IdentityReservationToken{};
        if (reservations) {
            reservations = pair_publish_token
                && pair_publish_token.value > cancelled_pair_token.value
                && !registry.PublishReservedIdentity(
                    owner, cancelled_pair_token)
                && !registry.CancelIdentityReservation(
                    owner, cancelled_pair_token)
                && registry.HasIdentityReservation(next)
                && registry.HasIdentityReservation(next_pair);
        }
        if (reservations) {
            g_allocations_before_failure = 0;
            const bool pair_published = registry.PublishReservedIdentity(
                owner, pair_publish_token);
            const bool no_allocation = g_allocations_before_failure == 0;
            g_allocations_before_failure = -1;
            reservations = pair_published && no_allocation
                && registry.Find(owner)->identity == next
                && registry.Find(owner)->pair_raster_identity == next_pair
                && !registry.HasIdentityReservation(next)
                && !registry.PublishReservedIdentity(owner, pair_publish_token);
        }
        const auto repair_token = reservations
            ? registry.ReserveIdentityPair(owner, next, next_pair,
                  case_variant, pair_member_path)
            : IdentityReservationToken{};
        if (reservations) {
            reservations = repair_token
                && repair_token.value > pair_publish_token.value;
        }
        {
            DocumentIdentity repaired_native{};
            repaired_native.kind =
                inkpod::app::DocumentIdentityKind::NormalizedPath;
            repaired_native.normalized_path = normalized.normalized_path;
            DocumentIdentity repaired_raster{};
            repaired_raster.kind = inkpod::app::DocumentIdentityKind::WindowsFile;
            repaired_raster.volume_serial = 0x9191U;
            repaired_raster.file_id[0] = 0x42U;
            if (reservations) {
                g_allocations_before_failure = 0;
                const bool repaired = registry.PublishRepairedReservedIdentityPair(
                    owner, repair_token, std::move(repaired_native),
                    std::move(repaired_raster));
                const bool no_allocation = g_allocations_before_failure == 0;
                g_allocations_before_failure = -1;
                reservations = repaired && no_allocation
                    && registry.Find(owner)->identity.kind
                        == inkpod::app::DocumentIdentityKind::NormalizedPath
                    && registry.Find(owner)->pair_raster_identity.kind
                        == inkpod::app::DocumentIdentityKind::WindowsFile
                    && !registry.HasIdentityReservation(next);
            }
            const DocumentIdentity revoked =
                inkpod::app::UntitledDocumentIdentity(501U, 503U);
            IdentityReservationToken revoke_token{};
            if (reservations) {
                revoke_token = registry.ReserveIdentityPair(
                    owner, next, next_pair, case_variant, pair_member_path);
                reservations = static_cast<bool>(revoke_token);
            }
            if (reservations) {
                g_allocations_before_failure = 0;
                const bool published =
                    registry.ForceRevokeIdentity(owner, revoked);
                const bool no_allocation = g_allocations_before_failure == 0;
                g_allocations_before_failure = -1;
                reservations = published && no_allocation
                    && registry.Find(owner)->identity == revoked
                    && !registry.Find(owner)->pair_raster_identity
                    && !registry.HasIdentityReservation(next)
                    && registry.FindByIdentity(revoked) == registry.Find(owner);
            }
            const DocumentIdentity forced_revoked =
                inkpod::app::UntitledDocumentIdentity(509U, 511U);
            if (reservations) {
                // A terminal Core revoke must clear the old pair even if the
                // frontend reservation was already lost.
                reservations = registry.AssignPairIdentities(
                    owner, next, next_pair);
            }
            if (reservations) {
                g_allocations_before_failure = 0;
                const bool forced = registry.ForceRevokeIdentity(
                    owner, forced_revoked);
                const bool no_allocation = g_allocations_before_failure == 0;
                g_allocations_before_failure = -1;
                reservations = forced && no_allocation
                    && registry.Find(owner)->identity == forced_revoked
                    && !registry.Find(owner)->pair_raster_identity
                    && registry.FindByIdentity(next) == nullptr
                    && registry.FindByIdentity(next_pair) == nullptr;
            }
            IdentityReservationToken prepared_token{};
            if (reservations) {
                prepared_token = registry.ReserveIdentityPair(
                    owner, next, next_pair, case_variant, pair_member_path);
                reservations = prepared_token
                    && prepared_token.value > revoke_token.value
                    && !registry.PublishPreparedIdentityPair(owner,
                        revoke_token, next, next_pair)
                    && registry.HasIdentityReservation(next)
                    && registry.HasIdentityReservation(next_pair);
            }
            if (reservations) {
                // Core commit publication consumes values prepared before the
                // apply fence and remains no-allocation, but only for the exact
                // operation that owns the still-live reservation.
                g_allocations_before_failure = 0;
                const bool committed = registry.PublishPreparedIdentityPair(
                    owner, prepared_token, next, next_pair);
                const bool no_allocation = g_allocations_before_failure == 0;
                g_allocations_before_failure = -1;
                reservations = committed && no_allocation
                    && registry.Find(owner)->identity == next
                    && registry.Find(owner)->pair_raster_identity == next_pair
                    && registry.ForceRevokeIdentity(owner, forced_revoked);
            }
            const auto removal_token = reservations
                ? registry.ReserveIdentity(owner, next, missing)
                : IdentityReservationToken{};
            reservations = reservations && removal_token
                && registry.Remove(owner)
                && !registry.HasIdentityReservation(next, normalized.normalized_path)
                && !registry.CancelIdentityReservation(owner, removal_token);
            const auto replacement_token = reservations
                ? registry.ReserveIdentity(other_owner, next)
                : IdentityReservationToken{};
            last_issued_token = replacement_token;
            reservations = reservations && replacement_token
                && replacement_token.value > removal_token.value
                && registry.Replace(DocumentSessionId{17U}, Generation{5U},
                    DocumentViewId{23U}, core)
                && !registry.HasIdentityReservation(next)
                && !registry.CancelIdentityReservation(
                    DocumentSessionId{17U}, replacement_token)
                && !registry.PublishReservedIdentity(
                    DocumentSessionId{17U}, replacement_token);
        }
    }
    if (reservations) {
        registry.Clear();
        const auto after_clear_owner = DocumentSessionId{31U};
        const auto after_clear_identity =
            inkpod::app::UntitledDocumentIdentity(701U, 703U);
        reservations = registry.InitializePlaceholder(Generation{7U})
            && registry.Replace(after_clear_owner, Generation{9U},
                DocumentViewId{29U}, core);
        const auto after_clear_token = reservations
            ? registry.ReserveIdentity(after_clear_owner, after_clear_identity)
            : IdentityReservationToken{};
        reservations = reservations && after_clear_token
            && after_clear_token.value > last_issued_token.value
            && !registry.CancelIdentityReservation(
                after_clear_owner, last_issued_token)
            && registry.HasIdentityReservation(after_clear_identity)
            && registry.CancelIdentityReservation(
                after_clear_owner, after_clear_token)
            && !registry.HasIdentityReservation(after_clear_identity);
    }
    registry.Clear();
    DeleteFileW(hard_link.c_str());
    DeleteFileW(path.c_str());
    return normalized_equal && indexed && reservations;
}

bool TestRevertPairSequenceBindingPublication() {
    DocumentRegistry registry;
    auto* core = reinterpret_cast<CoreHost*>(static_cast<std::uintptr_t>(1U));
    const DocumentSessionId owner{41U};
    const DocumentIdentity prior_native =
        inkpod::app::UntitledDocumentIdentity(101U, 103U);
    const DocumentIdentity prior_raster =
        inkpod::app::UntitledDocumentIdentity(107U, 109U);
    const DocumentIdentity next_native =
        inkpod::app::UntitledDocumentIdentity(201U, 203U);
    const DocumentIdentity next_raster =
        inkpod::app::UntitledDocumentIdentity(207U, 209U);
    const DocumentIdentity unrelated_raster =
        inkpod::app::UntitledDocumentIdentity(211U, 223U);
    const std::wstring next_native_path = L"C:\\cells\\next.inkpod";
    const std::wstring next_raster_path = L"C:\\cells\\next.png";
    if (!registry.InitializePlaceholder(Generation{1U})
        || !registry.Replace(
            owner, Generation{3U}, DocumentViewId{17U}, core)
        || !registry.AssignPairIdentities(
            owner, prior_native, prior_raster)) {
        return false;
    }
    auto* document = registry.Find(owner);
    if (document == nullptr) {
        return false;
    }
    inkpod::app::SequenceFileBinding prior_binding{};
    prior_binding.document_uuid_high = 301U;
    prior_binding.document_uuid_low = 307U;
    prior_binding.source_generation = 11U;
    prior_binding.raster_path = L"C:\\cells\\prior.png";
    prior_binding.raster_identity = prior_raster;
    std::vector<inkpod::app::SequenceFileBinding> bindings;
    bindings.push_back(std::move(prior_binding));
    if (!document->ReplaceSequenceFileBindings(std::move(bindings))) {
        return false;
    }
    g_allocations_before_failure = 0;
    const IdentityReservationToken allocation_failed = registry.ReserveIdentityPair(
        owner, next_native, next_raster,
        next_native_path, next_raster_path);
    g_allocations_before_failure = -1;
    const auto* after_failed_reservation = document->SequenceFileBindingAt(0U);
    if (allocation_failed || document->identity != prior_native
        || document->pair_raster_identity != prior_raster
        || after_failed_reservation == nullptr
        || after_failed_reservation->raster_path != L"C:\\cells\\prior.png"
        || after_failed_reservation->raster_identity != prior_raster
        || registry.HasIdentityReservation(next_native)
        || registry.HasIdentityReservation(next_raster)) {
        return false;
    }
    const IdentityReservationToken token = registry.ReserveIdentityPair(
        owner, next_native, next_raster,
        next_native_path, next_raster_path);
    if (!token) {
        return false;
    }
    const auto prepared_binding = [&](DocumentIdentity raster_identity,
                                      std::uint64_t uuid_low = 307U) {
        inkpod::app::SequenceFileBinding binding{};
        binding.document_uuid_high = 301U;
        binding.document_uuid_low = uuid_low;
        binding.source_generation = 11U;
        binding.raster_path = L"C:\\cells\\next.png";
        binding.raster_identity = std::move(raster_identity);
        return binding;
    };
    const auto unchanged = [&]() {
        const auto* binding = document->SequenceFileBindingAt(0U);
        return document->identity == prior_native
            && document->pair_raster_identity == prior_raster
            && binding != nullptr
            && binding->document_uuid_high == 301U
            && binding->document_uuid_low == 307U
            && binding->source_generation == 11U
            && binding->raster_path == L"C:\\cells\\prior.png"
            && binding->raster_identity == prior_raster
            && registry.HasIdentityReservation(next_native)
            && registry.HasIdentityReservation(next_raster);
    };

    auto stale_candidate = prepared_binding(next_raster);
    g_allocations_before_failure = 0;
    const bool stale_rejected =
        !registry.PublishReservedIdentityPairWithSequenceBinding(
            owner, IdentityReservationToken{token.value + 1U}, 0U,
            std::move(stale_candidate));
    const bool stale_no_allocation = g_allocations_before_failure == 0;
    g_allocations_before_failure = -1;
    if (!stale_rejected || !stale_no_allocation || !unchanged()) {
        return false;
    }

    auto wrong_key = prepared_binding(next_raster, 311U);
    if (registry.PublishReservedIdentityPairWithSequenceBinding(
            owner, token, 0U, std::move(wrong_key))
        || !unchanged()) {
        return false;
    }
    auto wrong_pair = prepared_binding(unrelated_raster);
    if (registry.PublishReservedIdentityPairWithSequenceBinding(
            owner, token, 0U, std::move(wrong_pair))
        || !unchanged()) {
        return false;
    }
    auto wrong_slot = prepared_binding(next_raster);
    if (registry.PublishReservedIdentityPairWithSequenceBinding(
            owner, token, 1U, std::move(wrong_slot))
        || !unchanged()) {
        return false;
    }

    auto committed_binding = prepared_binding(next_raster);
    g_allocations_before_failure = 0;
    const bool committed =
        registry.PublishReservedIdentityPairWithSequenceBinding(
            owner, token, 0U, std::move(committed_binding));
    const bool committed_no_allocation = g_allocations_before_failure == 0;
    g_allocations_before_failure = -1;
    const auto* published = document->SequenceFileBindingAt(0U);
    return committed && committed_no_allocation
        && document->identity == next_native
        && document->pair_raster_identity == next_raster
        && published != nullptr
        && published->document_uuid_high == 301U
        && published->document_uuid_low == 307U
        && published->source_generation == 11U
        && published->raster_path == L"C:\\cells\\next.png"
        && published->raster_identity == next_raster
        && !registry.HasIdentityReservation(next_native)
        && !registry.HasIdentityReservation(next_raster)
        && !registry.PublishReservedIdentityPairWithSequenceBinding(
            owner, token, 0U, prepared_binding(next_raster));
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
    const auto logical = inkpod::app::UntitledDocumentIdentity(91U, 93U);
    const auto physical = inkpod::app::UntitledDocumentIdentity(95U, 97U);
    if (!recent.Record(L"C:\\cells\\logical.png", logical)
        || !recent.Record(L"C:\\cells\\duplicate.inkpod", physical)
        || !recent.RecordReplacing(
            L"C:\\cells\\logical.inkpod", physical, logical)
        || recent.At(0U) == nullptr
        || recent.At(0U)->path != L"C:\\cells\\logical.inkpod"
        || recent.At(0U)->identity != physical) {
        return false;
    }
    std::size_t physical_count{};
    for (std::size_t index = 0U; index < recent.Count(); ++index) {
        const auto* entry = recent.At(index);
        if (entry != nullptr
            && (entry->identity == physical || entry->identity == logical)) {
            ++physical_count;
        }
    }
    if (physical_count != 1U) {
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
    const auto first_proof = RecoveryProof(101U);
    const auto second_proof = RecoveryProof(201U);
    auto reserved_proof = first_proof;
    reserved_proof.reserved = 1U;
    auto flagged_proof = first_proof;
    flagged_proof.native.flags = 2U;
    auto fake_physical_proof = first_proof;
    fake_physical_proof.metadata.identity.volume = 0U;
    fake_physical_proof.metadata.identity.object_high = 0U;
    fake_physical_proof.metadata.identity.object_low = 0U;
    if (document->PublishSequenceAutosave(
            71U, 73U, 5U, L"C:\\recovery\\invalid.inkpod",
            sequence_metadata, {})
        || document->PublishSequenceAutosave(
            71U, 73U, 5U, L"C:\\recovery\\reserved.inkpod",
            sequence_metadata, reserved_proof)
        || document->PublishSequenceAutosave(
            71U, 73U, 5U, L"C:\\recovery\\flagged.inkpod",
            sequence_metadata, flagged_proof)
        || document->PublishSequenceAutosave(
            71U, 73U, 5U, L"C:\\recovery\\fake.inkpod",
            sequence_metadata, fake_physical_proof)
        || !document->PublishSequenceAutosave(
            71U, 73U, 5U, L"C:\\recovery\\cell.inkpod", sequence_metadata,
            first_proof)
        || document->FindSequenceAutosave(71U, 73U, 4U) != nullptr) {
        return false;
    }
    const auto* sequence_autosave = document->FindSequenceAutosave(71U, 73U, 5U);
    inkpod::app::SequenceAutosaveBinding reserved_binding{};
    reserved_binding.document_uuid_high = 71U;
    reserved_binding.document_uuid_low = 73U;
    reserved_binding.source_generation = 5U;
    reserved_binding.recovery_path = L"C:\\recovery\\cell-2.inkpod";
    reserved_binding.metadata = sequence_metadata;
    reserved_binding.artifact_proof = second_proof;
    if (sequence_autosave == nullptr
        || sequence_autosave->artifact_generation != 1U
        || sequence_autosave->recovery_path != L"C:\\recovery\\cell.inkpod"
        || sequence_autosave->artifact_proof.native.identity.volume != 101U
        || !document->ReserveSequenceAutosave(71U, 73U, 5U, 1U)
        // A reserved continuation freezes this exact prior generation. A
        // newer publication/removal cannot win an ABA race while Core work is
        // in flight, and the wrong expected generation cannot publish.
        || document->PublishSequenceAutosave(
            71U, 73U, 5U, L"C:\\recovery\\racing.inkpod", sequence_metadata,
            RecoveryProof(251U))
        || document->RemoveSequenceAutosave(71U, 73U, 5U, 1U)
        || document->PublishReservedSequenceAutosave(
            reserved_binding, 0U)
        || !document->PublishReservedSequenceAutosave(
            std::move(reserved_binding), 1U)) {
        return false;
    }
    sequence_autosave = document->FindSequenceAutosave(71U, 73U, 5U);
    if (sequence_autosave == nullptr
        || sequence_autosave->artifact_generation != 2U
        || sequence_autosave->recovery_path != L"C:\\recovery\\cell-2.inkpod"
        || sequence_autosave->artifact_proof.native.identity.volume != 201U
        || document->RemoveSequenceAutosave(71U, 73U, 5U, 1U)
        || !document->RemoveSequenceAutosave(71U, 73U, 5U, 2U)
        || document->FindSequenceAutosave(71U, 73U, 5U) != nullptr
        || document->RemoveSequenceAutosave(71U, 73U, 5U, 2U)) {
        return false;
    }
    auto inactive_metadata = sequence_metadata;
    inactive_metadata.document_uuid_high = 81U;
    inactive_metadata.document_uuid_low = 83U;
    if (!document->PublishSequenceAutosave(
            71U, 73U, 5U, L"C:\\recovery\\cell-3.inkpod", sequence_metadata,
            RecoveryProof(301U))
        || !document->PublishSequenceAutosave(
            81U, 83U, 6U, L"C:\\recovery\\inactive.inkpod",
            inactive_metadata, RecoveryProof(401U))) {
        return false;
    }
    auto retired = document->TakeSequenceAutosave(71U, 73U, 5U);
    if (!retired.has_value()
        || retired->recovery_path != L"C:\\recovery\\cell-3.inkpod"
        || document->FindSequenceAutosave(71U, 73U, 5U) != nullptr
        || document->FindSequenceAutosave(81U, 83U, 6U) == nullptr
        || document->TakeSequenceAutosave(71U, 73U, 5U).has_value()) {
        return false;
    }
    document->ClearSequenceAutosaves();
    if (document->FindSequenceAutosave(71U, 73U, 5U) != nullptr
        || document->FindSequenceAutosave(81U, 83U, 6U) != nullptr) {
        return false;
    }
    inkpod::app::DocumentIdentity raster_identity{};
    raster_identity.kind = inkpod::app::DocumentIdentityKind::NormalizedPath;
    raster_identity.normalized_path = L"c:\\cells\\cell.png";
    inkpod::app::SequenceFileBinding file_binding{};
    file_binding.document_uuid_high = 71U;
    file_binding.document_uuid_low = 73U;
    file_binding.source_generation = 5U;
    file_binding.raster_path = L"C:\\cells\\cell.png";
    file_binding.raster_identity = raster_identity;
    std::vector<inkpod::app::SequenceFileBinding> file_bindings;
    file_bindings.push_back(std::move(file_binding));
    inkpod::app::DocumentIdentity updated_raster_identity{};
    updated_raster_identity.kind = inkpod::app::DocumentIdentityKind::NormalizedPath;
    updated_raster_identity.normalized_path = L"c:\\cells\\saved.png";
    if (!document->ReplaceSequenceFileBindings(std::move(file_bindings))
        || !document->UpdateSequenceFileBinding(0U, 81U, 83U, 7U,
            L"C:\\cells\\saved.png", updated_raster_identity)) {
        return false;
    }
    const auto* updated_binding = document->SequenceFileBindingAt(0U);
    if (updated_binding == nullptr || updated_binding->document_uuid_high != 81U
        || updated_binding->document_uuid_low != 83U
        || updated_binding->source_generation != 7U
        || updated_binding->raster_path != L"C:\\cells\\saved.png"
        || updated_binding->raster_identity != updated_raster_identity) {
        return false;
    }
    inkpod::app::SequenceFileBinding published_binding{};
    published_binding.document_uuid_high = 91U;
    published_binding.document_uuid_low = 93U;
    published_binding.source_generation = 9U;
    published_binding.raster_path = L"C:\\cells\\published.png";
    published_binding.raster_identity = updated_raster_identity;
    if (!document->PublishSequenceFileBinding(
            0U, std::move(published_binding))) {
        return false;
    }
    updated_binding = document->SequenceFileBindingAt(0U);
    if (updated_binding == nullptr
        || updated_binding->document_uuid_high != 91U
        || updated_binding->document_uuid_low != 93U
        || updated_binding->source_generation != 9U
        || updated_binding->raster_path != L"C:\\cells\\published.png") {
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
    if (!TestRevertPairSequenceBindingPublication()) {
        std::cerr << "Revert pair/sequence binding publication test failed\n";
        return 11;
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
