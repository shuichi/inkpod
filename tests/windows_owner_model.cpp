#include <array>
#include <cstdlib>
#include <cstdint>
#include <cwchar>
#include <cwctype>
#include <iostream>
#include <new>
#include <string>

#include "app/application_owner_graph.h"
#include "app/document_session.h"
#include "app/recent_documents.h"
#include "app/workspace_window.h"

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
using inkpod::app::Generation;
using inkpod::app::RecentDocumentList;
using inkpod::app::WorkspaceWindowId;
using inkpod::app::WorkspaceWindowRegistry;

bool TestDocumentIdentityAndIndex() {
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
    if (!inkpod::app::ResolveDocumentFileIdentity(path, direct)
        || !inkpod::app::ResolveDocumentFileIdentity(hard_link, alias)
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
                             relative_name + 1, relative)
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
        inkpod::app::ResolveDocumentFileIdentity(missing, normalized)
        && inkpod::app::ResolveDocumentFileIdentity(
            case_variant, normalized_case)
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
    registry.Clear();
    DeleteFileW(hard_link.c_str());
    DeleteFileW(path.c_str());
    return normalized_equal && indexed;
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
    if (registry.Initialize(nullptr, WorkspaceWindowId{5U}, Generation{7U})
        || registry.Initialize(owner, {}, Generation{7U})
        || registry.Initialize(owner, WorkspaceWindowId{5U}, {})
        || registry.Current() != nullptr
        || !registry.Initialize(owner, WorkspaceWindowId{5U}, Generation{7U})
        || registry.Current() == nullptr
        || registry.Current()->application != owner
        || registry.Current()->id != WorkspaceWindowId{5U}
        || registry.Current()->generation != Generation{7U}) {
        return false;
    }
    registry.Current()->windows.window = reinterpret_cast<HWND>(
        static_cast<std::uintptr_t>(31U));
    if (registry.Initialize(owner, {}, Generation{10U})
        || registry.Current()->id != WorkspaceWindowId{5U}
        || registry.Current()->windows.window == nullptr) {
        return false;
    }
    if (!registry.Initialize(owner, WorkspaceWindowId{9U}, Generation{10U})
        || registry.Current() == nullptr
        || registry.Current()->id != WorkspaceWindowId{9U}
        || registry.Current()->windows.window != nullptr) {
        return false;
    }
    registry.Clear();
    return registry.Current() == nullptr;
}

}  // namespace

int main() {
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
    return 0;
}
