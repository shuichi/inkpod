#include <windows.h>
#include <winioctl.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <span>
#include <string>
#include <string_view>
#include <vector>

#include "app/blake3_digest.h"
#include "app/inkscript_file_authority.h"

namespace {

using inkpod::app::Blake3Digest;
using inkpod::app::InkScriptFileAuthorityAdapter;

class TestDirectory final {
public:
    TestDirectory() {
        std::array<wchar_t, MAX_PATH> directory{};
        std::array<wchar_t, MAX_PATH> temporary{};
        const DWORD length = GetTempPathW(
            static_cast<DWORD>(directory.size()), directory.data());
        if (length == 0U || length >= directory.size()
            || GetTempFileNameW(directory.data(), L"ika", 0U, temporary.data()) == 0U
            || DeleteFileW(temporary.data()) == FALSE
            || CreateDirectoryW(temporary.data(), nullptr) == FALSE) {
            return;
        }
        path_ = temporary.data();
    }

    ~TestDirectory() {
        if (!path_.empty()) {
            std::error_code ignored;
            std::filesystem::remove_all(path_, ignored);
        }
    }

    [[nodiscard]] const std::wstring& path() const noexcept { return path_; }
    [[nodiscard]] explicit operator bool() const noexcept { return !path_.empty(); }

private:
    std::wstring path_;
};

bool WriteBytes(const std::wstring& path, std::span<const std::uint8_t> bytes) {
    HANDLE file = CreateFileW(
        path.c_str(),
        GENERIC_WRITE,
        FILE_SHARE_READ,
        nullptr,
        CREATE_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    DWORD written{};
    const bool ok = bytes.size() <= MAXDWORD
        && (bytes.empty()
            || (WriteFile(
                    file,
                    bytes.data(),
                    static_cast<DWORD>(bytes.size()),
                    &written,
                    nullptr)
                    != FALSE
                && written == bytes.size()))
        && FlushFileBuffers(file) != FALSE;
    CloseHandle(file);
    return ok;
}

bool ReadBytes(const std::wstring& path, std::vector<std::uint8_t>& output) {
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
        || size.QuadPart > static_cast<LONGLONG>(MAXDWORD)) {
        CloseHandle(file);
        return false;
    }
    output.resize(static_cast<std::size_t>(size.QuadPart));
    DWORD read{};
    const bool ok = output.empty()
        || (ReadFile(
                file,
                output.data(),
                static_cast<DWORD>(output.size()),
                &read,
                nullptr)
                != FALSE
            && read == output.size());
    CloseHandle(file);
    return ok;
}

bool WideToUtf8(const std::wstring& input, std::vector<std::uint8_t>& output) {
    if (input.empty() || input.size() > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int count = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        input.data(),
        static_cast<int>(input.size()),
        nullptr,
        0,
        nullptr,
        nullptr);
    if (count <= 0) {
        return false;
    }
    output.resize(static_cast<std::size_t>(count));
    return WideCharToMultiByte(
               CP_UTF8,
               WC_ERR_INVALID_CHARS,
               input.data(),
               static_cast<int>(input.size()),
               reinterpret_cast<char*>(output.data()),
               count,
               nullptr,
               nullptr)
        == count;
}

bool CreateNative(const std::wstring& path, std::uint64_t uuid) {
    InkpodCoreConfig config{
        sizeof(InkpodCoreConfig), inkpod_abi_version(), INKPOD_FEATURE_NONE};
    InkpodCore* core{};
    if (inkpod_core_create(&config, &core) != INKPOD_STATUS_OK) {
        return false;
    }
    const InkpodCellCreateOptions options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d32374100000000) | uuid,
        UINT64_C(0x4e41544900000000) | uuid,
        8U,
        8U,
        72'000U,
        72'000U};
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    std::vector<std::uint8_t> utf8;
    const bool created = inkpod_core_new_cell(core, &options, &info)
            == INKPOD_STATUS_OK
        && WideToUtf8(path, utf8)
        && inkpod_core_save(core, utf8.data(), utf8.size(), &info)
            == INKPOD_STATUS_OK;
    return inkpod_core_destroy(&core) == INKPOD_STATUS_OK && created;
}

struct FingerprintCopy final {
    std::vector<std::uint8_t> key;
    std::vector<std::uint8_t> label;
    InkpodInkScriptPathIdentity path{};
    InkpodInkScriptNativeFingerprint fingerprint{};

    bool Capture(const InkpodInkScriptNativeFingerprint* source) {
        if (source == nullptr || source->path == nullptr
            || source->path->canonical_key.byte_count > SIZE_MAX
            || source->display_label.byte_count > SIZE_MAX) {
            return false;
        }
        key.assign(
            source->path->canonical_key.bytes,
            source->path->canonical_key.bytes
                + static_cast<std::size_t>(
                    source->path->canonical_key.byte_count));
        label.assign(
            source->display_label.bytes,
            source->display_label.bytes
                + static_cast<std::size_t>(source->display_label.byte_count));
        path = *source->path;
        path.canonical_key = {key.data(), key.size()};
        fingerprint = *source;
        fingerprint.path = &path;
        fingerprint.display_label = {label.data(), label.size()};
        return true;
    }
};

InkpodStatus Call(
    InkpodInkScriptHostAdapter& host,
    InkpodInkScriptHostRequest& request,
    InkpodInkScriptHostResponse& response) {
    request.struct_size = sizeof(request);
    request.version = INKPOD_INKSCRIPT_RECORD_VERSION;
    response = {};
    response.struct_size = sizeof(response);
    response.version = INKPOD_INKSCRIPT_RECORD_VERSION;
    return host.call(host.context, &request, &response);
}

bool SameObject(
    const InkpodInkScriptPathIdentity& left,
    const InkpodInkScriptPathIdentity& right) noexcept {
    return std::memcmp(
               left.volume_id, right.volume_id, sizeof(left.volume_id))
            == 0
        && std::memcmp(
               left.object_id, right.object_id, sizeof(left.object_id))
            == 0
        && left.object_generation == right.object_generation;
}

bool IsZero(const std::uint8_t* bytes, std::size_t count) noexcept {
    return std::all_of(bytes, bytes + count, [](std::uint8_t value) {
        return value == 0U;
    });
}

bool CreateJunction(const std::wstring& junction, const std::wstring& target) {
    struct MountPointReparseData final {
        ULONG tag;
        USHORT data_length;
        USHORT reserved;
        USHORT substitute_offset;
        USHORT substitute_length;
        USHORT print_offset;
        USHORT print_length;
        wchar_t path[1U];
    };
    if (CreateDirectoryW(junction.c_str(), nullptr) == FALSE) {
        return false;
    }
    HANDLE directory = CreateFileW(
        junction.c_str(),
        GENERIC_WRITE,
        0U,
        nullptr,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        nullptr);
    if (directory == INVALID_HANDLE_VALUE) {
        RemoveDirectoryW(junction.c_str());
        return false;
    }
    const std::wstring substitute = L"\\??\\" + target;
    const std::wstring print = target;
    const std::size_t substitute_bytes = substitute.size() * sizeof(wchar_t);
    const std::size_t print_bytes = print.size() * sizeof(wchar_t);
    const std::size_t payload_bytes = substitute_bytes + sizeof(wchar_t)
        + print_bytes + sizeof(wchar_t);
    std::vector<std::uint8_t> storage(
        offsetof(MountPointReparseData, path) + payload_bytes);
    auto* data = reinterpret_cast<MountPointReparseData*>(storage.data());
    data->tag = IO_REPARSE_TAG_MOUNT_POINT;
    data->data_length = static_cast<USHORT>(payload_bytes + 8U);
    data->substitute_offset = 0U;
    data->substitute_length = static_cast<USHORT>(substitute_bytes);
    data->print_offset =
        static_cast<USHORT>(substitute_bytes + sizeof(wchar_t));
    data->print_length = static_cast<USHORT>(print_bytes);
    std::memcpy(
        data->path, substitute.data(), substitute_bytes);
    std::memcpy(
        reinterpret_cast<std::uint8_t*>(data->path)
            + substitute_bytes + sizeof(wchar_t),
        print.data(),
        print_bytes);
    DWORD returned{};
    const bool ok = DeviceIoControl(
        directory,
        FSCTL_SET_REPARSE_POINT,
        data,
        static_cast<DWORD>(8U + data->data_length),
        nullptr,
        0U,
        &returned,
        nullptr)
        != FALSE;
    CloseHandle(directory);
    if (!ok) {
        RemoveDirectoryW(junction.c_str());
    }
    return ok;
}

bool RemoveJunction(const std::wstring& path) {
    HANDLE directory = CreateFileW(
        path.c_str(),
        GENERIC_WRITE,
        0U,
        nullptr,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        nullptr);
    if (directory == INVALID_HANDLE_VALUE) {
        return false;
    }
    struct DeleteReparseData final {
        ULONG tag;
        USHORT data_length;
        USHORT reserved;
    } data{};
    data.tag = IO_REPARSE_TAG_MOUNT_POINT;
    DWORD returned{};
    const bool cleared = DeviceIoControl(
        directory,
        FSCTL_DELETE_REPARSE_POINT,
        &data,
        sizeof(data),
        nullptr,
        0U,
        &returned,
        nullptr)
        != FALSE;
    CloseHandle(directory);
    return cleared && RemoveDirectoryW(path.c_str()) != FALSE;
}

std::wstring FindTemporary(const std::wstring& directory) {
    WIN32_FIND_DATAW data{};
    HANDLE search = FindFirstFileW(
        (directory + L"\\.~inkpod-*.tmp").c_str(), &data);
    if (search == INVALID_HANDLE_VALUE) {
        return {};
    }
    const std::wstring result = directory + L"\\" + data.cFileName;
    FindClose(search);
    return result;
}

int Run() {
    const auto empty = Blake3Digest({});
    constexpr std::array<std::uint8_t, 32U> empty_expected{
        0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6,
        0xa0, 0x40, 0x4d, 0xea, 0x36, 0xdc, 0xc9, 0x49,
        0x9b, 0xcb, 0x25, 0xc9, 0xad, 0xc1, 0x12, 0xb7,
        0xcc, 0x9a, 0x93, 0xca, 0xe4, 0x1f, 0x32, 0x62};
    constexpr std::array<std::uint8_t, 3U> abc{'a', 'b', 'c'};
    constexpr std::array<std::uint8_t, 32U> abc_expected{
        0x64, 0x37, 0xb3, 0xac, 0x38, 0x46, 0x51, 0x33,
        0xff, 0xb6, 0x3b, 0x75, 0x27, 0x3a, 0x8d, 0xb5,
        0x48, 0xc5, 0x58, 0x46, 0x5d, 0x79, 0xdb, 0x03,
        0xfd, 0x35, 0x9c, 0x6c, 0xd5, 0xbd, 0x9d, 0x85};
    std::array<std::uint8_t, 2049U> chunked{};
    for (std::size_t index = 0U; index < chunked.size(); ++index) {
        chunked[index] = static_cast<std::uint8_t>(index % 251U);
    }
    constexpr std::array<std::uint8_t, 32U> chunked_expected{
        0x5f, 0x4d, 0x72, 0xf4, 0x0d, 0x7a, 0x5f, 0x82,
        0xb1, 0x5c, 0xa2, 0xb2, 0xe4, 0x4b, 0x1d, 0xe3,
        0xc2, 0xef, 0x86, 0xc4, 0x26, 0xc9, 0x5c, 0x1a,
        0xf0, 0xb6, 0x87, 0x95, 0x22, 0x56, 0x30, 0x30};
    if (empty != empty_expected || Blake3Digest(abc) != abc_expected
        || Blake3Digest(chunked) != chunked_expected) {
        return 1;
    }

    TestDirectory test_directory;
    if (!test_directory) {
        return 2;
    }
    const std::wstring root = test_directory.path();
    const std::wstring original = root + L"\\original.inkpod";
    const std::wstring alias = root + L"\\alias.inkpod";
    constexpr std::array<std::uint8_t, 4U> original_bytes{1U, 2U, 3U, 4U};
    if (!WriteBytes(original, original_bytes)
        || CreateHardLinkW(alias.c_str(), original.c_str(), nullptr) == FALSE) {
        return 3;
    }

    InkScriptFileAuthorityAdapter adapter;
    InkpodInkScriptAuthorityGrant original_grant{};
    InkpodInkScriptAuthorityGrant alias_grant{};
    InkpodInkScriptAuthorityGrant root_grant{};
    if (adapter.AuthorizePath(
            1U, INKPOD_INKSCRIPT_PATH_READ, original, original_grant)
            != INKPOD_STATUS_OK
        || adapter.AuthorizePath(
               2U, INKPOD_INKSCRIPT_PATH_READ, alias, alias_grant)
            != INKPOD_STATUS_OK
        || !SameObject(*original_grant.resolved, *alias_grant.resolved)
        || std::memcmp(
               original_grant.resolved->alias_key,
               alias_grant.resolved->alias_key,
               sizeof(original_grant.resolved->alias_key)) == 0
        || adapter.AuthorizePath(
               3U, INKPOD_INKSCRIPT_PATH_CREATE, root, root_grant)
            != INKPOD_STATUS_OK) {
        return 4;
    }
    InkpodInkScriptHostAdapter host = adapter.HostAdapterRecord();
    InkpodInkScriptHostRequest request{};
    InkpodInkScriptHostResponse response{};

    const std::wstring native_directory = root + L"\\native";
    const std::wstring native_path = native_directory + L"\\native_0001.inkpod";
    const std::wstring native_replacement =
        native_directory + L"\\replacement.inkpod";
    const std::wstring resource_counter_path = native_directory + L"\\資源.txt";
    constexpr std::array<std::uint8_t, 1U> resource_counter_bytes{0U};
    if (CreateDirectoryW(native_directory.c_str(), nullptr) == FALSE
        || !CreateNative(native_path, 1U)
        || !CreateNative(native_replacement, 2U)
        || !WriteBytes(resource_counter_path, resource_counter_bytes)) {
        return 35;
    }
    InkpodInkScriptAuthorityGrant native_grant{};
    InkpodInkScriptAuthorityGrant native_folder_grant{};
    InkpodInkScriptAuthorityGrant replace_grant{};
    InkpodInkScriptAuthorityGrant rejected_grant{};
    if (adapter.AuthorizePath(
            4U, INKPOD_INKSCRIPT_PATH_READ, native_path, native_grant)
            != INKPOD_STATUS_OK
        || adapter.AuthorizePath(
               5U,
               INKPOD_INKSCRIPT_PATH_ENUMERATE,
               native_directory,
               native_folder_grant)
            != INKPOD_STATUS_OK
        || adapter.AuthorizePath(
               6U, INKPOD_INKSCRIPT_PATH_REPLACE, native_path, replace_grant)
            != INKPOD_STATUS_OK
        || !SameObject(*native_grant.resolved, *replace_grant.resolved)
        || adapter.AuthorizePath(
               7U,
               INKPOD_INKSCRIPT_PATH_REPLACE,
               native_directory,
               rejected_grant)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || adapter.AuthorizePath(
               8U,
               INKPOD_INKSCRIPT_PATH_CREATE,
               native_path,
               rejected_grant)
            != INKPOD_STATUS_INVALID_ARGUMENT) {
        return 36;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_RESOLVE_FILE;
    request.intent_id = 4U;
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 37;
    }
    FingerprintCopy original_fingerprint;
    if (!original_fingerprint.Capture(response.fingerprint)) {
        return 38;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_READ_NATIVE;
    request.fingerprint = &original_fingerprint.fingerprint;
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.byte_count == 0U || response.bytes == nullptr
        || response.fingerprint == nullptr
        || Blake3Digest(std::span<const std::uint8_t>{
               response.bytes,
               static_cast<std::size_t>(response.byte_count)})
            != [&response] {
                   std::array<std::uint8_t, 32U> value{};
                   std::copy_n(
                       response.fingerprint->content_digest,
                       value.size(),
                       value.begin());
                   return value;
               }()) {
        return 39;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_ENUMERATE_FOLDER;
    request.intent_id = 5U;
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.record_count != 2U || response.observed_entries != 3U
        || response.normalized_name_bytes != 46U
        || response.work_units != 3U) {
        return 40;
    }
    if (MoveFileExW(
            native_replacement.c_str(),
            native_path.c_str(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
        == FALSE) {
        return 41;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_FINGERPRINT_NATIVE;
    request.fingerprint = &original_fingerprint.fingerprint;
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.fingerprint == nullptr
        || SameObject(
            original_fingerprint.path, *response.fingerprint->path)) {
        return 42;
    }
    request.operation = INKPOD_INKSCRIPT_HOST_READ_NATIVE;
    if (Call(host, request, response) != INKPOD_STATUS_INVALID_STATE) {
        return 43;
    }

    const std::wstring asset_path = root + L"\\asset.bin";
    constexpr std::array<std::uint8_t, 6U> asset_bytes{3U, 1U, 4U, 1U, 5U, 9U};
    if (!WriteBytes(asset_path, asset_bytes)
        || adapter.AuthorizeAsset("palette", asset_path) != INKPOD_STATUS_OK) {
        return 44;
    }
    const std::string_view asset_symbol = "palette";
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_ASSET_IDENTITY;
    request.asset_symbol = {
        reinterpret_cast<const std::uint8_t*>(asset_symbol.data()),
        asset_symbol.size()};
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.byte_count != asset_bytes.size()) {
        return 45;
    }
    request.operation = INKPOD_INKSCRIPT_HOST_ASSET_READ;
    request.byte_capacity = 64U;
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.byte_count != asset_bytes.size()
        || !std::equal(
            response.bytes,
            response.bytes + response.byte_count,
            asset_bytes.begin(),
            asset_bytes.end())) {
        return 46;
    }

    request.operation = INKPOD_INKSCRIPT_HOST_AUTHORITY_GENERATION;
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.generation < 4U) {
        return 5;
    }
    const std::uint64_t generation = response.generation;
    if (adapter.RevokePathAuthority(2U) != INKPOD_STATUS_OK
        || Call(host, request, response) != INKPOD_STATUS_OK
        || response.generation != generation + 1U) {
        return 6;
    }

    const std::wstring trap = root + L"\\trap";
    if (CreateDirectoryW(trap.c_str(), nullptr) == FALSE) {
        return 7;
    }
    const std::array<std::string_view, 2U> parts{"nested", "out.inkpod"};
    std::array<InkpodInkScriptUtf8Span, parts.size()> spans{};
    for (std::size_t index = 0; index < parts.size(); ++index) {
        spans[index] = {
            reinterpret_cast<const std::uint8_t*>(parts[index].data()),
            parts[index].size()};
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_RESOLVE_DESTINATION;
    request.intent_id = 3U;
    request.identity = root_grant.resolved;
    request.relative_components = spans.data();
    request.relative_component_count = spans.size();
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.identity == nullptr
        || (response.identity->flags & INKPOD_INKSCRIPT_PATH_EXPECTED_ABSENT) == 0U) {
        return 8;
    }
    const InkpodInkScriptPathIdentity planned = *response.identity;
    const std::string planned_key{
        reinterpret_cast<const char*>(planned.canonical_key.bytes),
        static_cast<std::size_t>(planned.canonical_key.byte_count)};
    std::vector<std::uint8_t> planned_key_storage(
        planned_key.begin(), planned_key.end());
    InkpodInkScriptPathIdentity planned_copy = planned;
    planned_copy.canonical_key = {
        planned_key_storage.data(), planned_key_storage.size()};

    const std::wstring nested = root + L"\\nested";
    if (!CreateJunction(nested, trap)) {
        return 9;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_PREPARE_DESTINATION;
    request.identity = &planned_copy;
    const InkpodStatus reparse_status = Call(host, request, response);
    if (reparse_status != INKPOD_STATUS_INVALID_STATE) {
        return 100 + static_cast<int>(reparse_status);
    }
    if (GetFileAttributesW((trap + L"\\out.inkpod").c_str())
        != INVALID_FILE_ATTRIBUTES) {
        return 102;
    }
    if (!RemoveJunction(nested)) {
        return 103;
    }
    const InkpodStatus prepare_status = Call(host, request, response);
    if (prepare_status != INKPOD_STATUS_OK) {
        return 110 + static_cast<int>(prepare_status);
    }
    if (response.identity == nullptr) {
        return 120;
    }
    if (response.record_count != 1U) {
        return 121;
    }
    if (GetFileAttributesW(nested.c_str()) == INVALID_FILE_ATTRIBUTES) {
        return 122;
    }
    const InkpodInkScriptPathIdentity prepared = *response.identity;
    const std::string prepared_key{
        reinterpret_cast<const char*>(prepared.canonical_key.bytes),
        static_cast<std::size_t>(prepared.canonical_key.byte_count)};
    std::vector<std::uint8_t> prepared_key_storage(
        prepared_key.begin(), prepared_key.end());
    InkpodInkScriptPathIdentity prepared_copy = prepared;
    prepared_copy.canonical_key = {
        prepared_key_storage.data(), prepared_key_storage.size()};

    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_CREATE_TEMPORARY;
    request.identity = &prepared_copy;
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || IsZero(response.temporary.object_id, sizeof(response.temporary.object_id))) {
        return 12;
    }
    const InkpodInkScriptTemporaryIdentity attacked_temporary = response.temporary;
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_WRITE_CLOSE_TEMPORARY;
    request.temporary = attacked_temporary;
    request.bytes = original_bytes.data();
    request.byte_count = original_bytes.size();
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 13;
    }
    const std::wstring attacked_path = FindTemporary(nested);
    constexpr std::array<std::uint8_t, 3U> attacker_bytes{9U, 8U, 7U};
    if (attacked_path.empty() || DeleteFileW(attacked_path.c_str()) == FALSE
        || !WriteBytes(attacked_path, attacker_bytes)) {
        return 14;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_REVALIDATE_TEMPORARY;
    request.temporary = attacked_temporary;
    if (Call(host, request, response) != INKPOD_STATUS_INVALID_STATE) {
        return 15;
    }
    request.operation = INKPOD_INKSCRIPT_HOST_CLEANUP_TEMPORARY;
    if (Call(host, request, response) != INKPOD_STATUS_INVALID_STATE
        || GetFileAttributesW(attacked_path.c_str()) == INVALID_FILE_ATTRIBUTES) {
        return 16;
    }
    request.operation = INKPOD_INKSCRIPT_HOST_ATOMIC_INSTALL;
    request.identity = &prepared_copy;
    if (Call(host, request, response) != INKPOD_STATUS_INVALID_STATE
        || GetFileAttributesW((nested + L"\\out.inkpod").c_str())
            != INVALID_FILE_ATTRIBUTES
        || DeleteFileW(attacked_path.c_str()) == FALSE) {
        return 17;
    }

    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_CREATE_TEMPORARY;
    request.identity = &prepared_copy;
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 47;
    }
    const InkpodInkScriptTemporaryIdentity reparse_temporary = response.temporary;
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_WRITE_CLOSE_TEMPORARY;
    request.temporary = reparse_temporary;
    request.bytes = original_bytes.data();
    request.byte_count = original_bytes.size();
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 48;
    }
    const std::wstring reparse_temporary_path = FindTemporary(nested);
    if (reparse_temporary_path.empty()
        || DeleteFileW(reparse_temporary_path.c_str()) == FALSE
        || !CreateJunction(reparse_temporary_path, trap)) {
        return 49;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_REVALIDATE_TEMPORARY;
    request.temporary = reparse_temporary;
    if (Call(host, request, response) != INKPOD_STATUS_INVALID_STATE) {
        return 50;
    }
    request.operation = INKPOD_INKSCRIPT_HOST_CLEANUP_TEMPORARY;
    if (Call(host, request, response) != INKPOD_STATUS_INVALID_STATE
        || !RemoveJunction(reparse_temporary_path)) {
        return 51;
    }

    // Two adapters start with the same private name sequence. The second must
    // survive a real FILE_CREATE collision and create another object.
    InkScriptFileAuthorityAdapter collision_adapter;
    InkpodInkScriptAuthorityGrant collision_root{};
    if (collision_adapter.AuthorizePath(
            30U, INKPOD_INKSCRIPT_PATH_CREATE, root, collision_root)
            != INKPOD_STATUS_OK) {
        return 18;
    }
    InkpodInkScriptHostAdapter collision_host =
        collision_adapter.HostAdapterRecord();
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_RESOLVE_DESTINATION;
    request.intent_id = 30U;
    request.identity = collision_root.resolved;
    const std::string_view collision_name = "collision.inkpod";
    InkpodInkScriptUtf8Span collision_span{
        reinterpret_cast<const std::uint8_t*>(collision_name.data()),
        collision_name.size()};
    request.relative_components = &collision_span;
    request.relative_component_count = 1U;
    if (Call(collision_host, request, response) != INKPOD_STATUS_OK) {
        return 19;
    }
    const InkpodInkScriptPathIdentity collision_destination = *response.identity;
    const std::string collision_key{
        reinterpret_cast<const char*>(collision_destination.canonical_key.bytes),
        static_cast<std::size_t>(
            collision_destination.canonical_key.byte_count)};
    std::vector<std::uint8_t> collision_key_storage(
        collision_key.begin(), collision_key.end());
    InkpodInkScriptPathIdentity collision_destination_copy = collision_destination;
    collision_destination_copy.canonical_key = {
        collision_key_storage.data(), collision_key_storage.size()};
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_CREATE_TEMPORARY;
    request.identity = &collision_destination_copy;
    if (Call(collision_host, request, response) != INKPOD_STATUS_OK) {
        return 20;
    }
    const InkpodInkScriptTemporaryIdentity collision_temp = response.temporary;

    InkScriptFileAuthorityAdapter second_collision_adapter;
    InkpodInkScriptAuthorityGrant second_collision_root{};
    if (second_collision_adapter.AuthorizePath(
            31U, INKPOD_INKSCRIPT_PATH_CREATE, root, second_collision_root)
            != INKPOD_STATUS_OK) {
        return 52;
    }
    InkpodInkScriptHostAdapter second_collision_host =
        second_collision_adapter.HostAdapterRecord();
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_RESOLVE_DESTINATION;
    request.intent_id = 31U;
    request.identity = second_collision_root.resolved;
    request.relative_components = &collision_span;
    request.relative_component_count = 1U;
    if (Call(second_collision_host, request, response) != INKPOD_STATUS_OK) {
        return 53;
    }
    const InkpodInkScriptPathIdentity second_collision_destination =
        *response.identity;
    const std::string second_collision_key{
        reinterpret_cast<const char*>(
            second_collision_destination.canonical_key.bytes),
        static_cast<std::size_t>(
            second_collision_destination.canonical_key.byte_count)};
    std::vector<std::uint8_t> second_collision_key_storage(
        second_collision_key.begin(), second_collision_key.end());
    InkpodInkScriptPathIdentity second_collision_destination_copy =
        second_collision_destination;
    second_collision_destination_copy.canonical_key = {
        second_collision_key_storage.data(), second_collision_key_storage.size()};
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_CREATE_TEMPORARY;
    request.identity = &second_collision_destination_copy;
    if (Call(second_collision_host, request, response) != INKPOD_STATUS_OK
        || std::memcmp(
               collision_temp.object_id,
               response.temporary.object_id,
               sizeof(collision_temp.object_id))
            == 0) {
        return 21;
    }
    const InkpodInkScriptTemporaryIdentity second_collision_temp =
        response.temporary;

    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_CREATE_TEMPORARY;
    request.identity = &prepared_copy;
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 54;
    }
    const InkpodInkScriptTemporaryIdentity install_temp = response.temporary;
    constexpr std::array<std::uint8_t, 5U> installed_bytes{5U, 4U, 3U, 2U, 1U};
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_WRITE_CLOSE_TEMPORARY;
    request.temporary = install_temp;
    request.bytes = installed_bytes.data();
    request.byte_count = installed_bytes.size();
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 22;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_REVALIDATE_TEMPORARY;
    request.temporary = install_temp;
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 23;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_ATOMIC_INSTALL;
    request.temporary = install_temp;
    request.identity = &prepared_copy;
    const InkpodStatus install_status = Call(host, request, response);
    if (install_status != INKPOD_STATUS_OK) {
        return 200 + static_cast<int>(install_status);
    }
    if (response.result_kind != 1U) {
        return 220 + static_cast<int>(response.result_kind);
    }
    std::vector<std::uint8_t> observed_bytes;
    if (!ReadBytes(nested + L"\\out.inkpod", observed_bytes)
        || !std::equal(
            observed_bytes.begin(), observed_bytes.end(), installed_bytes.begin(), installed_bytes.end())) {
        return 25;
    }

    const std::array<std::string_view, 3U> deep_parts{
        "deep-one", "deep-two", "future.inkpod"};
    std::array<InkpodInkScriptUtf8Span, deep_parts.size()> deep_spans{};
    for (std::size_t index = 0; index < deep_parts.size(); ++index) {
        deep_spans[index] = {
            reinterpret_cast<const std::uint8_t*>(deep_parts[index].data()),
            deep_parts[index].size()};
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_RESOLVE_DESTINATION;
    request.intent_id = 3U;
    request.identity = root_grant.resolved;
    request.relative_components = deep_spans.data();
    request.relative_component_count = deep_spans.size();
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.identity == nullptr) {
        return 62;
    }
    const InkpodInkScriptPathIdentity deep_destination = *response.identity;
    const std::string deep_key{
        reinterpret_cast<const char*>(deep_destination.canonical_key.bytes),
        static_cast<std::size_t>(deep_destination.canonical_key.byte_count)};
    std::vector<std::uint8_t> deep_key_storage(
        deep_key.begin(), deep_key.end());
    InkpodInkScriptPathIdentity deep_destination_copy = deep_destination;
    deep_destination_copy.canonical_key = {
        deep_key_storage.data(), deep_key_storage.size()};
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_PREPARE_DESTINATION;
    request.identity = &deep_destination_copy;
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.record_count != 2U
        || GetFileAttributesW(
               (root + L"\\deep-one\\deep-two").c_str())
            == INVALID_FILE_ATTRIBUTES) {
        return 63;
    }

    // Open-session identity is object-based, so a hard-link spelling cannot be
    // selected as an overwrite destination.
    if (adapter.RegisterOpenSession(41U, 2U, 7U, 8U, original)
            != INKPOD_STATUS_OK) {
        return 26;
    }
    const std::string_view alias_name = "alias.inkpod";
    InkpodInkScriptUtf8Span alias_span{
        reinterpret_cast<const std::uint8_t*>(alias_name.data()),
        alias_name.size()};
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_RESOLVE_DESTINATION;
    request.intent_id = 3U;
    request.identity = root_grant.resolved;
    request.relative_components = &alias_span;
    request.relative_component_count = 1U;
    if (Call(host, request, response) != INKPOD_STATUS_INVALID_STATE) {
        return 27;
    }

    // The guard denies an already-open or newly-opened writer while retaining
    // an object handle for exact fingerprinting.
    InkpodInkScriptNativeFingerprint source{};
    source.struct_size = sizeof(source);
    source.version = INKPOD_INKSCRIPT_RECORD_VERSION;
    source.path = original_grant.resolved;
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_ACQUIRE_OVERWRITE_GUARD;
    request.fingerprint = &source;
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || IsZero(response.overwrite_guard, sizeof(response.overwrite_guard))) {
        return 28;
    }
    const std::array<std::uint8_t, 32U> guard = [&response] {
        std::array<std::uint8_t, 32U> value{};
        std::copy_n(response.overwrite_guard, value.size(), value.begin());
        return value;
    }();
    HANDLE denied_writer = CreateFileW(
        original.c_str(),
        GENERIC_WRITE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (denied_writer != INVALID_HANDLE_VALUE) {
        CloseHandle(denied_writer);
        return 29;
    }
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_RELEASE_OVERWRITE_GUARD;
    std::copy(guard.begin(), guard.end(), request.overwrite_guard);
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 30;
    }
    HANDLE allowed_writer = CreateFileW(
        original.c_str(),
        GENERIC_WRITE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (allowed_writer == INVALID_HANDLE_VALUE) {
        return 31;
    }
    CloseHandle(allowed_writer);

    const std::wstring overwrite_path = root + L"\\overwrite.inkpod";
    constexpr std::array<std::uint8_t, 3U> overwrite_before{6U, 6U, 6U};
    constexpr std::array<std::uint8_t, 4U> overwrite_after{7U, 7U, 7U, 7U};
    if (!WriteBytes(overwrite_path, overwrite_before)) {
        return 56;
    }
    const std::string_view overwrite_name = "overwrite.inkpod";
    InkpodInkScriptUtf8Span overwrite_span{
        reinterpret_cast<const std::uint8_t*>(overwrite_name.data()),
        overwrite_name.size()};
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_RESOLVE_DESTINATION;
    request.intent_id = 3U;
    request.identity = root_grant.resolved;
    request.relative_components = &overwrite_span;
    request.relative_component_count = 1U;
    if (Call(host, request, response) != INKPOD_STATUS_OK
        || response.identity == nullptr) {
        return 57;
    }
    const InkpodInkScriptPathIdentity overwrite_destination = *response.identity;
    const std::string overwrite_key{
        reinterpret_cast<const char*>(overwrite_destination.canonical_key.bytes),
        static_cast<std::size_t>(
            overwrite_destination.canonical_key.byte_count)};
    std::vector<std::uint8_t> overwrite_key_storage(
        overwrite_key.begin(), overwrite_key.end());
    InkpodInkScriptPathIdentity overwrite_destination_copy =
        overwrite_destination;
    overwrite_destination_copy.canonical_key = {
        overwrite_key_storage.data(), overwrite_key_storage.size()};
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_CREATE_TEMPORARY;
    request.identity = &overwrite_destination_copy;
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 58;
    }
    const InkpodInkScriptTemporaryIdentity overwrite_temporary =
        response.temporary;
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_WRITE_CLOSE_TEMPORARY;
    request.temporary = overwrite_temporary;
    request.bytes = overwrite_after.data();
    request.byte_count = overwrite_after.size();
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 59;
    }
    InkpodInkScriptNativeFingerprint overwrite_source{};
    overwrite_source.struct_size = sizeof(overwrite_source);
    overwrite_source.version = INKPOD_INKSCRIPT_RECORD_VERSION;
    overwrite_source.path = &overwrite_destination_copy;
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_ACQUIRE_OVERWRITE_GUARD;
    request.fingerprint = &overwrite_source;
    if (Call(host, request, response) != INKPOD_STATUS_OK) {
        return 60;
    }
    const std::array<std::uint8_t, 32U> overwrite_guard = [&response] {
        std::array<std::uint8_t, 32U> value{};
        std::copy_n(response.overwrite_guard, value.size(), value.begin());
        return value;
    }();
    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_ATOMIC_INSTALL;
    request.flags = INKPOD_INKSCRIPT_HOST_HAS_OVERWRITE_GUARD;
    request.temporary = overwrite_temporary;
    request.identity = &overwrite_destination_copy;
    std::copy(
        overwrite_guard.begin(),
        overwrite_guard.end(),
        request.overwrite_guard);
    const InkpodStatus overwrite_status = Call(host, request, response);
    if (overwrite_status != INKPOD_STATUS_OK) {
        return 150 + static_cast<int>(overwrite_status);
    }
    if (response.result_kind != 1U) {
        return 170;
    }
    if (!ReadBytes(overwrite_path, observed_bytes)) {
        return 171;
    }
    if (!std::equal(
            observed_bytes.begin(),
            observed_bytes.end(),
            overwrite_after.begin(),
            overwrite_after.end())) {
        return 172;
    }

    // A callback from another thread is rejected and cannot mutate authority.
    InkpodStatus cross_thread_status = INKPOD_STATUS_OK;
    std::pair<InkpodInkScriptHostAdapter*, InkpodStatus*> state{
        &host, &cross_thread_status};
    HANDLE thread = CreateThread(
        nullptr,
        0U,
        [](void* value) -> DWORD {
            auto* thread_state = static_cast<std::pair<InkpodInkScriptHostAdapter*, InkpodStatus*>*>(value);
            InkpodInkScriptHostRequest cross_request{};
            InkpodInkScriptHostResponse cross_response{};
            cross_request.operation = INKPOD_INKSCRIPT_HOST_AUTHORITY_GENERATION;
            *thread_state->second = Call(
                *thread_state->first, cross_request, cross_response);
            return 0U;
        },
        &state,
        0U,
        nullptr);
    if (thread == nullptr || WaitForSingleObject(thread, 5000U) != WAIT_OBJECT_0) {
        if (thread != nullptr) {
            CloseHandle(thread);
        }
        return 32;
    }
    CloseHandle(thread);
    if (cross_thread_status != INKPOD_STATUS_WRONG_THREAD) {
        return 33;
    }

    request = {};
    request.operation = INKPOD_INKSCRIPT_HOST_CLEANUP_TEMPORARY;
    request.temporary = collision_temp;
    if (Call(collision_host, request, response) != INKPOD_STATUS_OK) {
        return 34;
    }
    request.temporary = second_collision_temp;
    if (Call(second_collision_host, request, response) != INKPOD_STATUS_OK) {
        return 55;
    }
    return 0;
}

}  // namespace

int wmain() {
    return Run();
}
