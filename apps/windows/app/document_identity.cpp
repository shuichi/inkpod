#include "document_identity.h"

#include <windows.h>

#include <algorithm>
#include <array>
#include <cstring>
#include <new>
#include <utility>

namespace inkpod::app {
namespace {

bool NormalizeAbsolutePath(
    const std::wstring& path,
    std::wstring& output) noexcept {
    if (path.empty() || path.size() >= 32768U) {
        return false;
    }
    std::array<wchar_t, 32768U> absolute{};
    const DWORD length = GetFullPathNameW(
        path.c_str(), static_cast<DWORD>(absolute.size()), absolute.data(), nullptr);
    if (length == 0U || length >= absolute.size()) {
        return false;
    }
    try {
        output.assign(absolute.data(), length);
        std::replace(output.begin(), output.end(), L'/', L'\\');
        while (output.size() > 3U && output.back() == L'\\') {
            output.pop_back();
        }
    } catch (const std::bad_alloc&) {
        output.clear();
        return false;
    }
    if (!output.empty()
        && LCMapStringEx(
               LOCALE_NAME_INVARIANT,
               LCMAP_LOWERCASE,
               output.data(),
               static_cast<int>(output.size()),
               output.data(),
               static_cast<int>(output.size()),
               nullptr,
               nullptr,
               0U)
            == 0) {
        output.clear();
        return false;
    }
    return true;
}

}  // namespace

bool operator==(
    const DocumentIdentity& left,
    const DocumentIdentity& right) noexcept {
    if (left.kind != right.kind) {
        return false;
    }
    switch (left.kind) {
        case DocumentIdentityKind::None:
            return true;
        case DocumentIdentityKind::WindowsFile:
            return left.volume_serial == right.volume_serial
                && left.file_id == right.file_id;
        case DocumentIdentityKind::NormalizedPath:
            return left.normalized_path == right.normalized_path;
        case DocumentIdentityKind::Untitled:
            return left.uuid_high == right.uuid_high
                && left.uuid_low == right.uuid_low;
    }
    return false;
}

bool ResolveDocumentFileIdentity(
    const std::wstring& path,
    DocumentIdentity& output) noexcept {
    DocumentIdentity candidate{};
    HANDLE file = CreateFileW(
        path.c_str(),
        FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS,
        nullptr);
    if (file != INVALID_HANDLE_VALUE) {
        FILE_ID_INFO info{};
        if (GetFileInformationByHandleEx(
                file, FileIdInfo, &info, sizeof(info)) != FALSE) {
            candidate.kind = DocumentIdentityKind::WindowsFile;
            candidate.volume_serial = info.VolumeSerialNumber;
            static_assert(sizeof(candidate.file_id) == sizeof(info.FileId.Identifier));
            std::memcpy(
                candidate.file_id.data(),
                info.FileId.Identifier,
                candidate.file_id.size());
        }
        CloseHandle(file);
        if (candidate) {
            output = std::move(candidate);
            return true;
        }
    }
    candidate.kind = DocumentIdentityKind::NormalizedPath;
    if (!NormalizeAbsolutePath(path, candidate.normalized_path)) {
        return false;
    }
    output = std::move(candidate);
    return true;
}

DocumentIdentity UntitledDocumentIdentity(
    std::uint64_t uuid_high,
    std::uint64_t uuid_low) noexcept {
    DocumentIdentity identity{};
    if (uuid_high != 0U || uuid_low != 0U) {
        identity.kind = DocumentIdentityKind::Untitled;
        identity.uuid_high = uuid_high;
        identity.uuid_low = uuid_low;
    }
    return identity;
}

}  // namespace inkpod::app
