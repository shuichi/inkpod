#include "session_recovery.h"

#include <shlobj.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cstring>
#include <limits>
#include <new>
#include <string_view>
#include <utility>

namespace inkpod::app {
namespace {

constexpr std::uint32_t kMetadataMagic = UINT32_C(0x4d524b49);
constexpr std::uint16_t kMetadataVersion = 1U;
constexpr std::uint16_t kMetadataHeaderBytes = 100U;
constexpr std::size_t kMaximumMetadataBytes = 512U * 1024U;
constexpr std::uint32_t kSessionPathsMagic = UINT32_C(0x53524b49);
constexpr std::uint16_t kSessionPathsVersion = 1U;
constexpr std::uint32_t kSequenceSwitchPolicyMagic = UINT32_C(0x50534b49);
constexpr std::uint16_t kSequenceSwitchPolicyVersion = 1U;
constexpr std::size_t kSequenceSwitchPolicyBytes = 16U;
constexpr std::size_t kMaximumSessionRecordBytes = 1024U * 1024U;
constexpr wchar_t kSettingsKey[] = L"Software\\inkpod";
constexpr wchar_t kRestoreSettingValue[] = L"RestorePreviousDocumentsV1";
constexpr wchar_t kPreviousPathsValue[] = L"PreviousDocumentPathsV1";
constexpr wchar_t kSequenceSwitchPolicyValue[] = L"SequenceCellSwitchPolicyV1";
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

void AppendU64(std::vector<std::uint8_t>& bytes, std::uint64_t value) {
    for (std::uint32_t shift = 0U; shift < 64U; shift += 8U) {
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

bool ReadU64(
    const std::uint8_t* bytes,
    std::size_t length,
    std::size_t& cursor,
    std::uint64_t& value) noexcept {
    if (bytes == nullptr || cursor > length || length - cursor < 8U) {
        return false;
    }
    value = 0U;
    for (std::uint32_t shift = 0U; shift < 64U; shift += 8U) {
        value |= static_cast<std::uint64_t>(bytes[cursor++]) << shift;
    }
    return true;
}

bool EnsureDirectory(const std::wstring& path) noexcept {
    if (CreateDirectoryW(path.c_str(), nullptr) != FALSE) {
        return true;
    }
    if (GetLastError() != ERROR_ALREADY_EXISTS) {
        return false;
    }
    const DWORD attributes = GetFileAttributesW(path.c_str());
    return attributes != INVALID_FILE_ATTRIBUTES
        && (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0U;
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

bool OpenSettingsKey(REGSAM access, HKEY& key) noexcept {
    key = nullptr;
    return RegCreateKeyExW(
               HKEY_CURRENT_USER,
               kSettingsKey,
               0U,
               nullptr,
               REG_OPTION_NON_VOLATILE,
               access,
               nullptr,
               &key,
               nullptr)
        == ERROR_SUCCESS;
}

bool DeleteIfPresent(const std::wstring& path) noexcept {
    return DeleteFileW(path.c_str()) != FALSE || GetLastError() == ERROR_FILE_NOT_FOUND;
}

bool ValidIdentityKind(DocumentIdentityKind kind) noexcept {
    return kind == DocumentIdentityKind::None
        || kind == DocumentIdentityKind::WindowsFile
        || kind == DocumentIdentityKind::NormalizedPath
        || kind == DocumentIdentityKind::Untitled;
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
    PWSTR local_app_data{};
    if (FAILED(SHGetKnownFolderPath(
            FOLDERID_LocalAppData, KF_FLAG_CREATE, nullptr, &local_app_data))) {
        return false;
    }
    try {
        std::wstring root(local_app_data);
        CoTaskMemFree(local_app_data);
        local_app_data = nullptr;
        root += L"\\inkpod";
        if (!EnsureDirectory(root)) {
            return false;
        }
        root += L"\\Recovery";
        if (!EnsureDirectory(root)) {
            return false;
        }
        output = std::move(root);
        return true;
    } catch (const std::bad_alloc&) {
        if (local_app_data != nullptr) {
            CoTaskMemFree(local_app_data);
        }
        return false;
    }
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
    FILETIME now{};
    GetSystemTimeAsFileTime(&now);
    RecoveryMetadata metadata{};
    metadata.session = session;
    metadata.generation = generation;
    metadata.document_uuid_high = document_uuid_high;
    metadata.document_uuid_low = document_uuid_low;
    metadata.written_file_time = static_cast<std::uint64_t>(now.dwLowDateTime)
        | (static_cast<std::uint64_t>(now.dwHighDateTime) << 32U);
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

bool EncodeRecoveryMetadata(
    const RecoveryMetadata& metadata,
    std::vector<std::uint8_t>& output) noexcept {
    if (!metadata.session || !metadata.generation
        || !ValidIdentity(metadata.original_identity)
        || (metadata.document_uuid_high == 0U
            && metadata.document_uuid_low == 0U)) {
        return false;
    }
    try {
        std::vector<std::uint8_t> original_path;
        std::vector<std::uint8_t> normalized_path;
        std::vector<std::uint8_t> source_path;
        if (!WideToUtf8(metadata.original_path, original_path)
            || !WideToUtf8(
                metadata.original_identity.normalized_path, normalized_path)
            || !WideToUtf8(metadata.source_path, source_path)) {
            return false;
        }
        const std::size_t total = kMetadataHeaderBytes + 12U
            + original_path.size() + normalized_path.size() + source_path.size();
        if (total > kMaximumMetadataBytes || total > UINT32_MAX) {
            return false;
        }
        output.clear();
        output.reserve(total);
        AppendU32(output, kMetadataMagic);
        AppendU16(output, kMetadataVersion);
        AppendU16(output, kMetadataHeaderBytes);
        AppendU32(output, static_cast<std::uint32_t>(total));
        AppendU64(output, metadata.session.Value());
        AppendU64(output, metadata.generation.Value());
        AppendU64(output, metadata.document_uuid_high);
        AppendU64(output, metadata.document_uuid_low);
        AppendU32(
            output, static_cast<std::uint32_t>(metadata.original_identity.kind));
        AppendU32(output, 0U);
        AppendU64(output, metadata.original_identity.volume_serial);
        output.insert(
            output.end(),
            metadata.original_identity.file_id.begin(),
            metadata.original_identity.file_id.end());
        AppendU64(output, metadata.original_identity.uuid_high);
        AppendU64(output, metadata.original_identity.uuid_low);
        AppendU64(output, metadata.written_file_time);
        AppendString(output, original_path);
        AppendString(output, normalized_path);
        AppendString(output, source_path);
        return output.size() == total;
    } catch (const std::bad_alloc&) {
        output.clear();
        return false;
    }
}

bool DecodeRecoveryMetadata(
    const std::uint8_t* bytes,
    std::size_t length,
    RecoveryMetadata& output) noexcept {
    if (bytes == nullptr || length < kMetadataHeaderBytes
        || length > kMaximumMetadataBytes) {
        return false;
    }
    std::size_t cursor{};
    std::uint32_t magic{};
    std::uint16_t version{};
    std::uint16_t header_bytes{};
    std::uint32_t total_bytes{};
    std::uint64_t session{};
    std::uint64_t generation{};
    std::uint64_t uuid_high{};
    std::uint64_t uuid_low{};
    std::uint32_t identity_kind{};
    std::uint32_t reserved{};
    std::uint64_t volume_serial{};
    std::uint64_t identity_uuid_high{};
    std::uint64_t identity_uuid_low{};
    std::uint64_t written{};
    if (!ReadU32(bytes, length, cursor, magic)
        || !ReadU16(bytes, length, cursor, version)
        || !ReadU16(bytes, length, cursor, header_bytes)
        || !ReadU32(bytes, length, cursor, total_bytes)
        || !ReadU64(bytes, length, cursor, session)
        || !ReadU64(bytes, length, cursor, generation)
        || !ReadU64(bytes, length, cursor, uuid_high)
        || !ReadU64(bytes, length, cursor, uuid_low)
        || !ReadU32(bytes, length, cursor, identity_kind)
        || !ReadU32(bytes, length, cursor, reserved)
        || !ReadU64(bytes, length, cursor, volume_serial)
        || cursor > length || length - cursor < 16U) {
        return false;
    }
    RecoveryMetadata metadata{};
    std::copy_n(bytes + cursor, 16U, metadata.original_identity.file_id.begin());
    cursor += 16U;
    if (!ReadU64(bytes, length, cursor, identity_uuid_high)
        || !ReadU64(bytes, length, cursor, identity_uuid_low)
        || !ReadU64(bytes, length, cursor, written)
        || magic != kMetadataMagic || version != kMetadataVersion
        || header_bytes != kMetadataHeaderBytes || total_bytes != length
        || session == 0U || generation == 0U || reserved != 0U
        || (uuid_high == 0U && uuid_low == 0U)
        || !ValidIdentityKind(
            static_cast<DocumentIdentityKind>(identity_kind))) {
        return false;
    }
    metadata.session = DocumentSessionId(session);
    metadata.generation = Generation(generation);
    metadata.document_uuid_high = uuid_high;
    metadata.document_uuid_low = uuid_low;
    metadata.original_identity.kind =
        static_cast<DocumentIdentityKind>(identity_kind);
    metadata.original_identity.volume_serial = volume_serial;
    metadata.original_identity.uuid_high = identity_uuid_high;
    metadata.original_identity.uuid_low = identity_uuid_low;
    metadata.written_file_time = written;
    try {
        if (!ReadString(bytes, length, cursor, metadata.original_path)
            || !ReadString(
                bytes,
                length,
                cursor,
                metadata.original_identity.normalized_path)
            || !ReadString(bytes, length, cursor, metadata.source_path)
            || cursor != length || written == 0U
            || !ValidIdentity(metadata.original_identity)) {
            return false;
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    output = std::move(metadata);
    return true;
}

bool WriteRecoveryMetadata(
    const std::wstring& recovery_path,
    const RecoveryMetadata& metadata) noexcept {
    std::wstring metadata_path;
    std::vector<std::uint8_t> bytes;
    return RecoveryMetadataPath(recovery_path, metadata_path)
        && EncodeRecoveryMetadata(metadata, bytes)
        && WriteFileAtomic(metadata_path, bytes);
}

bool ReadRecoveryMetadata(
    const std::wstring& recovery_path,
    RecoveryMetadata& metadata) noexcept {
    std::wstring metadata_path;
    std::vector<std::uint8_t> bytes;
    return RecoveryMetadataPath(recovery_path, metadata_path)
        && ReadFileBounded(metadata_path, kMaximumMetadataBytes, bytes)
        && DecodeRecoveryMetadata(bytes.data(), bytes.size(), metadata);
}

bool EnumerateRecoveryCandidates(
    std::vector<RecoveryCandidate>& output) noexcept {
    std::wstring directory;
    return RecoveryRootDirectory(directory)
        && EnumerateRecoveryCandidatesInDirectory(directory, output);
}

bool EnumerateRecoveryCandidatesInDirectory(
    const std::wstring& directory,
    std::vector<RecoveryCandidate>& output) noexcept {
    std::wstring pattern;
    try {
        pattern = directory + L"\\*.inkpod";
    } catch (const std::bad_alloc&) {
        return false;
    }
    WIN32_FIND_DATAW entry{};
    HANDLE search = FindFirstFileW(pattern.c_str(), &entry);
    if (search == INVALID_HANDLE_VALUE) {
        if (GetLastError() == ERROR_FILE_NOT_FOUND) {
            output.clear();
            return true;
        }
        return false;
    }
    std::vector<RecoveryCandidate> candidates;
    bool valid = true;
    do {
        if ((entry.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0U) {
            continue;
        }
        if (candidates.size() >= kMaximumRecoveryCandidates) {
            valid = false;
            break;
        }
        try {
            RecoveryCandidate candidate{};
            candidate.recovery_path = directory + L"\\" + entry.cFileName;
            candidate.modified = entry.ftLastWriteTime;
            candidate.has_metadata = ReadRecoveryMetadata(
                candidate.recovery_path, candidate.metadata);
            (void)RecoveryMetadataPath(
                candidate.recovery_path, candidate.metadata_path);
            candidates.push_back(std::move(candidate));
        } catch (const std::bad_alloc&) {
            valid = false;
            break;
        }
    } while (FindNextFileW(search, &entry) != FALSE);
    if (valid && GetLastError() != ERROR_NO_MORE_FILES) {
        valid = false;
    }
    FindClose(search);
    if (!valid) {
        return false;
    }
    std::sort(
        candidates.begin(),
        candidates.end(),
        [](const RecoveryCandidate& left, const RecoveryCandidate& right) {
            const LONG compared = CompareFileTime(&left.modified, &right.modified);
            if (compared != 0) {
                return compared > 0;
            }
            return left.recovery_path < right.recovery_path;
        });
    output = std::move(candidates);
    return true;
}

bool DiscardRecoveryArtifact(const std::wstring& recovery_path) noexcept {
    if (recovery_path.empty()) {
        return false;
    }
    std::wstring metadata_path;
    return RecoveryMetadataPath(recovery_path, metadata_path)
        && DeleteIfPresent(recovery_path) && DeleteIfPresent(metadata_path);
}

bool LoadRestorePreviousDocumentsSetting(bool& enabled) noexcept {
    enabled = false;
    HKEY key{};
    if (!OpenSettingsKey(KEY_QUERY_VALUE, key)) {
        return false;
    }
    DWORD type{};
    DWORD value{};
    DWORD bytes = sizeof(value);
    const LSTATUS status = RegQueryValueExW(
        key,
        kRestoreSettingValue,
        nullptr,
        &type,
        reinterpret_cast<LPBYTE>(&value),
        &bytes);
    RegCloseKey(key);
    if (status == ERROR_FILE_NOT_FOUND) {
        return true;
    }
    if (status != ERROR_SUCCESS || type != REG_DWORD || bytes != sizeof(value)
        || value > 1U) {
        return false;
    }
    enabled = value != 0U;
    return true;
}

bool SaveRestorePreviousDocumentsSetting(bool enabled) noexcept {
    HKEY key{};
    if (!OpenSettingsKey(KEY_SET_VALUE, key)) {
        return false;
    }
    const DWORD value = enabled ? 1U : 0U;
    const bool saved = RegSetValueExW(
        key,
        kRestoreSettingValue,
        0U,
        REG_DWORD,
        reinterpret_cast<const BYTE*>(&value),
        sizeof(value)) == ERROR_SUCCESS;
    RegCloseKey(key);
    return saved;
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

bool EncodeSequenceCellSwitchPolicy(
    SequenceCellSwitchPolicy policy,
    std::vector<std::uint8_t>& output) noexcept {
    if (policy != SequenceCellSwitchPolicy::Prompt
        && policy != SequenceCellSwitchPolicy::AutosaveBeforeSwitch) {
        return false;
    }
    try {
        output.clear();
        output.reserve(kSequenceSwitchPolicyBytes);
        AppendU32(output, kSequenceSwitchPolicyMagic);
        AppendU16(output, kSequenceSwitchPolicyVersion);
        AppendU16(output, 0U);
        AppendU32(output, static_cast<std::uint32_t>(kSequenceSwitchPolicyBytes));
        AppendU32(output, static_cast<std::uint32_t>(policy));
        return output.size() == kSequenceSwitchPolicyBytes;
    } catch (const std::bad_alloc&) {
        output.clear();
        return false;
    }
}

bool DecodeSequenceCellSwitchPolicy(
    const std::uint8_t* bytes,
    std::size_t length,
    SequenceCellSwitchPolicy& policy) noexcept {
    if (bytes == nullptr || length != kSequenceSwitchPolicyBytes) {
        return false;
    }
    std::size_t cursor{};
    std::uint32_t magic{};
    std::uint16_t version{};
    std::uint16_t reserved{};
    std::uint32_t total{};
    std::uint32_t raw_policy{};
    if (!ReadU32(bytes, length, cursor, magic)
        || !ReadU16(bytes, length, cursor, version)
        || !ReadU16(bytes, length, cursor, reserved)
        || !ReadU32(bytes, length, cursor, total)
        || !ReadU32(bytes, length, cursor, raw_policy)
        || cursor != length || magic != kSequenceSwitchPolicyMagic
        || version != kSequenceSwitchPolicyVersion || reserved != 0U
        || total != length
        || (raw_policy
                != static_cast<std::uint32_t>(SequenceCellSwitchPolicy::Prompt)
            && raw_policy != static_cast<std::uint32_t>(
                SequenceCellSwitchPolicy::AutosaveBeforeSwitch))) {
        return false;
    }
    policy = static_cast<SequenceCellSwitchPolicy>(raw_policy);
    return true;
}

bool LoadSequenceCellSwitchPolicy(
    SequenceCellSwitchPolicy& policy) noexcept {
    policy = SequenceCellSwitchPolicy::Prompt;
    HKEY key{};
    if (!OpenSettingsKey(KEY_QUERY_VALUE, key)) {
        return false;
    }
    DWORD type{};
    DWORD byte_count{};
    LSTATUS status = RegQueryValueExW(
        key, kSequenceSwitchPolicyValue, nullptr, &type, nullptr, &byte_count);
    if (status == ERROR_FILE_NOT_FOUND) {
        RegCloseKey(key);
        return true;
    }
    if (status != ERROR_SUCCESS || type != REG_BINARY
        || byte_count != kSequenceSwitchPolicyBytes) {
        RegCloseKey(key);
        return false;
    }
    std::array<std::uint8_t, kSequenceSwitchPolicyBytes> bytes{};
    status = RegQueryValueExW(
        key,
        kSequenceSwitchPolicyValue,
        nullptr,
        &type,
        bytes.data(),
        &byte_count);
    RegCloseKey(key);
    return status == ERROR_SUCCESS
        && DecodeSequenceCellSwitchPolicy(bytes.data(), bytes.size(), policy);
}

bool SaveSequenceCellSwitchPolicy(
    SequenceCellSwitchPolicy policy) noexcept {
    std::vector<std::uint8_t> bytes;
    if (!EncodeSequenceCellSwitchPolicy(policy, bytes)) {
        return false;
    }
    HKEY key{};
    if (!OpenSettingsKey(KEY_SET_VALUE, key)) {
        return false;
    }
    const bool saved = RegSetValueExW(
        key,
        kSequenceSwitchPolicyValue,
        0U,
        REG_BINARY,
        bytes.data(),
        static_cast<DWORD>(bytes.size())) == ERROR_SUCCESS;
    RegCloseKey(key);
    return saved;
}

bool LoadPreviousDocumentPaths(
    std::vector<std::wstring>& paths) noexcept {
    HKEY key{};
    if (!OpenSettingsKey(KEY_QUERY_VALUE, key)) {
        return false;
    }
    DWORD type{};
    DWORD byte_count{};
    LSTATUS status = RegQueryValueExW(
        key, kPreviousPathsValue, nullptr, &type, nullptr, &byte_count);
    if (status == ERROR_FILE_NOT_FOUND) {
        RegCloseKey(key);
        paths.clear();
        return true;
    }
    if (status != ERROR_SUCCESS || type != REG_BINARY
        || byte_count > kMaximumSessionRecordBytes) {
        RegCloseKey(key);
        return false;
    }
    std::vector<std::uint8_t> bytes;
    try {
        bytes.resize(byte_count);
    } catch (const std::bad_alloc&) {
        RegCloseKey(key);
        return false;
    }
    status = RegQueryValueExW(
        key,
        kPreviousPathsValue,
        nullptr,
        &type,
        bytes.data(),
        &byte_count);
    RegCloseKey(key);
    if (status != ERROR_SUCCESS) {
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
    HKEY key{};
    if (!OpenSettingsKey(KEY_SET_VALUE, key)) {
        return false;
    }
    const bool saved = RegSetValueExW(
        key,
        kPreviousPathsValue,
        0U,
        REG_BINARY,
        bytes.data(),
        static_cast<DWORD>(bytes.size())) == ERROR_SUCCESS;
    RegCloseKey(key);
    return saved;
}

bool ClearPreviousDocumentPaths() noexcept {
    HKEY key{};
    if (!OpenSettingsKey(KEY_SET_VALUE, key)) {
        return false;
    }
    const LSTATUS status = RegDeleteValueW(key, kPreviousPathsValue);
    RegCloseKey(key);
    return status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND;
}

}  // namespace inkpod::app
