#include "document_identity.h"

#include "inkpod/core_ffi.h"

#include <windows.h>

#include <algorithm>
#include <array>
#include <cstring>
#include <new>
#include <string_view>
#include <utility>
#include <vector>

namespace inkpod::app {
namespace {

bool IsReservedWin32DeviceName(std::wstring_view component) noexcept {
    auto stem = component.substr(0U, component.find(L'.'));
    while (!stem.empty() && stem.back() == L' ') {
        stem.remove_suffix(1U);
    }
    std::array<wchar_t, 7U> uppercase{};
    if (stem.size() > uppercase.size()) {
        return false;
    }
    for (std::size_t index = 0U; index < stem.size(); ++index) {
        const wchar_t value = stem[index];
        uppercase[index] = value >= L'a' && value <= L'z'
            ? static_cast<wchar_t>(value - L'a' + L'A') : value;
    }
    const std::wstring_view name(uppercase.data(), stem.size());
    if (name == L"CON" || name == L"PRN" || name == L"AUX" || name == L"NUL"
        || name == L"CONIN$" || name == L"CONOUT$" || name == L"CLOCK$") {
        return true;
    }
    if (name.size() != 4U
        || (!name.starts_with(L"COM") && !name.starts_with(L"LPT"))) {
        return false;
    }
    const wchar_t suffix = name.back();
    return (suffix >= L'1' && suffix <= L'9')
        || suffix == L'\u00b9' || suffix == L'\u00b2' || suffix == L'\u00b3';
}

bool HasEquivalentWin32Components(
    std::wstring_view path,
    std::size_t first_component,
    std::size_t minimum_components) noexcept {
    std::size_t count = 0U;
    while (first_component < path.size()) {
        const auto end = path.find(L'\\', first_component);
        const auto component = path.substr(first_component,
            end == std::wstring_view::npos ? end : end - first_component);
        if (component.empty() || component.back() == L'.' || component.back() == L' '
            || component.find_first_of(L"/<>:\"|?*") != std::wstring_view::npos
            || std::any_of(component.begin(), component.end(),
                [](wchar_t value) { return value < L' '; })
            || IsReservedWin32DeviceName(component)) {
            return false;
        }
        ++count;
        if (end == std::wstring_view::npos) {
            break;
        }
        first_component = end + 1U;
    }
    return count >= minimum_components;
}

}  // namespace

bool NormalizeDocumentFilePath(
    const std::wstring& path,
    std::wstring& output) noexcept {
    if (path.empty() || path.size() >= 32768U || path.find(L'\0') != std::wstring::npos) {
        return false;
    }
    try {
        const std::wstring_view source(path);
        std::wstring ordinary;
        bool normalize = !source.starts_with(L"\\\\?\\");
        if (!normalize && source.size() >= 7U && source[5U] == L':'
            && source[6U] == L'\\'
            && ((source[4U] >= L'A' && source[4U] <= L'Z')
                || (source[4U] >= L'a' && source[4U] <= L'z'))
            && HasEquivalentWin32Components(source, 7U, 0U)) {
            ordinary.assign(source.substr(4U));
            normalize = true;
        } else if (!normalize && source.size() >= 8U && source[7U] == L'\\'
            && CompareStringOrdinal(source.data() + 4U, 3, L"UNC", 3, TRUE) == CSTR_EQUAL
            && HasEquivalentWin32Components(source, 8U, 2U)) {
            ordinary.assign(L"\\\\");
            ordinary.append(source.substr(8U));
            normalize = true;
        }
        if (normalize) {
            std::array<wchar_t, 32768U> absolute{};
            const DWORD length = GetFullPathNameW(
                ordinary.empty() ? path.c_str() : ordinary.c_str(),
                static_cast<DWORD>(absolute.size()), absolute.data(), nullptr);
            if (length == 0U || length >= absolute.size()) {
                return false;
            }
            output.assign(absolute.data(), length);
            std::replace(output.begin(), output.end(), L'/', L'\\');
            while (output.size() > 3U && output.back() == L'\\') {
                output.pop_back();
            }
        } else {
            // GetFullPathName also normalizes explicit verbatim paths. Keep
            // their syntax when stripping it could resolve a different file
            // (trailing dot/space, DOS device, relative component or namespace).
            output = path;
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
    const InkpodIoManager* manager,
    const std::wstring& path,
    DocumentIdentity& output) noexcept {
    if (manager == nullptr || path.empty() || path.size() >= 32768U
        || path.find(L'\0') != std::wstring::npos) {
        return false;
    }
    const int length = WideCharToMultiByte(
        CP_UTF8, WC_ERR_INVALID_CHARS, path.data(), static_cast<int>(path.size()),
        nullptr, 0, nullptr, nullptr);
    if (length <= 0) {
        return false;
    }
    std::vector<std::uint8_t> encoded_path;
    try {
        encoded_path.resize(static_cast<std::size_t>(length));
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (WideCharToMultiByte(
            CP_UTF8, WC_ERR_INVALID_CHARS, path.data(), static_cast<int>(path.size()),
            reinterpret_cast<char*>(encoded_path.data()), length, nullptr, nullptr)
        != length) {
        return false;
    }
    InkpodIoFileIdentity identity{};
    identity.struct_size = sizeof(identity);
    if (inkpod_io_resolve_identity(
            manager, encoded_path.data(), encoded_path.size(), &identity)
        != INKPOD_STATUS_OK) {
        return false;
    }

    DocumentIdentity candidate{};
    if (identity.kind == 1U) {
        candidate.kind = DocumentIdentityKind::WindowsFile;
        candidate.volume_serial = identity.volume;
        static_assert(sizeof(candidate.file_id) == 2U * sizeof(std::uint64_t));
        std::memcpy(candidate.file_id.data(), &identity.object_low, sizeof(std::uint64_t));
        std::memcpy(
            candidate.file_id.data() + sizeof(std::uint64_t),
            &identity.object_high,
            sizeof(std::uint64_t));
    } else if (identity.kind == 2U) {
        candidate.kind = DocumentIdentityKind::NormalizedPath;
        // Keep the frontend's string identity for a destination that is absent.
        // Normalization is pure path processing; Rust alone checks the file.
        if (!NormalizeDocumentFilePath(path, candidate.normalized_path)) {
            return false;
        }
    } else {
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
