#include "session_recovery.h"

#include <algorithm>
#include <array>
#include <atomic>
#include <cstring>
#include <limits>
#include <new>
#include <string_view>
#include <utility>

#include "application_data_paths.h"

namespace inkpod::app {
namespace {

constexpr std::size_t kMaximumMetadataBytes = 512U * 1024U;
constexpr std::uint32_t kSessionPathsMagic = UINT32_C(0x53524b49);
constexpr std::uint16_t kSessionPathsVersion = 1U;
constexpr std::size_t kMaximumSessionRecordBytes = 1024U * 1024U;
constexpr std::size_t kMaximumRestoredDocumentPaths = 64U;

void AppendU16(std::vector<std::uint8_t>& bytes, std::uint16_t value) {
    bytes.push_back(static_cast<std::uint8_t>(value & 0xffU));
    bytes.push_back(static_cast<std::uint8_t>((value >> 8U) & 0xffU));
}

void AppendU32(std::vector<std::uint8_t>& bytes, std::uint32_t value) {
    for (std::uint32_t shift = 0U; shift < 32U; shift += 8U) {
        bytes.push_back(static_cast<std::uint8_t>((value >> shift) & 0xffU));
    }
}

bool ReadU16(
    const std::uint8_t* bytes,
    std::size_t length,
    std::size_t& cursor,
    std::uint16_t& value) noexcept {
    if (bytes == nullptr || cursor > length || length - cursor < 2U) {
        return false;
    }
    value = static_cast<std::uint16_t>(bytes[cursor])
        | static_cast<std::uint16_t>(bytes[cursor + 1U]) << 8U;
    cursor += 2U;
    return true;
}

bool ReadU32(
    const std::uint8_t* bytes,
    std::size_t length,
    std::size_t& cursor,
    std::uint32_t& value) noexcept {
    if (bytes == nullptr || cursor > length || length - cursor < 4U) {
        return false;
    }
    value = 0U;
    for (std::uint32_t shift = 0U; shift < 32U; shift += 8U) {
        value |= static_cast<std::uint32_t>(bytes[cursor++]) << shift;
    }
    return true;
}

bool WideToUtf8(std::wstring_view text, std::vector<std::uint8_t>& output) {
    if (text.size() > 32767U
        || std::find(text.begin(), text.end(), L'\0') != text.end()) {
        return false;
    }
    if (text.empty()) {
        output.clear();
        return true;
    }
    const int required = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        text.data(),
        static_cast<int>(text.size()),
        nullptr,
        0,
        nullptr,
        nullptr);
    if (required <= 0) {
        return false;
    }
    output.resize(static_cast<std::size_t>(required));
    return WideCharToMultiByte(
               CP_UTF8,
               WC_ERR_INVALID_CHARS,
               text.data(),
               static_cast<int>(text.size()),
               reinterpret_cast<char*>(output.data()),
               required,
               nullptr,
               nullptr)
        == required;
}

bool Utf8ToWide(
    const std::uint8_t* bytes,
    std::size_t length,
    std::wstring& output) {
    if (length == 0U) {
        output.clear();
        return true;
    }
    if (bytes == nullptr || length > static_cast<std::size_t>(INT_MAX)
        || std::find(bytes, bytes + length, std::uint8_t{0U}) != bytes + length) {
        return false;
    }
    const int required = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(bytes),
        static_cast<int>(length),
        nullptr,
        0);
    if (required <= 0 || required > 32767) {
        return false;
    }
    output.resize(static_cast<std::size_t>(required));
    return MultiByteToWideChar(
               CP_UTF8,
               MB_ERR_INVALID_CHARS,
               reinterpret_cast<const char*>(bytes),
               static_cast<int>(length),
               output.data(),
               required)
        == required;
}

void AppendString(
    std::vector<std::uint8_t>& bytes,
    const std::vector<std::uint8_t>& text) {
    AppendU32(bytes, static_cast<std::uint32_t>(text.size()));
    bytes.insert(bytes.end(), text.begin(), text.end());
}

bool ReadString(
    const std::uint8_t* bytes,
    std::size_t length,
    std::size_t& cursor,
    std::wstring& output) {
    std::uint32_t byte_count{};
    if (!ReadU32(bytes, length, cursor, byte_count) || cursor > length
        || byte_count > length - cursor) {
        return false;
    }
    if (!Utf8ToWide(bytes + cursor, byte_count, output)) {
        return false;
    }
    cursor += byte_count;
    return true;
}

bool ReadFileBounded(
    const std::wstring& path,
    std::size_t maximum,
    std::vector<std::uint8_t>& output) noexcept {
    HANDLE file = CreateFileW(
        path.c_str(),
        GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    LARGE_INTEGER size{};
    if (GetFileSizeEx(file, &size) == FALSE || size.QuadPart < 0
        || static_cast<std::uint64_t>(size.QuadPart) > maximum) {
        CloseHandle(file);
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(size.QuadPart));
    } catch (const std::bad_alloc&) {
        CloseHandle(file);
        return false;
    }
    DWORD read{};
    const bool success = output.empty()
        || (ReadFile(
                file,
                output.data(),
                static_cast<DWORD>(output.size()),
                &read,
                nullptr)
                != FALSE
            && read == output.size());
    CloseHandle(file);
    return success;
}

bool WriteFileAtomic(
    const std::wstring& path,
    const std::vector<std::uint8_t>& bytes) noexcept {
    static std::atomic<std::uint32_t> sequence{1U};
    std::wstring temporary;
    try {
        temporary = path + L".tmp." + std::to_wstring(GetCurrentProcessId())
            + L"." + std::to_wstring(sequence.fetch_add(1U));
    } catch (const std::bad_alloc&) {
        return false;
    }
    HANDLE file = CreateFileW(
        temporary.c_str(),
        GENERIC_WRITE,
        0U,
        nullptr,
        CREATE_NEW,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    DWORD written{};
    const bool wrote = bytes.empty()
        || (WriteFile(
                file,
                bytes.data(),
                static_cast<DWORD>(bytes.size()),
                &written,
                nullptr)
                != FALSE
            && written == bytes.size());
    const bool flushed = wrote && FlushFileBuffers(file) != FALSE;
    CloseHandle(file);
    const bool replaced = flushed
        && MoveFileExW(
               temporary.c_str(),
               path.c_str(),
               MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
            != FALSE;
    if (!replaced) {
        DeleteFileW(temporary.c_str());
    }
    return replaced;
}

bool DeleteIfPresent(const std::wstring& path) noexcept {
    return DeleteFileW(path.c_str()) != FALSE || GetLastError() == ERROR_FILE_NOT_FOUND;
}

bool ValidIdentity(const DocumentIdentity& identity) noexcept {
    const bool has_file_id = identity.volume_serial != 0U
        || std::any_of(
            identity.file_id.begin(), identity.file_id.end(),
            [](std::uint8_t value) { return value != 0U; });
    const bool has_uuid = identity.uuid_high != 0U || identity.uuid_low != 0U;
    switch (identity.kind) {
        case DocumentIdentityKind::None:
            return !has_file_id && identity.normalized_path.empty() && !has_uuid;
        case DocumentIdentityKind::WindowsFile:
            return has_file_id && identity.normalized_path.empty() && !has_uuid;
        case DocumentIdentityKind::NormalizedPath:
            return !has_file_id && !identity.normalized_path.empty() && !has_uuid;
        case DocumentIdentityKind::Untitled:
            return !has_file_id && identity.normalized_path.empty() && has_uuid;
    }
    return false;
}

}  // namespace

bool RecoveryRootDirectory(std::wstring& output) noexcept {
    return ResolveApplicationDataDirectory(
        ApplicationDataDirectory::Recovery, output);
}

bool RecoveryMetadataPath(
    const std::wstring& recovery_path,
    std::wstring& output) noexcept {
    if (recovery_path.empty() || recovery_path.size() > 32758U) {
        return false;
    }
    try {
        output = recovery_path + L".metadata";
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool BuildRecoveryMetadata(
    DocumentSessionId session,
    Generation generation,
    const DocumentIdentity& identity,
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    const std::wstring& current_path,
    const std::wstring& source_path,
    RecoveryMetadata& output) noexcept {
    if (!session || !generation || !ValidIdentity(identity)
        || (document_uuid_high == 0U && document_uuid_low == 0U)) {
        return false;
    }
    RecoveryMetadata metadata{};
    metadata.session = session;
    metadata.generation = generation;
    metadata.document_uuid_high = document_uuid_high;
    metadata.document_uuid_low = document_uuid_low;
    // The Rust recovery writer supplies the durable metadata timestamp.
    metadata.written_file_time = 0U;
    try {
        metadata.original_identity = identity;
        metadata.original_path = current_path;
        metadata.source_path = source_path;
    } catch (const std::bad_alloc&) {
        return false;
    }
    output = std::move(metadata);
    return true;
}

bool RecoveryMetadataToAbi(
    const RecoveryMetadata& metadata,
    InkpodIoRecoveryMetadata& output,
    std::vector<std::uint8_t>& text) noexcept {
    if (!ValidIdentity(metadata.original_identity)) {
        return false;
    }
    try {
        std::array<std::vector<std::uint8_t>, 3U> strings;
        if (!WideToUtf8(metadata.original_path, strings[0])
            || !WideToUtf8(metadata.source_path, strings[1])
            || !WideToUtf8(metadata.original_identity.normalized_path, strings[2])) {
            return false;
        }
        const std::size_t total = strings[0].size() + strings[1].size() + strings[2].size();
        if (total > kMaximumMetadataBytes) {
            return false;
        }
        text.clear();
        text.reserve(total);
        for (const auto& string : strings) {
            text.insert(text.end(), string.begin(), string.end());
        }
        InkpodIoRecoveryMetadata result{};
        result.struct_size = static_cast<std::uint32_t>(sizeof(result));
        result.flags = 1U;
        result.session_id = metadata.session.Value();
        result.generation = metadata.generation.Value();
        result.document_uuid_high = metadata.document_uuid_high;
        result.document_uuid_low = metadata.document_uuid_low;
        result.written_time_100ns = metadata.written_file_time;
        result.identity_kind = static_cast<std::uint32_t>(metadata.original_identity.kind);
        result.identity_volume = metadata.original_identity.volume_serial;
        if (metadata.original_identity.kind == DocumentIdentityKind::Untitled) {
            result.identity_object_high = metadata.original_identity.uuid_high;
            result.identity_object_low = metadata.original_identity.uuid_low;
        } else if (metadata.original_identity.kind == DocumentIdentityKind::WindowsFile) {
            std::memcpy(&result.identity_object_low, metadata.original_identity.file_id.data(), 8U);
            std::memcpy(&result.identity_object_high, metadata.original_identity.file_id.data() + 8U, 8U);
        }
        std::array<InkpodIoPath*, 3U> spans{
            &result.original_path, &result.source_path, &result.identity_path};
        std::size_t offset{};
        for (std::size_t index = 0U; index < spans.size(); ++index) {
            auto& span = *spans[index];
            span.struct_size = static_cast<std::uint32_t>(sizeof(span));
            span.path = strings[index].empty() ? nullptr : text.data() + offset;
            span.path_bytes = strings[index].size();
            offset += strings[index].size();
        }
        output = result;
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool RecoveryMetadataFromAbi(
    const InkpodIoRecoveryMetadata& input,
    RecoveryMetadata& output) noexcept {
    if (input.struct_size < sizeof(input) || (input.flags & 1U) == 0U
        || input.reserved != 0U || input.identity_kind > 3U) {
        return false;
    }
    try {
        RecoveryMetadata result{};
        result.session = DocumentSessionId(input.session_id);
        result.generation = Generation(input.generation);
        result.document_uuid_high = input.document_uuid_high;
        result.document_uuid_low = input.document_uuid_low;
        result.written_file_time = input.written_time_100ns;
        result.original_identity.kind = static_cast<DocumentIdentityKind>(input.identity_kind);
        result.original_identity.volume_serial = input.identity_volume;
        if (result.original_identity.kind == DocumentIdentityKind::Untitled) {
            result.original_identity.uuid_high = input.identity_object_high;
            result.original_identity.uuid_low = input.identity_object_low;
        } else if (result.original_identity.kind == DocumentIdentityKind::WindowsFile) {
            std::memcpy(result.original_identity.file_id.data(), &input.identity_object_low, 8U);
            std::memcpy(result.original_identity.file_id.data() + 8U, &input.identity_object_high, 8U);
        }
        const std::array<const InkpodIoPath*, 3U> spans{
            &input.original_path, &input.source_path, &input.identity_path};
        const std::array<std::wstring*, 3U> strings{
            &result.original_path, &result.source_path, &result.original_identity.normalized_path};
        for (std::size_t index = 0U; index < spans.size(); ++index) {
            const auto& span = *spans[index];
            if (span.struct_size < sizeof(span) || span.reserved != 0U
                || span.path_bytes > kMaximumMetadataBytes
                || !Utf8ToWide(span.path, static_cast<std::size_t>(span.path_bytes), *strings[index])) {
                return false;
            }
        }
        if (!ValidIdentity(result.original_identity)) {
            return false;
        }
        output = std::move(result);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool EncodeRecoveryMetadata(
    const RecoveryMetadata& metadata,
    std::vector<std::uint8_t>& output) noexcept {
    try {
        InkpodIoRecoveryMetadata record{};
        std::vector<std::uint8_t> text;
        std::uint64_t required{};
        if (!RecoveryMetadataToAbi(metadata, record, text)
            || inkpod_recovery_metadata_encode(&record, nullptr, 0U, &required) != INKPOD_STATUS_OK
            || required > kMaximumMetadataBytes) {
            return false;
        }
        std::vector<std::uint8_t> encoded(static_cast<std::size_t>(required));
        if (inkpod_recovery_metadata_encode(&record, encoded.data(), encoded.size(), &required)
            != INKPOD_STATUS_OK) {
            return false;
        }
        output = std::move(encoded);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool DecodeRecoveryMetadata(
    const std::uint8_t* bytes,
    std::size_t length,
    RecoveryMetadata& output) noexcept {
    try {
        InkpodIoRecoveryMetadata record{};
        record.struct_size = static_cast<std::uint32_t>(sizeof(record));
        std::uint64_t required{};
        if (inkpod_recovery_metadata_decode(bytes, length, &record, nullptr, 0U, &required)
                != INKPOD_STATUS_OK
            || required > kMaximumMetadataBytes) {
            return false;
        }
        std::vector<std::uint8_t> text(static_cast<std::size_t>(required));
        if (inkpod_recovery_metadata_decode(
                bytes, length, &record, text.data(), text.size(), &required) != INKPOD_STATUS_OK) {
            return false;
        }
        return RecoveryMetadataFromAbi(record, output);
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool SequenceRecoveryPath(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation,
    std::wstring& output) noexcept {
    if ((document_uuid_high == 0U && document_uuid_low == 0U)
        || source_generation == 0U) {
        return false;
    }
    std::wstring directory;
    if (!RecoveryRootDirectory(directory)) {
        return false;
    }
    std::array<wchar_t, 128U> name{};
    _snwprintf_s(
        name.data(),
        name.size(),
        _TRUNCATE,
        L"\\%016llx%016llx-sequence-%016llx.inkpod",
        static_cast<unsigned long long>(document_uuid_high),
        static_cast<unsigned long long>(document_uuid_low),
        static_cast<unsigned long long>(source_generation));
    try {
        output = directory + name.data();
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool LoadPreviousDocumentPaths(
    std::vector<std::wstring>& paths) noexcept {
    std::wstring path;
    if (!ResolveApplicationSessionPath(path)) {
        return false;
    }
    const DWORD attributes = GetFileAttributesW(path.c_str());
    const DWORD attribute_error = GetLastError();
    if (attributes == INVALID_FILE_ATTRIBUTES
        && (attribute_error == ERROR_FILE_NOT_FOUND
            || attribute_error == ERROR_PATH_NOT_FOUND)) {
        paths.clear();
        return true;
    }
    if (attributes == INVALID_FILE_ATTRIBUTES) {
        return false;
    }
    std::vector<std::uint8_t> bytes;
    if (!ReadFileBounded(path, kMaximumSessionRecordBytes, bytes)) {
        return false;
    }
    return DecodePreviousDocumentPaths(bytes.data(), bytes.size(), paths);
}

bool DecodePreviousDocumentPaths(
    const std::uint8_t* input,
    std::size_t length,
    std::vector<std::wstring>& paths) noexcept {
    if (input == nullptr || length < 16U
        || length > kMaximumSessionRecordBytes) {
        return false;
    }
    std::size_t cursor{};
    std::uint32_t magic{};
    std::uint16_t version{};
    std::uint16_t reserved{};
    std::uint32_t total{};
    std::uint32_t count{};
    if (!ReadU32(input, length, cursor, magic)
        || !ReadU16(input, length, cursor, version)
        || !ReadU16(input, length, cursor, reserved)
        || !ReadU32(input, length, cursor, total)
        || !ReadU32(input, length, cursor, count)
        || magic != kSessionPathsMagic || version != kSessionPathsVersion
        || reserved != 0U || total != length
        || count > kMaximumRestoredDocumentPaths) {
        return false;
    }
    std::vector<std::wstring> decoded;
    try {
        decoded.reserve(count);
        for (std::uint32_t index = 0U; index < count; ++index) {
            std::wstring path;
            if (!ReadString(input, length, cursor, path)
                || path.empty()) {
                return false;
            }
            decoded.push_back(std::move(path));
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (cursor != length) {
        return false;
    }
    paths = std::move(decoded);
    return true;
}

bool EncodePreviousDocumentPaths(
    std::span<const std::wstring> paths,
    std::vector<std::uint8_t>& bytes) noexcept {
    if (paths.size() > kMaximumRestoredDocumentPaths) {
        return false;
    }
    try {
        bytes.clear();
        bytes.reserve(16U);
        AppendU32(bytes, kSessionPathsMagic);
        AppendU16(bytes, kSessionPathsVersion);
        AppendU16(bytes, 0U);
        AppendU32(bytes, 0U);
        AppendU32(bytes, static_cast<std::uint32_t>(paths.size()));
        for (const auto& path : paths) {
            std::vector<std::uint8_t> utf8;
            if (path.empty() || !WideToUtf8(path, utf8)
                || bytes.size() > kMaximumSessionRecordBytes - 4U
                || utf8.size() > kMaximumSessionRecordBytes - bytes.size() - 4U) {
                return false;
            }
            AppendString(bytes, utf8);
        }
        const std::uint32_t total = static_cast<std::uint32_t>(bytes.size());
        for (std::size_t index = 0U; index < 4U; ++index) {
            bytes[8U + index] = static_cast<std::uint8_t>(
                (total >> (index * 8U)) & 0xffU);
        }
    } catch (const std::bad_alloc&) {
        bytes.clear();
        return false;
    }
    return true;
}

bool SavePreviousDocumentPaths(
    std::span<const std::wstring> paths) noexcept {
    std::vector<std::uint8_t> bytes;
    if (!EncodePreviousDocumentPaths(paths, bytes)) {
        return false;
    }
    std::wstring directory;
    std::wstring path;
    if (!EnsureApplicationDataDirectory(
            ApplicationDataDirectory::Session, directory)
        || !ResolveApplicationSessionPath(path)) {
        return false;
    }
    return WriteFileAtomic(path, bytes);
}

bool ClearPreviousDocumentPaths() noexcept {
    std::wstring path;
    if (!ResolveApplicationSessionPath(path)) {
        return false;
    }
    return DeleteIfPresent(path);
}

}  // namespace inkpod::app
