#include "inkscript_file_authority.h"

#include <windows.h>
#include <winternl.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cctype>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <limits>
#include <map>
#include <memory>
#include <new>
#include <span>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "blake3_digest.h"

namespace inkpod::app {
namespace {

constexpr std::uint64_t maximum_file_bytes = UINT64_C(4) * 1024U * 1024U * 1024U;
constexpr ULONG object_attributes_case_insensitive = 0x00000040UL;
constexpr ULONG file_open_disposition = 1UL;
constexpr ULONG file_create_disposition = 2UL;
constexpr ULONG file_directory_option = 0x00000001UL;
constexpr ULONG file_synchronous_option = 0x00000020UL;
constexpr ULONG file_non_directory_option = 0x00000040UL;
constexpr ULONG file_open_reparse_option = 0x00200000UL;
constexpr NTSTATUS status_object_name_not_found =
    static_cast<NTSTATUS>(0xc0000034UL);
constexpr NTSTATUS status_object_path_not_found =
    static_cast<NTSTATUS>(0xc000003aUL);
constexpr NTSTATUS status_no_such_file =
    static_cast<NTSTATUS>(0xc000000fUL);
constexpr NTSTATUS status_object_name_collision =
    static_cast<NTSTATUS>(0xc0000035UL);

using NtCreateFileFunction = NTSTATUS(NTAPI*)(
    PHANDLE,
    ACCESS_MASK,
    POBJECT_ATTRIBUTES,
    PIO_STATUS_BLOCK,
    PLARGE_INTEGER,
    ULONG,
    ULONG,
    ULONG,
    ULONG,
    PVOID,
    ULONG);

using NtSetInformationFileFunction = NTSTATUS(NTAPI*)(
    HANDLE,
    PIO_STATUS_BLOCK,
    PVOID,
    ULONG,
    FILE_INFORMATION_CLASS);

class UniqueHandle final {
public:
    UniqueHandle() noexcept = default;
    explicit UniqueHandle(HANDLE handle) noexcept : handle_(handle) {}
    ~UniqueHandle() { Reset(); }

    UniqueHandle(const UniqueHandle&) = delete;
    UniqueHandle& operator=(const UniqueHandle&) = delete;

    UniqueHandle(UniqueHandle&& other) noexcept
        : handle_(std::exchange(other.handle_, INVALID_HANDLE_VALUE)) {}

    UniqueHandle& operator=(UniqueHandle&& other) noexcept {
        if (this != &other) {
            Reset(std::exchange(other.handle_, INVALID_HANDLE_VALUE));
        }
        return *this;
    }

    [[nodiscard]] HANDLE Get() const noexcept { return handle_; }
    [[nodiscard]] explicit operator bool() const noexcept {
        return handle_ != nullptr && handle_ != INVALID_HANDLE_VALUE;
    }

    HANDLE Release() noexcept {
        return std::exchange(handle_, INVALID_HANDLE_VALUE);
    }

    void Reset(HANDLE replacement = INVALID_HANDLE_VALUE) noexcept {
        if (*this) {
            CloseHandle(handle_);
        }
        handle_ = replacement;
    }

private:
    HANDLE handle_{INVALID_HANDLE_VALUE};
};

struct OwnedPath final {
    std::string canonical_key;
    std::wstring absolute_path;
    InkpodInkScriptPathIdentity record{};
    bool directory{};

    OwnedPath() = default;
    OwnedPath(const OwnedPath&) = delete;
    OwnedPath& operator=(const OwnedPath&) = delete;

    OwnedPath(OwnedPath&& other) noexcept
        : canonical_key(std::move(other.canonical_key)),
          absolute_path(std::move(other.absolute_path)),
          record(other.record),
          directory(other.directory) {
        RefreshPointer();
    }

    OwnedPath& operator=(OwnedPath&& other) noexcept {
        if (this != &other) {
            canonical_key = std::move(other.canonical_key);
            absolute_path = std::move(other.absolute_path);
            record = other.record;
            directory = other.directory;
            RefreshPointer();
        }
        return *this;
    }

    void RefreshPointer() noexcept {
        record.canonical_key.bytes = canonical_key.empty()
            ? nullptr
            : reinterpret_cast<const std::uint8_t*>(canonical_key.data());
        record.canonical_key.byte_count = canonical_key.size();
    }
};

struct ObservedPath final {
    OwnedPath path;
    UniqueHandle target;
    UniqueHandle parent;
};

struct OwnedFingerprint final {
    OwnedPath path;
    std::string display_label;
    InkpodInkScriptNativeFingerprint record{};

    void RefreshPointers() noexcept {
        path.RefreshPointer();
        record.path = &path.record;
        record.display_label = {
            display_label.empty()
                ? nullptr
                : reinterpret_cast<const std::uint8_t*>(display_label.data()),
            display_label.size()};
    }
};

bool IsSuccess(NTSTATUS status) noexcept {
    return status >= 0;
}

bool IsMissing(NTSTATUS status) noexcept {
    return status == status_object_name_not_found
        || status == status_object_path_not_found || status == status_no_such_file;
}

NtCreateFileFunction ResolveNtCreateFile() noexcept {
    static const NtCreateFileFunction function = [] {
        HMODULE module = GetModuleHandleW(L"ntdll.dll");
        if (module == nullptr) {
            return static_cast<NtCreateFileFunction>(nullptr);
        }
        return reinterpret_cast<NtCreateFileFunction>(
            GetProcAddress(module, "NtCreateFile"));
    }();
    return function;
}

NtSetInformationFileFunction ResolveNtSetInformationFile() noexcept {
    static const NtSetInformationFileFunction function = [] {
        HMODULE module = GetModuleHandleW(L"ntdll.dll");
        if (module == nullptr) {
            return static_cast<NtSetInformationFileFunction>(nullptr);
        }
        return reinterpret_cast<NtSetInformationFileFunction>(
            GetProcAddress(module, "NtSetInformationFile"));
    }();
    return function;
}

NTSTATUS OpenRelative(
    HANDLE parent,
    std::wstring_view component,
    ACCESS_MASK access,
    ULONG share,
    ULONG disposition,
    ULONG attributes,
    ULONG options,
    UniqueHandle& output) noexcept {
    output.Reset();
    const NtCreateFileFunction create_file = ResolveNtCreateFile();
    if (create_file == nullptr || parent == INVALID_HANDLE_VALUE
        || component.empty()
        || component.size()
            > std::numeric_limits<USHORT>::max() / sizeof(wchar_t)) {
        return static_cast<NTSTATUS>(0xc000000dUL);
    }
    UNICODE_STRING name{};
    name.Buffer = const_cast<PWSTR>(component.data());
    name.Length = static_cast<USHORT>(component.size() * sizeof(wchar_t));
    name.MaximumLength = name.Length;
    OBJECT_ATTRIBUTES object_attributes{};
    object_attributes.Length = sizeof(object_attributes);
    object_attributes.RootDirectory = parent;
    object_attributes.ObjectName = &name;
    object_attributes.Attributes = object_attributes_case_insensitive;
    IO_STATUS_BLOCK status_block{};
    HANDLE handle = INVALID_HANDLE_VALUE;
    const NTSTATUS status = create_file(
        &handle,
        access,
        &object_attributes,
        &status_block,
        nullptr,
        attributes,
        share,
        disposition,
        options
            | (disposition == file_create_disposition
                    ? 0U
                    : file_open_reparse_option),
        nullptr,
        0U);
    if (IsSuccess(status)) {
        output.Reset(handle);
    }
    return status;
}

bool Duplicate(HANDLE source, UniqueHandle& output) noexcept {
    HANDLE duplicate = INVALID_HANDLE_VALUE;
    if (DuplicateHandle(
            GetCurrentProcess(),
            source,
            GetCurrentProcess(),
            &duplicate,
            0U,
            FALSE,
            DUPLICATE_SAME_ACCESS)
        == FALSE) {
        return false;
    }
    output.Reset(duplicate);
    return true;
}

bool NormalizeAbsolutePath(
    const std::wstring& input,
    std::wstring& output) noexcept {
    output.clear();
    if (input.empty() || input.size() >= 32767U) {
        return false;
    }
    std::array<wchar_t, 32768U> buffer{};
    const DWORD length = GetFullPathNameW(
        input.c_str(), static_cast<DWORD>(buffer.size()), buffer.data(), nullptr);
    if (length == 0U || length >= buffer.size()) {
        return false;
    }
    try {
        output.assign(buffer.data(), length);
        std::replace(output.begin(), output.end(), L'/', L'\\');
        while (output.size() > 3U && output.back() == L'\\') {
            output.pop_back();
        }
    } catch (const std::bad_alloc&) {
        output.clear();
        return false;
    }
    return output.size() >= 3U
        && ((output[0] >= L'A' && output[0] <= L'Z')
            || (output[0] >= L'a' && output[0] <= L'z'))
        && output[1] == L':' && output[2] == L'\\';
}

bool FinalOpenedDosPath(HANDLE handle, std::wstring& output) noexcept {
    output.clear();
    std::array<wchar_t, 32768U> buffer{};
    const DWORD length = GetFinalPathNameByHandleW(
        handle,
        buffer.data(),
        static_cast<DWORD>(buffer.size()),
        FILE_NAME_OPENED | VOLUME_NAME_DOS);
    if (length == 0U || length >= buffer.size()) {
        return false;
    }
    try {
        std::wstring opened{buffer.data(), length};
        constexpr std::wstring_view extended_dos_prefix{L"\\\\?\\"};
        if (opened.starts_with(extended_dos_prefix)) {
            opened.erase(0U, extended_dos_prefix.size());
        }
        return NormalizeAbsolutePath(opened, output);
    } catch (const std::bad_alloc&) {
        output.clear();
        return false;
    }
}

bool ValidComponent(std::wstring_view component) noexcept {
    if (component.empty() || component == L"." || component == L".."
        || component.back() == L'.' || component.back() == L' ') {
        return false;
    }
    for (wchar_t value : component) {
        if (value < 32 || value == L'\\' || value == L'/' || value == L':'
            || value == L'*' || value == L'?' || value == L'"'
            || value == L'<' || value == L'>' || value == L'|') {
            return false;
        }
    }
    return true;
}

bool SplitAbsolute(
    const std::wstring& absolute,
    std::wstring& root,
    std::vector<std::wstring>& components) {
    if (absolute.size() < 3U || absolute[1] != L':' || absolute[2] != L'\\') {
        return false;
    }
    root = absolute.substr(0U, 3U);
    components.clear();
    std::size_t start = 3U;
    while (start < absolute.size()) {
        const std::size_t end = absolute.find(L'\\', start);
        const std::size_t count = end == std::wstring::npos
            ? absolute.size() - start
            : end - start;
        if (count == 0U) {
            return false;
        }
        components.emplace_back(absolute.substr(start, count));
        if (!ValidComponent(components.back())) {
            return false;
        }
        if (end == std::wstring::npos) {
            break;
        }
        start = end + 1U;
    }
    return true;
}

bool IsReparse(HANDLE handle, bool& directory) noexcept {
    FILE_ATTRIBUTE_TAG_INFO tags{};
    if (GetFileInformationByHandleEx(
            handle, FileAttributeTagInfo, &tags, sizeof(tags))
        == FALSE) {
        return true;
    }
    directory = (tags.FileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0U;
    return (tags.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) != 0U;
}

std::uint64_t NonzeroHash64(std::span<const std::uint8_t> bytes) noexcept {
    std::uint64_t value = UINT64_C(1469598103934665603);
    for (std::uint8_t byte : bytes) {
        value ^= byte;
        value *= UINT64_C(1099511628211);
    }
    return value == 0U ? 1U : value;
}

template <typename Value>
void AppendValue(std::vector<std::uint8_t>& bytes, const Value& value) {
    const auto* begin = reinterpret_cast<const std::uint8_t*>(&value);
    bytes.insert(bytes.end(), begin, begin + sizeof(value));
}

bool QueryFileIdentity(
    HANDLE handle,
    std::array<std::uint8_t, 16U>& volume,
    std::array<std::uint8_t, 32U>& object) noexcept {
    FILE_ID_INFO info{};
    if (GetFileInformationByHandleEx(handle, FileIdInfo, &info, sizeof(info))
        == FALSE) {
        return false;
    }
    volume.fill(0U);
    object.fill(0U);
    std::memcpy(volume.data(), &info.VolumeSerialNumber, sizeof(info.VolumeSerialNumber));
    volume[15U] = 0xa5U;
    static_assert(sizeof(info.FileId.Identifier) == 16U);
    std::memcpy(object.data(), info.FileId.Identifier, sizeof(info.FileId.Identifier));
    std::memcpy(
        object.data() + sizeof(info.FileId.Identifier),
        &info.VolumeSerialNumber,
        sizeof(info.VolumeSerialNumber));
    object[31U] = 0x5aU;
    return true;
}

std::uint64_t QueryGeneration(HANDLE handle, bool directory) {
    FILE_BASIC_INFO basic{};
    FILE_STANDARD_INFO standard{};
    if (GetFileInformationByHandleEx(
            handle, FileBasicInfo, &basic, sizeof(basic))
            == FALSE
        || GetFileInformationByHandleEx(
               handle, FileStandardInfo, &standard, sizeof(standard))
            == FALSE) {
        return 0U;
    }
    std::vector<std::uint8_t> bytes;
    bytes.reserve(80U);
    AppendValue(bytes, basic.CreationTime.QuadPart);
    if (!directory) {
        AppendValue(bytes, basic.LastWriteTime.QuadPart);
        AppendValue(bytes, basic.ChangeTime.QuadPart);
        AppendValue(bytes, standard.EndOfFile.QuadPart);
    }
    AppendValue(bytes, standard.NumberOfLinks);
    return NonzeroHash64(bytes);
}

std::string CanonicalKey(const std::wstring& absolute) {
    std::wstring mapped = absolute;
    if (!mapped.empty()
        && LCMapStringEx(
               LOCALE_NAME_INVARIANT,
               LCMAP_LOWERCASE,
               mapped.data(),
               static_cast<int>(mapped.size()),
               mapped.data(),
               static_cast<int>(mapped.size()),
               nullptr,
               nullptr,
               0U)
            == 0) {
        return {};
    }
    std::replace(mapped.begin(), mapped.end(), L'\\', L'/');
    const int count = mapped.empty()
        ? 0
        : WideCharToMultiByte(
            CP_UTF8,
            WC_ERR_INVALID_CHARS,
            mapped.data(),
            static_cast<int>(mapped.size()),
            nullptr,
            0,
            nullptr,
            nullptr);
    if (!mapped.empty() && count <= 0) {
        return {};
    }
    std::string output{"win:/"};
    const std::size_t prefix_size = output.size();
    output.resize(prefix_size + static_cast<std::size_t>(count));
    if (count != 0
        && WideCharToMultiByte(
               CP_UTF8,
               WC_ERR_INVALID_CHARS,
               mapped.data(),
               static_cast<int>(mapped.size()),
               output.data() + prefix_size,
               count,
               nullptr,
               nullptr)
            != count) {
        return {};
    }
    return output;
}

std::array<std::uint8_t, 32U> OpaqueKey(std::string_view value) noexcept {
    return Blake3Digest(std::span<const std::uint8_t>{
        reinterpret_cast<const std::uint8_t*>(value.data()), value.size()});
}

bool CopyArray(
    const std::array<std::uint8_t, 16U>& source,
    std::uint8_t (&destination)[16U]) noexcept {
    std::copy(source.begin(), source.end(), destination);
    return true;
}

bool CopyArray(
    const std::array<std::uint8_t, 32U>& source,
    std::uint8_t (&destination)[32U]) noexcept {
    std::copy(source.begin(), source.end(), destination);
    return true;
}

bool SameBytes(
    const std::uint8_t* left,
    const std::uint8_t* right,
    std::size_t count) noexcept {
    return std::memcmp(left, right, count) == 0;
}

bool SameObject(
    const InkpodInkScriptPathIdentity& left,
    const InkpodInkScriptPathIdentity& right) noexcept {
    return SameBytes(left.volume_id, right.volume_id, sizeof(left.volume_id))
        && SameBytes(left.object_id, right.object_id, sizeof(left.object_id));
}

bool ReadUtf8Span(
    InkpodInkScriptUtf8Span span,
    std::string& output) {
    if ((span.byte_count > 0U && span.bytes == nullptr)
        || span.byte_count > static_cast<std::uint64_t>(INT_MAX)
        || span.byte_count > std::numeric_limits<std::size_t>::max()) {
        return false;
    }
    output.assign(
        span.byte_count == 0U
            ? ""
            : reinterpret_cast<const char*>(span.bytes),
        static_cast<std::size_t>(span.byte_count));
    if (output.find('\0') != std::string::npos) {
        return false;
    }
    const int wide_count = output.empty()
        ? 0
        : MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            output.data(),
            static_cast<int>(output.size()),
            nullptr,
            0);
    return output.empty() || wide_count > 0;
}

bool Utf8ToWide(std::string_view input, std::wstring& output) {
    output.clear();
    if (input.empty() || input.size() > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int count = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        input.data(),
        static_cast<int>(input.size()),
        nullptr,
        0);
    if (count <= 0) {
        return false;
    }
    output.resize(static_cast<std::size_t>(count));
    return MultiByteToWideChar(
               CP_UTF8,
               MB_ERR_INVALID_CHARS,
               input.data(),
               static_cast<int>(input.size()),
               output.data(),
               count)
        == count;
}

bool WideToUtf8(std::wstring_view input, std::string& output) {
    output.clear();
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
               output.data(),
               count,
               nullptr,
               nullptr)
        == count;
}

InkpodStatus ObserveAbsolute(
    const std::wstring& input,
    bool permit_missing_tail,
    ObservedPath& output) {
    output = {};
    std::wstring absolute;
    std::wstring root;
    std::vector<std::wstring> components;
    if (!NormalizeAbsolutePath(input, absolute)
        || !SplitAbsolute(absolute, root, components)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    UniqueHandle current{CreateFileW(
        root.c_str(),
        FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        nullptr)};
    bool current_directory{};
    if (!current || IsReparse(current.Get(), current_directory)
        || !current_directory) {
        return INKPOD_STATUS_IO_ERROR;
    }

    if (components.empty()) {
        if (!Duplicate(current.Get(), output.parent)) {
            return INKPOD_STATUS_IO_ERROR;
        }
        output.target = std::move(current);
    } else {
        for (std::size_t index = 0; index < components.size(); ++index) {
            const bool final = index + 1U == components.size();
            UniqueHandle next;
            const NTSTATUS status = OpenRelative(
                current.Get(),
                components[index],
                FILE_READ_ATTRIBUTES | FILE_READ_DATA | SYNCHRONIZE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                file_open_disposition,
                FILE_ATTRIBUTE_NORMAL,
                file_synchronous_option,
                next);
            if (!IsSuccess(status)) {
                if (!permit_missing_tail || !IsMissing(status)) {
                    return IsMissing(status)
                        ? INKPOD_STATUS_INVALID_STATE
                        : INKPOD_STATUS_IO_ERROR;
                }
                std::array<std::uint8_t, 16U> parent_volume{};
                std::array<std::uint8_t, 32U> parent_object{};
                if (!QueryFileIdentity(
                        current.Get(), parent_volume, parent_object)) {
                    return INKPOD_STATUS_IO_ERROR;
                }
                const std::uint64_t parent_generation =
                    QueryGeneration(current.Get(), true);
                std::wstring parent_absolute;
                if (!FinalOpenedDosPath(current.Get(), parent_absolute)) {
                    return INKPOD_STATUS_IO_ERROR;
                }
                std::wstring resolved_absolute = parent_absolute;
                for (std::size_t part = index; part < components.size(); ++part) {
                    if (resolved_absolute.back() != L'\\') {
                        resolved_absolute.push_back(L'\\');
                    }
                    resolved_absolute.append(components[part]);
                }
                const std::string canonical = CanonicalKey(resolved_absolute);
                const std::string parent_key = CanonicalKey(parent_absolute);
                if (canonical.empty() || parent_key.empty()
                    || parent_generation == 0U) {
                    return INKPOD_STATUS_IO_ERROR;
                }
                output.path.canonical_key = canonical;
                output.path.absolute_path = std::move(resolved_absolute);
                output.path.directory = false;
                output.path.record.struct_size = sizeof(output.path.record);
                output.path.record.version = INKPOD_INKSCRIPT_RECORD_VERSION;
                CopyArray(parent_volume, output.path.record.volume_id);
                CopyArray(OpaqueKey(canonical), output.path.record.alias_key);
                CopyArray(
                    parent_object, output.path.record.parent_object_id);
                output.path.record.parent_generation = parent_generation;
                CopyArray(
                    OpaqueKey(parent_key),
                    output.path.record.parent_alias_key);
                output.path.record.flags = INKPOD_INKSCRIPT_PATH_EXPECTED_ABSENT;
                output.path.RefreshPointer();
                output.parent = std::move(current);
                return INKPOD_STATUS_OK;
            }
            bool directory{};
            if (IsReparse(next.Get(), directory) || (!final && !directory)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            if (final) {
                output.parent = std::move(current);
                output.target = std::move(next);
                output.path.directory = directory;
            } else {
                current = std::move(next);
            }
        }
    }

    std::array<std::uint8_t, 16U> volume{};
    std::array<std::uint8_t, 32U> object{};
    std::array<std::uint8_t, 16U> parent_volume{};
    std::array<std::uint8_t, 32U> parent_object{};
    if (!QueryFileIdentity(output.target.Get(), volume, object)
        || !QueryFileIdentity(
            output.parent.Get(), parent_volume, parent_object)
        || volume != parent_volume) {
        return INKPOD_STATUS_IO_ERROR;
    }
    const std::uint64_t object_generation = QueryGeneration(
        output.target.Get(), output.path.directory);
    const std::uint64_t parent_generation = QueryGeneration(
        output.parent.Get(), true);
    std::wstring target_absolute;
    std::wstring parent_absolute;
    if (!FinalOpenedDosPath(output.target.Get(), target_absolute)
        || !FinalOpenedDosPath(output.parent.Get(), parent_absolute)) {
        return INKPOD_STATUS_IO_ERROR;
    }
    const std::string canonical = CanonicalKey(target_absolute);
    const std::string parent_key = CanonicalKey(parent_absolute);
    if (canonical.empty() || parent_key.empty() || object_generation == 0U
        || parent_generation == 0U) {
        return INKPOD_STATUS_IO_ERROR;
    }
    output.path.canonical_key = canonical;
    output.path.absolute_path = std::move(target_absolute);
    output.path.record.struct_size = sizeof(output.path.record);
    output.path.record.version = INKPOD_INKSCRIPT_RECORD_VERSION;
    CopyArray(volume, output.path.record.volume_id);
    CopyArray(object, output.path.record.object_id);
    output.path.record.object_generation = object_generation;
    CopyArray(OpaqueKey(canonical), output.path.record.alias_key);
    CopyArray(parent_object, output.path.record.parent_object_id);
    output.path.record.parent_generation = parent_generation;
    CopyArray(OpaqueKey(parent_key), output.path.record.parent_alias_key);
    output.path.RefreshPointer();
    return INKPOD_STATUS_OK;
}

bool RecordMatches(
    const InkpodInkScriptPathIdentity& expected,
    const InkpodInkScriptPathIdentity& observed) {
    std::string expected_key;
    std::string observed_key;
    return expected.struct_size >= sizeof(expected)
        && expected.version == INKPOD_INKSCRIPT_RECORD_VERSION
        && expected.feature_flags == 0U
        && ReadUtf8Span(expected.canonical_key, expected_key)
        && ReadUtf8Span(observed.canonical_key, observed_key)
        && expected_key == observed_key
        && SameBytes(expected.volume_id, observed.volume_id, sizeof(expected.volume_id))
        && SameBytes(expected.object_id, observed.object_id, sizeof(expected.object_id))
        && expected.object_generation == observed.object_generation
        && SameBytes(expected.alias_key, observed.alias_key, sizeof(expected.alias_key))
        && SameBytes(
            expected.parent_object_id,
            observed.parent_object_id,
            sizeof(expected.parent_object_id))
        && expected.parent_generation == observed.parent_generation
        && SameBytes(
            expected.parent_alias_key,
            observed.parent_alias_key,
            sizeof(expected.parent_alias_key))
        && expected.flags == observed.flags;
}

std::string PathKey(const InkpodInkScriptPathIdentity& identity) {
    std::string key;
    if (!ReadUtf8Span(identity.canonical_key, key)) {
        return {};
    }
    return key;
}

std::array<std::uint8_t, 32U> Token(
    std::string_view label,
    std::uint64_t generation) {
    std::string input{label};
    input.push_back('#');
    input.append(std::to_string(generation));
    return OpaqueKey(input);
}

bool ReadHandleBytes(HANDLE handle, std::vector<std::uint8_t>& bytes) {
    LARGE_INTEGER size{};
    if (GetFileSizeEx(handle, &size) == FALSE || size.QuadPart <= 0
        || static_cast<std::uint64_t>(size.QuadPart) > maximum_file_bytes
        || static_cast<std::uint64_t>(size.QuadPart)
            > std::numeric_limits<std::size_t>::max()) {
        return false;
    }
    LARGE_INTEGER zero{};
    if (SetFilePointerEx(handle, zero, nullptr, FILE_BEGIN) == FALSE) {
        return false;
    }
    bytes.resize(static_cast<std::size_t>(size.QuadPart));
    std::size_t offset{};
    while (offset < bytes.size()) {
        const DWORD count = static_cast<DWORD>(std::min<std::size_t>(
            bytes.size() - offset, static_cast<std::size_t>(MAXDWORD)));
        DWORD read{};
        if (ReadFile(handle, bytes.data() + offset, count, &read, nullptr) == FALSE
            || read != count) {
            bytes.clear();
            return false;
        }
        offset += read;
    }
    return true;
}

std::uint32_t DisplayNumber(std::wstring_view path) noexcept {
    const std::size_t slash = path.find_last_of(L"\\/");
    const std::size_t dot = path.find_last_of(L'.');
    const std::size_t end = dot == std::wstring_view::npos ? path.size() : dot;
    std::size_t start = end;
    while (start > (slash == std::wstring_view::npos ? 0U : slash + 1U)
        && path[start - 1U] >= L'0' && path[start - 1U] <= L'9') {
        --start;
    }
    if (start == end) {
        return 1U;
    }
    std::uint64_t number{};
    for (std::size_t index = start; index < end; ++index) {
        number = number * 10U + static_cast<std::uint64_t>(path[index] - L'0');
        if (number > UINT32_MAX) {
            return 1U;
        }
    }
    return number == 0U ? 1U : static_cast<std::uint32_t>(number);
}

}  // namespace

struct InkScriptFileAuthorityAdapter::Impl final {
    struct Grant final {
        std::uint32_t access{};
        std::array<std::uint8_t, 32U> authority_id{};
        OwnedPath path;
    };

    struct Asset final {
        OwnedPath path;
    };

    struct OpenSession final {
        std::uint64_t session_id{};
        std::uint64_t session_generation{};
        std::uint64_t uuid_high{};
        std::uint64_t uuid_low{};
        OwnedPath path;
    };

    struct Temporary final {
        InkpodInkScriptTemporaryIdentity identity{};
        std::wstring parent_path;
        std::wstring component;
        UniqueHandle handle;
        bool closed{};
    };

    struct Guard final {
        std::array<std::uint8_t, 32U> token{};
        OwnedPath path;
        UniqueHandle handle;
    };

    explicit Impl() noexcept : owner_thread(GetCurrentThreadId()) {}

    ~Impl() {
        for (auto& [key, temporary] : temporaries) {
            (void)key;
            CleanupTemporary(temporary);
        }
    }

    [[nodiscard]] bool OwnerThread() const noexcept {
        return GetCurrentThreadId() == owner_thread;
    }

    InkpodStatus AdvanceGeneration(std::uint64_t& generation) noexcept {
        if (generation == UINT64_MAX) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        ++generation;
        return INKPOD_STATUS_OK;
    }

    void ResetScratch() {
        scratch_paths.clear();
        scratch_fingerprints.clear();
        scratch_open_sessions.clear();
        scratch_bytes.clear();
    }

    OwnedPath* KeepPath(OwnedPath path) {
        auto owned = std::make_unique<OwnedPath>(std::move(path));
        OwnedPath* result = owned.get();
        scratch_paths.push_back(std::move(owned));
        locations[result->canonical_key] = result->absolute_path;
        return result;
    }

    OwnedFingerprint* KeepFingerprint(OwnedFingerprint fingerprint) {
        auto owned = std::make_unique<OwnedFingerprint>(std::move(fingerprint));
        owned->RefreshPointers();
        OwnedFingerprint* result = owned.get();
        scratch_fingerprints.push_back(std::move(owned));
        locations[result->path.canonical_key] = result->path.absolute_path;
        return result;
    }

    InkpodStatus ObserveKnown(
        const InkpodInkScriptPathIdentity* identity,
        bool permit_missing,
        ObservedPath& observed) {
        if (identity == nullptr || identity->struct_size < sizeof(*identity)
            || identity->version != INKPOD_INKSCRIPT_RECORD_VERSION
            || identity->feature_flags != 0U) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        const std::string key = PathKey(*identity);
        const auto location = locations.find(key);
        if (key.empty() || location == locations.end()) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        return ObserveAbsolute(location->second, permit_missing, observed);
    }

    InkpodStatus BuildFingerprint(
        ObservedPath observed,
        std::vector<std::uint8_t>* retained_bytes,
        OwnedFingerprint& output) {
        if ((observed.path.record.flags & INKPOD_INKSCRIPT_PATH_EXPECTED_ABSENT) != 0U
            || observed.path.directory) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        UniqueHandle locked{CreateFileW(
            observed.path.absolute_path.c_str(),
            GENERIC_READ | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ,
            nullptr,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            nullptr)};
        bool locked_directory{};
        std::array<std::uint8_t, 16U> locked_volume{};
        std::array<std::uint8_t, 32U> locked_object{};
        if (!locked || IsReparse(locked.Get(), locked_directory)
            || locked_directory
            || !QueryFileIdentity(locked.Get(), locked_volume, locked_object)
            || !SameBytes(
                locked_volume.data(),
                observed.path.record.volume_id,
                locked_volume.size())
            || !SameBytes(
                locked_object.data(),
                observed.path.record.object_id,
                locked_object.size())) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        observed.target = std::move(locked);
        std::vector<std::uint8_t> bytes;
        if (!ReadHandleBytes(observed.target.Get(), bytes)) {
            return INKPOD_STATUS_IO_ERROR;
        }
        std::string path_utf8;
        if (!WideToUtf8(observed.path.absolute_path, path_utf8)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        InkpodCoreConfig config{
            sizeof(InkpodCoreConfig), inkpod_abi_version(), INKPOD_FEATURE_NONE};
        InkpodCore* core{};
        InkpodDocumentInfo info{};
        info.struct_size = sizeof(info);
        InkpodStatus status = inkpod_core_create(&config, &core);
        if (status == INKPOD_STATUS_OK) {
            status = inkpod_core_open(
                core,
                reinterpret_cast<const std::uint8_t*>(path_utf8.data()),
                path_utf8.size(),
                &info);
        }
        const InkpodStatus destroy_status = inkpod_core_destroy(&core);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        if (destroy_status != INKPOD_STATUS_OK) {
            return destroy_status;
        }
        const std::size_t slash = observed.path.absolute_path.find_last_of(L"\\/");
        const std::wstring_view filename = std::wstring_view{observed.path.absolute_path}.substr(
            slash == std::wstring::npos ? 0U : slash + 1U);
        output = {};
        output.path = std::move(observed.path);
        if (!WideToUtf8(filename, output.display_label)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        output.record.struct_size = sizeof(output.record);
        output.record.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        output.record.display_number = DisplayNumber(output.path.absolute_path);
        output.record.flags = INKPOD_INKSCRIPT_FINGERPRINT_HAS_CHANGE_TOKEN
            | INKPOD_INKSCRIPT_FINGERPRINT_ATOMIC_OVERWRITE;
        output.record.document_uuid_low = info.document_uuid_low;
        output.record.document_uuid_high = info.document_uuid_high;
        output.record.logical_length = bytes.size();
        const auto digest = Blake3Digest(bytes);
        CopyArray(digest, output.record.content_digest);
        const auto change = Token(
            output.path.canonical_key,
            output.path.record.object_generation);
        CopyArray(change, output.record.change_token);
        output.RefreshPointers();
        if (retained_bytes != nullptr) {
            *retained_bytes = std::move(bytes);
        }
        return INKPOD_STATUS_OK;
    }

    InkpodStatus FingerprintGrant(
        std::uint64_t intent_id,
        OwnedFingerprint& output) {
        const auto found = grants.find(intent_id);
        if (found == grants.end()
            || (found->second->access != INKPOD_INKSCRIPT_PATH_READ
                && found->second->access != INKPOD_INKSCRIPT_PATH_ENUMERATE)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        ObservedPath observed;
        InkpodStatus status = ObserveAbsolute(
            found->second->path.absolute_path, false, observed);
        if (status != INKPOD_STATUS_OK
            || !RecordMatches(found->second->path.record, observed.path.record)) {
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_STATE
                : status;
        }
        return BuildFingerprint(std::move(observed), nullptr, output);
    }

    InkpodStatus ResolveDestination(
        const InkpodInkScriptHostRequest& request,
        OwnedPath& output) {
        if (request.identity == nullptr || request.relative_component_count == 0U
            || request.relative_component_count > 64U
            || request.relative_components == nullptr) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        const std::string base_key = PathKey(*request.identity);
        const auto location = locations.find(base_key);
        if (base_key.empty() || location == locations.end()) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        if (request.intent_id != 0U) {
            const auto grant = grants.find(request.intent_id);
            if (grant == grants.end()
                || (grant->second->access != INKPOD_INKSCRIPT_PATH_CREATE
                    && grant->second->access != INKPOD_INKSCRIPT_PATH_REPLACE)
                || !RecordMatches(grant->second->path.record, *request.identity)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
        }
        ObservedPath base;
        InkpodStatus status = ObserveKnown(request.identity, false, base);
        if (status != INKPOD_STATUS_OK
            || !RecordMatches(*request.identity, base.path.record)) {
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_STATE
                : status;
        }
        std::wstring destination = base.path.absolute_path;
        if (request.intent_id == 0U && !base.path.directory) {
            const std::size_t slash = destination.find_last_of(L'\\');
            if (slash == std::wstring::npos) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            destination.resize(slash);
        }
        for (std::uint64_t index = 0; index < request.relative_component_count; ++index) {
            std::string utf8;
            std::wstring component;
            if (!ReadUtf8Span(request.relative_components[index], utf8)
                || !Utf8ToWide(utf8, component) || !ValidComponent(component)) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            if (destination.size() > 3U) {
                destination.push_back(L'\\');
            }
            destination.append(component);
        }
        ObservedPath observed;
        status = ObserveAbsolute(destination, true, observed);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        if ((observed.path.record.flags & INKPOD_INKSCRIPT_PATH_EXPECTED_ABSENT) == 0U) {
            for (const auto& [session_key, session] : open_sessions) {
                (void)session_key;
                if (SameObject(session.path.record, observed.path.record)) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
            }
        }
        output = std::move(observed.path);
        locations[output.canonical_key] = output.absolute_path;
        return INKPOD_STATUS_OK;
    }

    InkpodStatus EnsureParentDirectories(
        const std::wstring& destination,
        std::vector<OwnedPath>& created) {
        constexpr ACCESS_MASK directory_access = FILE_LIST_DIRECTORY
            | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE;
        std::wstring absolute;
        std::wstring root;
        std::vector<std::wstring> components;
        if (!NormalizeAbsolutePath(destination, absolute)
            || !SplitAbsolute(absolute, root, components)
            || components.empty()) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        UniqueHandle current{CreateFileW(
            root.c_str(),
            directory_access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            nullptr,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            nullptr)};
        bool directory{};
        if (!current || IsReparse(current.Get(), directory) || !directory) {
            return INKPOD_STATUS_IO_ERROR;
        }
        std::wstring current_path = root;
        for (std::size_t index = 0; index + 1U < components.size(); ++index) {
            UniqueHandle next;
            NTSTATUS status = OpenRelative(
                current.Get(),
                components[index],
                directory_access,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                file_open_disposition,
                FILE_ATTRIBUTE_DIRECTORY,
                file_directory_option | file_synchronous_option,
                next);
            bool made{};
            if (!IsSuccess(status) && IsMissing(status)) {
                status = OpenRelative(
                    current.Get(),
                    components[index],
                    directory_access,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    file_create_disposition,
                    FILE_ATTRIBUTE_DIRECTORY,
                    file_directory_option | file_synchronous_option,
                    next);
                if (status == status_object_name_collision) {
                    status = OpenRelative(
                        current.Get(),
                        components[index],
                        directory_access,
                        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                        file_open_disposition,
                        FILE_ATTRIBUTE_DIRECTORY,
                        file_directory_option | file_synchronous_option,
                        next);
                } else {
                    made = IsSuccess(status);
                }
            }
            if (!IsSuccess(status) && !IsMissing(status)) {
                std::wstring candidate = current_path;
                if (candidate.size() > 3U) {
                    candidate.push_back(L'\\');
                }
                candidate.append(components[index]);
                const DWORD candidate_attributes = GetFileAttributesW(
                    candidate.c_str());
                if (candidate_attributes != INVALID_FILE_ATTRIBUTES
                    && (candidate_attributes & FILE_ATTRIBUTE_REPARSE_POINT)
                        != 0U) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
                UniqueHandle reparse{CreateFileW(
                    candidate.c_str(),
                    FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    nullptr,
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                    nullptr)};
                bool reparse_directory{};
                if (reparse
                    && IsReparse(reparse.Get(), reparse_directory)) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
            }
            if (!IsSuccess(status)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            if (IsReparse(next.Get(), directory)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            if (!directory) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            if (current_path.size() > 3U) {
                current_path.push_back(L'\\');
            }
            current_path.append(components[index]);
            if (made) {
                ObservedPath observed;
                const InkpodStatus observed_status = ObserveAbsolute(
                    current_path, false, observed);
                if (observed_status != INKPOD_STATUS_OK) {
                    return observed_status;
                }
                created.push_back(std::move(observed.path));
            }
            current = std::move(next);
        }
        return INKPOD_STATUS_OK;
    }

    using TemporaryKey = std::array<std::uint8_t, 32U>;

    static TemporaryKey TemporaryObjectKey(
        const InkpodInkScriptTemporaryIdentity& identity) noexcept {
        TemporaryKey key{};
        std::copy_n(identity.object_id, key.size(), key.begin());
        return key;
    }

    bool TemporaryMatches(
        const Temporary& stored,
        const InkpodInkScriptTemporaryIdentity& supplied) const noexcept {
        return SameBytes(
                   stored.identity.volume_id,
                   supplied.volume_id,
                   sizeof(supplied.volume_id))
            && SameBytes(
                stored.identity.parent_object_id,
                supplied.parent_object_id,
                sizeof(supplied.parent_object_id))
            && stored.identity.parent_generation == supplied.parent_generation
            && SameBytes(
                stored.identity.object_id,
                supplied.object_id,
                sizeof(supplied.object_id))
            && stored.identity.object_generation == supplied.object_generation;
    }

    InkpodStatus ReopenTemporary(
        Temporary& temporary,
        ACCESS_MASK access,
        ULONG share,
        UniqueHandle& output) {
        ObservedPath parent;
        InkpodStatus status = ObserveAbsolute(
            temporary.parent_path, false, parent);
        if (status != INKPOD_STATUS_OK
            || !SameBytes(
                parent.path.record.object_id,
                temporary.identity.parent_object_id,
                sizeof(temporary.identity.parent_object_id))
            || parent.path.record.object_generation
                != temporary.identity.parent_generation) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        const NTSTATUS open_status = OpenRelative(
            parent.target.Get(),
            temporary.component,
            access | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            share,
            file_open_disposition,
            FILE_ATTRIBUTE_NORMAL,
            file_non_directory_option | file_synchronous_option,
            output);
        if (!IsSuccess(open_status)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        bool directory{};
        std::array<std::uint8_t, 16U> volume{};
        std::array<std::uint8_t, 32U> object{};
        if (IsReparse(output.Get(), directory) || directory
            || !QueryFileIdentity(output.Get(), volume, object)
            || !SameBytes(
                volume.data(),
                temporary.identity.volume_id,
                volume.size())
            || !SameBytes(
                object.data(),
                temporary.identity.object_id,
                object.size())) {
            output.Reset();
            return INKPOD_STATUS_INVALID_STATE;
        }
        return INKPOD_STATUS_OK;
    }

    InkpodStatus CleanupTemporary(Temporary& temporary) noexcept {
        UniqueHandle file;
        const InkpodStatus status = ReopenTemporary(
            temporary,
            DELETE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            file);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        FILE_DISPOSITION_INFO disposition{TRUE};
        if (SetFileInformationByHandle(
                file.Get(),
                FileDispositionInfo,
                &disposition,
                sizeof(disposition))
            == FALSE) {
            return INKPOD_STATUS_IO_ERROR;
        }
        temporary.handle.Reset();
        return INKPOD_STATUS_OK;
    }

    static InkpodStatus CALLBACK HostCall(
        void* context,
        const InkpodInkScriptHostRequest* request,
        InkpodInkScriptHostResponse* response) noexcept {
        if (context == nullptr || request == nullptr || response == nullptr) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        return static_cast<Impl*>(context)->Call(*request, *response);
    }

    InkpodStatus Call(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) noexcept {
        if (!OwnerThread()) {
            return INKPOD_STATUS_WRONG_THREAD;
        }
        if (request.struct_size < sizeof(request)
            || request.version != INKPOD_INKSCRIPT_RECORD_VERSION
            || request.feature_flags != 0U || response.struct_size < sizeof(response)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        try {
            ResetScratch();
            response = {};
            response.struct_size = sizeof(response);
            response.version = INKPOD_INKSCRIPT_RECORD_VERSION;
            switch (request.operation) {
                case INKPOD_INKSCRIPT_HOST_AUTHORITY_GENERATION:
                    response.generation = authority_generation;
                    return INKPOD_STATUS_OK;
                case INKPOD_INKSCRIPT_HOST_OPEN_SESSION_GENERATION:
                    response.generation = open_session_generation;
                    return INKPOD_STATUS_OK;
                case INKPOD_INKSCRIPT_HOST_OPEN_SESSIONS:
                    return CopyOpenSessions(response);
                case INKPOD_INKSCRIPT_HOST_RESOLVE_FILE:
                    return ResolveFile(request, response);
                case INKPOD_INKSCRIPT_HOST_ENUMERATE_FOLDER:
                    return EnumerateFolder(request, response);
                case INKPOD_INKSCRIPT_HOST_RESOLVE_DESTINATION:
                    return ResolveDestinationCall(request, response);
                case INKPOD_INKSCRIPT_HOST_ASSET_IDENTITY:
                    return AssetIdentity(request, response);
                case INKPOD_INKSCRIPT_HOST_ASSET_READ:
                    return AssetRead(request, response);
                case INKPOD_INKSCRIPT_HOST_READ_NATIVE:
                    return ReadNative(request, response);
                case INKPOD_INKSCRIPT_HOST_FINGERPRINT_NATIVE:
                    return FingerprintNative(request, response);
                case INKPOD_INKSCRIPT_HOST_ATOMIC_CAPABILITIES:
                    response.flags = INKPOD_INKSCRIPT_HOST_CAPABILITY_INSTALL
                        | INKPOD_INKSCRIPT_HOST_CAPABILITY_OVERWRITE;
                    return INKPOD_STATUS_OK;
                case INKPOD_INKSCRIPT_HOST_PREPARE_DESTINATION:
                    return PrepareDestination(request, response);
                case INKPOD_INKSCRIPT_HOST_REVALIDATE_DESTINATION:
                    return RevalidateDestination(request, response);
                case INKPOD_INKSCRIPT_HOST_CREATE_TEMPORARY:
                    return CreateTemporary(request, response);
                case INKPOD_INKSCRIPT_HOST_WRITE_CLOSE_TEMPORARY:
                    return WriteCloseTemporary(request, response);
                case INKPOD_INKSCRIPT_HOST_REVALIDATE_TEMPORARY:
                    return RevalidateTemporary(request, response);
                case INKPOD_INKSCRIPT_HOST_ACQUIRE_OVERWRITE_GUARD:
                    return AcquireGuard(request, response);
                case INKPOD_INKSCRIPT_HOST_FINGERPRINT_UNDER_GUARD:
                    return FingerprintUnderGuard(request, response);
                case INKPOD_INKSCRIPT_HOST_RELEASE_OVERWRITE_GUARD:
                    return ReleaseGuard(request);
                case INKPOD_INKSCRIPT_HOST_ATOMIC_INSTALL:
                    return AtomicInstall(request, response);
                case INKPOD_INKSCRIPT_HOST_CLEANUP_TEMPORARY:
                    return CleanupTemporaryCall(request);
                case INKPOD_INKSCRIPT_HOST_CURRENT_DOCUMENT:
                case INKPOD_INKSCRIPT_HOST_CURRENT_SEQUENCE:
                case INKPOD_INKSCRIPT_HOST_CAPTURE_OPEN_SESSION:
                case INKPOD_INKSCRIPT_HOST_SESSION_IS_CURRENT:
                    return INKPOD_STATUS_UNSUPPORTED;
                default:
                    return INKPOD_STATUS_UNSUPPORTED;
            }
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_IO_ERROR;
        } catch (...) {
            return INKPOD_STATUS_IO_ERROR;
        }
    }

    InkpodStatus CopyOpenSessions(InkpodInkScriptHostResponse& response) {
        scratch_open_sessions.reserve(open_sessions.size());
        for (const auto& [key, session] : open_sessions) {
            (void)key;
            InkpodInkScriptOpenSession record{};
            record.struct_size = sizeof(record);
            record.version = INKPOD_INKSCRIPT_RECORD_VERSION;
            record.session_id = session.session_id;
            record.session_generation = session.session_generation;
            record.document_uuid_low = session.uuid_low;
            record.document_uuid_high = session.uuid_high;
            record.backing_path = &session.path.record;
            scratch_open_sessions.push_back(record);
        }
        response.generation = open_session_generation;
        response.records = scratch_open_sessions.empty()
            ? nullptr
            : scratch_open_sessions.data();
        response.record_count = scratch_open_sessions.size();
        response.record_stride_bytes = sizeof(InkpodInkScriptOpenSession);
        return INKPOD_STATUS_OK;
    }

    InkpodStatus ResolveFile(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        OwnedFingerprint fingerprint;
        const InkpodStatus status = FingerprintGrant(request.intent_id, fingerprint);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        response.fingerprint = &KeepFingerprint(std::move(fingerprint))->record;
        return INKPOD_STATUS_OK;
    }

    InkpodStatus EnumerateFolder(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        const auto grant = grants.find(request.intent_id);
        if (grant == grants.end()
            || grant->second->access != INKPOD_INKSCRIPT_PATH_ENUMERATE
            || !grant->second->path.directory) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        WIN32_FIND_DATAW data{};
        HANDLE search = FindFirstFileW(
            (grant->second->path.absolute_path + L"\\*").c_str(), &data);
        if (search == INVALID_HANDLE_VALUE) {
            return INKPOD_STATUS_IO_ERROR;
        }
        std::vector<OwnedFingerprint> found;
        do {
            const std::wstring_view name{data.cFileName};
            if (name == L"." || name == L"..") {
                continue;
            }
            std::string normalized_name;
            if (!WideToUtf8(name, normalized_name)
                || normalized_name.size()
                    > (std::numeric_limits<std::uint64_t>::max)()
                    - response.normalized_name_bytes
                || response.observed_entries
                    == (std::numeric_limits<std::uint64_t>::max)()
                || response.work_units
                    == (std::numeric_limits<std::uint64_t>::max)()) {
                FindClose(search);
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            ++response.observed_entries;
            response.normalized_name_bytes += normalized_name.size();
            ++response.work_units;
            if ((data.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) != 0U) {
                FindClose(search);
                return INKPOD_STATUS_INVALID_STATE;
            }
            const std::size_t dot = name.find_last_of(L'.');
            std::wstring extension = dot == std::wstring_view::npos
                ? L""
                : std::wstring{name.substr(dot)};
            std::transform(
                extension.begin(),
                extension.end(),
                extension.begin(),
                [](wchar_t value) {
                    return value >= L'A' && value <= L'Z'
                        ? static_cast<wchar_t>(value - L'A' + L'a')
                        : value;
                });
            if (extension != L".inkpod") {
                continue;
            }
            ObservedPath observed;
            const InkpodStatus observe_status = ObserveAbsolute(
                grant->second->path.absolute_path + L"\\" + std::wstring{name},
                false,
                observed);
            OwnedFingerprint fingerprint;
            const InkpodStatus fingerprint_status = observe_status == INKPOD_STATUS_OK
                ? BuildFingerprint(std::move(observed), nullptr, fingerprint)
                : observe_status;
            if (fingerprint_status != INKPOD_STATUS_OK) {
                FindClose(search);
                return fingerprint_status;
            }
            found.push_back(std::move(fingerprint));
        } while (FindNextFileW(search, &data) != FALSE);
        const DWORD find_error = GetLastError();
        FindClose(search);
        if (find_error != ERROR_NO_MORE_FILES) {
            return INKPOD_STATUS_IO_ERROR;
        }
        response.maximum_depth = 1U;
        scratch_fingerprint_records.clear();
        scratch_fingerprint_records.reserve(found.size());
        for (auto& fingerprint : found) {
            OwnedFingerprint* kept = KeepFingerprint(std::move(fingerprint));
            scratch_fingerprint_records.push_back(kept->record);
        }
        response.records = scratch_fingerprint_records.empty()
            ? nullptr
            : scratch_fingerprint_records.data();
        response.record_count = scratch_fingerprint_records.size();
        response.record_stride_bytes = sizeof(InkpodInkScriptNativeFingerprint);
        return INKPOD_STATUS_OK;
    }

    InkpodStatus ResolveDestinationCall(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        OwnedPath path;
        const InkpodStatus status = ResolveDestination(request, path);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        response.identity = &KeepPath(std::move(path))->record;
        return INKPOD_STATUS_OK;
    }

    InkpodStatus AssetIdentity(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        std::string symbol;
        if (!ReadUtf8Span(request.asset_symbol, symbol)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        const auto asset = assets.find(symbol);
        if (asset == assets.end()) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        ObservedPath observed;
        const InkpodStatus status = ObserveAbsolute(
            asset->second.path.absolute_path, false, observed);
        if (status != INKPOD_STATUS_OK
            || !RecordMatches(asset->second.path.record, observed.path.record)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        LARGE_INTEGER size{};
        if (GetFileSizeEx(observed.target.Get(), &size) == FALSE || size.QuadPart < 0) {
            return INKPOD_STATUS_IO_ERROR;
        }
        const auto token = Token(
            observed.path.canonical_key,
            observed.path.record.object_generation);
        CopyArray(token, response.overwrite_guard);
        response.generation = observed.path.record.object_generation;
        response.byte_count = static_cast<std::uint64_t>(size.QuadPart);
        return INKPOD_STATUS_OK;
    }

    InkpodStatus AssetRead(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        std::string symbol;
        if (!ReadUtf8Span(request.asset_symbol, symbol)
            || request.byte_capacity > 16U * 1024U * 1024U) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        const auto asset = assets.find(symbol);
        if (asset == assets.end()) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        ObservedPath observed;
        const InkpodStatus status = ObserveAbsolute(
            asset->second.path.absolute_path, false, observed);
        if (status != INKPOD_STATUS_OK
            || !RecordMatches(asset->second.path.record, observed.path.record)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        LARGE_INTEGER offset{};
        if (request.byte_offset > static_cast<std::uint64_t>(LLONG_MAX)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        offset.QuadPart = static_cast<LONGLONG>(request.byte_offset);
        if (SetFilePointerEx(observed.target.Get(), offset, nullptr, FILE_BEGIN) == FALSE) {
            return INKPOD_STATUS_IO_ERROR;
        }
        scratch_bytes.resize(static_cast<std::size_t>(request.byte_capacity));
        DWORD read{};
        if (!scratch_bytes.empty()
            && ReadFile(
                   observed.target.Get(),
                   scratch_bytes.data(),
                   static_cast<DWORD>(scratch_bytes.size()),
                   &read,
                   nullptr)
                == FALSE) {
            return INKPOD_STATUS_IO_ERROR;
        }
        scratch_bytes.resize(read);
        response.bytes = scratch_bytes.empty() ? nullptr : scratch_bytes.data();
        response.byte_count = scratch_bytes.size();
        return INKPOD_STATUS_OK;
    }

    InkpodStatus ReadNative(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        ObservedPath observed;
        InkpodStatus status = ObserveKnown(request.fingerprint == nullptr
                ? nullptr
                : request.fingerprint->path,
            false,
            observed);
        if (status != INKPOD_STATUS_OK || request.fingerprint == nullptr
            || !RecordMatches(*request.fingerprint->path, observed.path.record)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        OwnedFingerprint fingerprint;
        status = BuildFingerprint(
            std::move(observed), &scratch_bytes, fingerprint);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        OwnedFingerprint second;
        ObservedPath after;
        status = ObserveKnown(request.fingerprint->path, false, after);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        status = BuildFingerprint(std::move(after), nullptr, second);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        response.fingerprint = &KeepFingerprint(std::move(fingerprint))->record;
        response.fingerprint_after = &KeepFingerprint(std::move(second))->record;
        response.bytes = scratch_bytes.data();
        response.byte_count = scratch_bytes.size();
        return INKPOD_STATUS_OK;
    }

    InkpodStatus FingerprintNative(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        if (request.fingerprint == nullptr) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        ObservedPath observed;
        InkpodStatus status = ObserveKnown(
            request.fingerprint->path, false, observed);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        OwnedFingerprint fingerprint;
        status = BuildFingerprint(std::move(observed), nullptr, fingerprint);
        if (status == INKPOD_STATUS_OK) {
            response.fingerprint = &KeepFingerprint(std::move(fingerprint))->record;
        }
        return status;
    }

    InkpodStatus PrepareDestination(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        if (request.identity == nullptr) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        const std::string key = PathKey(*request.identity);
        const auto location = locations.find(key);
        if (key.empty() || location == locations.end()) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        for (std::uint64_t index = 0; index < request.known_directory_count; ++index) {
            if (request.known_directories == nullptr) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            ObservedPath known;
            const InkpodStatus known_status = ObserveKnown(
                &request.known_directories[index], false, known);
            if (known_status != INKPOD_STATUS_OK
                || !RecordMatches(
                    request.known_directories[index], known.path.record)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
        }
        std::vector<OwnedPath> created;
        InkpodStatus status = EnsureParentDirectories(location->second, created);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        ObservedPath observed;
        status = ObserveAbsolute(location->second, true, observed);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        response.identity = &KeepPath(std::move(observed.path))->record;
        scratch_path_records.clear();
        scratch_path_records.reserve(created.size());
        for (auto& path : created) {
            scratch_path_records.push_back(KeepPath(std::move(path))->record);
        }
        response.records = scratch_path_records.empty()
            ? nullptr
            : scratch_path_records.data();
        response.record_count = scratch_path_records.size();
        response.record_stride_bytes = sizeof(InkpodInkScriptPathIdentity);
        return INKPOD_STATUS_OK;
    }

    InkpodStatus RevalidateDestination(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        ObservedPath observed;
        const InkpodStatus status = ObserveKnown(
            request.identity, true, observed);
        if (status != INKPOD_STATUS_OK
            || !RecordMatches(*request.identity, observed.path.record)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        response.identity = &KeepPath(std::move(observed.path))->record;
        return INKPOD_STATUS_OK;
    }

    InkpodStatus CreateTemporary(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        ObservedPath destination;
        InkpodStatus status = ObserveKnown(request.identity, true, destination);
        if (status != INKPOD_STATUS_OK
            || !RecordMatches(*request.identity, destination.path.record)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        std::wstring parent_path = destination.path.absolute_path;
        const std::size_t slash = parent_path.find_last_of(L'\\');
        if (slash == std::wstring::npos) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        parent_path.resize(slash == 2U ? 3U : slash);
        ObservedPath parent;
        status = ObserveAbsolute(parent_path, false, parent);
        if (status != INKPOD_STATUS_OK || !parent.path.directory
            || !SameBytes(
                parent.path.record.object_id,
                request.identity->parent_object_id,
                sizeof(request.identity->parent_object_id))
            || parent.path.record.object_generation
                != request.identity->parent_generation) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        for (std::uint32_t attempt = 0; attempt < 128U; ++attempt) {
            if (temporary_counter == UINT64_MAX) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            const std::uint64_t number = ++temporary_counter;
            wchar_t name[96]{};
            const int count = swprintf_s(
                name,
                L".~inkpod-%08lx-%016llx.tmp",
                static_cast<unsigned long>(GetCurrentProcessId()),
                static_cast<unsigned long long>(number));
            if (count <= 0) {
                return INKPOD_STATUS_IO_ERROR;
            }
            UniqueHandle file;
            const NTSTATUS create_status = OpenRelative(
                parent.target.Get(),
                std::wstring_view{name, static_cast<std::size_t>(count)},
                GENERIC_READ | GENERIC_WRITE | DELETE | FILE_READ_ATTRIBUTES
                    | SYNCHRONIZE,
                FILE_SHARE_READ | FILE_SHARE_DELETE,
                file_create_disposition,
                FILE_ATTRIBUTE_TEMPORARY,
                file_non_directory_option | file_synchronous_option,
                file);
            if (create_status == status_object_name_collision) {
                continue;
            }
            if (!IsSuccess(create_status)) {
                return INKPOD_STATUS_IO_ERROR;
            }
            bool directory{};
            std::array<std::uint8_t, 16U> volume{};
            std::array<std::uint8_t, 32U> object{};
            if (IsReparse(file.Get(), directory) || directory
                || !QueryFileIdentity(file.Get(), volume, object)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            Temporary temporary{};
            CopyArray(volume, temporary.identity.volume_id);
            std::copy_n(
                request.identity->parent_object_id,
                sizeof(request.identity->parent_object_id),
                temporary.identity.parent_object_id);
            temporary.identity.parent_generation =
                request.identity->parent_generation;
            CopyArray(object, temporary.identity.object_id);
            temporary.identity.object_generation = 1U;
            temporary.parent_path = parent_path;
            temporary.component.assign(name, static_cast<std::size_t>(count));
            temporary.handle = std::move(file);
            response.temporary = temporary.identity;
            temporaries.emplace(
                TemporaryObjectKey(temporary.identity), std::move(temporary));
            return INKPOD_STATUS_OK;
        }
        return INKPOD_STATUS_IO_ERROR;
    }

    InkpodStatus WriteCloseTemporary(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        const auto found = temporaries.find(TemporaryObjectKey(request.temporary));
        if (found == temporaries.end()
            || !TemporaryMatches(found->second, request.temporary)
            || found->second.closed
            || (request.byte_count != 0U && request.bytes == nullptr)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        Temporary& temporary = found->second;
        LARGE_INTEGER zero{};
        if (SetFilePointerEx(
                temporary.handle.Get(), zero, nullptr, FILE_BEGIN)
                == FALSE
            || SetEndOfFile(temporary.handle.Get()) == FALSE) {
            return INKPOD_STATUS_IO_ERROR;
        }
        std::uint64_t offset{};
        while (offset < request.byte_count) {
            const DWORD count = static_cast<DWORD>(std::min<std::uint64_t>(
                request.byte_count - offset, MAXDWORD));
            DWORD written{};
            if (WriteFile(
                    temporary.handle.Get(),
                    request.bytes + offset,
                    count,
                    &written,
                    nullptr)
                    == FALSE
                || written != count) {
                return INKPOD_STATUS_IO_ERROR;
            }
            offset += written;
        }
        if (FlushFileBuffers(temporary.handle.Get()) == FALSE) {
            return INKPOD_STATUS_IO_ERROR;
        }
        temporary.handle.Reset();
        temporary.closed = true;
        response.temporary = temporary.identity;
        return INKPOD_STATUS_OK;
    }

    InkpodStatus RevalidateTemporary(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        const auto found = temporaries.find(TemporaryObjectKey(request.temporary));
        if (found == temporaries.end()
            || !TemporaryMatches(found->second, request.temporary)
            || !found->second.closed) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        UniqueHandle file;
        const InkpodStatus status = ReopenTemporary(
            found->second,
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_DELETE,
            file);
        if (status == INKPOD_STATUS_OK) {
            response.temporary = found->second.identity;
        }
        return status;
    }

    using GuardKey = std::array<std::uint8_t, 32U>;

    static GuardKey GuardToken(const std::uint8_t (&token)[32U]) noexcept {
        GuardKey result{};
        std::copy_n(token, result.size(), result.begin());
        return result;
    }

    InkpodStatus AcquireGuard(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        if (request.fingerprint == nullptr || request.fingerprint->path == nullptr) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        ObservedPath observed;
        InkpodStatus status = ObserveKnown(
            request.fingerprint->path, false, observed);
        if (status != INKPOD_STATUS_OK
            || !RecordMatches(*request.fingerprint->path, observed.path.record)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        // Reopen without FILE_SHARE_WRITE. Windows checks this against existing
        // writers and prevents new writers for the guard lifetime.
        UniqueHandle guarded{CreateFileW(
            observed.path.absolute_path.c_str(),
            GENERIC_READ | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ,
            nullptr,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            nullptr)};
        bool directory{};
        std::array<std::uint8_t, 16U> guarded_volume{};
        std::array<std::uint8_t, 32U> guarded_object{};
        if (!guarded || IsReparse(guarded.Get(), directory) || directory
            || !QueryFileIdentity(
                guarded.Get(), guarded_volume, guarded_object)
            || !SameBytes(
                guarded_volume.data(),
                observed.path.record.volume_id,
                guarded_volume.size())
            || !SameBytes(
                guarded_object.data(),
                observed.path.record.object_id,
                guarded_object.size())) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        if (guard_counter == UINT64_MAX) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        const std::uint64_t next_guard_counter = guard_counter + 1U;
        const std::array<std::uint8_t, 32U> token = Token(
            observed.path.canonical_key, next_guard_counter);
        Guard guard{};
        guard.token = token;
        guard.path = std::move(observed.path);
        guard.handle = std::move(guarded);
        const auto [guard_entry, inserted] = guards.emplace(
            token, std::move(guard));
        (void)guard_entry;
        if (!inserted) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        guard_counter = next_guard_counter;
        CopyArray(token, response.overwrite_guard);
        return INKPOD_STATUS_OK;
    }

    InkpodStatus FingerprintUnderGuard(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        const auto found = guards.find(GuardToken(request.overwrite_guard));
        if (found == guards.end() || request.fingerprint == nullptr
            || request.fingerprint->path == nullptr
            || !RecordMatches(
                *request.fingerprint->path, found->second.path.record)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        ObservedPath observed;
        const InkpodStatus observe_status = ObserveKnown(
            request.fingerprint->path, false, observed);
        if (observe_status != INKPOD_STATUS_OK
            || !RecordMatches(found->second.path.record, observed.path.record)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        OwnedFingerprint fingerprint;
        const InkpodStatus status = BuildFingerprint(
            std::move(observed), nullptr, fingerprint);
        if (status == INKPOD_STATUS_OK) {
            response.fingerprint = &KeepFingerprint(std::move(fingerprint))->record;
        }
        return status;
    }

    InkpodStatus ReleaseGuard(const InkpodInkScriptHostRequest& request) {
        const auto found = guards.find(GuardToken(request.overwrite_guard));
        if (found == guards.end()) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        guards.erase(found);
        return INKPOD_STATUS_OK;
    }

    InkpodStatus AtomicInstall(
        const InkpodInkScriptHostRequest& request,
        InkpodInkScriptHostResponse& response) {
        const auto temporary_found = temporaries.find(
            TemporaryObjectKey(request.temporary));
        if (temporary_found == temporaries.end()
            || !TemporaryMatches(temporary_found->second, request.temporary)
            || !temporary_found->second.closed || request.identity == nullptr) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        Temporary& temporary = temporary_found->second;
        ObservedPath destination;
        InkpodStatus status = ObserveKnown(request.identity, true, destination);
        if (status != INKPOD_STATUS_OK
            || !RecordMatches(*request.identity, destination.path.record)
            || !SameBytes(
                request.temporary.volume_id,
                request.identity->volume_id,
                sizeof(request.temporary.volume_id))
            || !SameBytes(
                request.temporary.parent_object_id,
                request.identity->parent_object_id,
                sizeof(request.temporary.parent_object_id))
            || request.temporary.parent_generation
                != request.identity->parent_generation) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        auto guard = guards.end();
        const bool overwrite =
            (request.flags & INKPOD_INKSCRIPT_HOST_HAS_OVERWRITE_GUARD) != 0U;
        if (request.flags & ~INKPOD_INKSCRIPT_HOST_HAS_OVERWRITE_GUARD) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        if (overwrite) {
            guard = guards.find(GuardToken(request.overwrite_guard));
            if (guard == guards.end()
                || !SameObject(guard->second.path.record, destination.path.record)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
        } else if ((destination.path.record.flags
                    & INKPOD_INKSCRIPT_PATH_EXPECTED_ABSENT)
            == 0U) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        UniqueHandle temporary_file;
        status = ReopenTemporary(
            temporary,
            DELETE,
            FILE_SHARE_READ | FILE_SHARE_DELETE,
            temporary_file);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        if (guard != guards.end()) {
            guard->second.handle.Reset();
            ObservedPath guarded_destination;
            status = ObserveKnown(request.identity, false, guarded_destination);
            if (status != INKPOD_STATUS_OK
                || !RecordMatches(
                    *request.identity, guarded_destination.path.record)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
        }
        destination.target.Reset();
        destination.parent.Reset();
        UniqueHandle rename_parent{CreateFileW(
            temporary.parent_path.c_str(),
            FILE_LIST_DIRECTORY | FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY
                | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            nullptr,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            nullptr)};
        bool rename_parent_directory{};
        std::array<std::uint8_t, 16U> rename_parent_volume{};
        std::array<std::uint8_t, 32U> rename_parent_object{};
        if (!rename_parent
            || IsReparse(rename_parent.Get(), rename_parent_directory)
            || !rename_parent_directory
            || !QueryFileIdentity(
                rename_parent.Get(),
                rename_parent_volume,
                rename_parent_object)
            || !SameBytes(
                rename_parent_object.data(),
                request.identity->parent_object_id,
                rename_parent_object.size())) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        const std::wstring& destination_path = destination.path.absolute_path;
        const std::size_t slash = destination_path.find_last_of(L'\\');
        if (slash == std::wstring::npos) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        const std::wstring_view component{
            destination_path.data() + slash + 1U,
            destination_path.size() - slash - 1U};
        const std::size_t allocation = offsetof(FILE_RENAME_INFO, FileName)
            + component.size() * sizeof(wchar_t);
        std::vector<std::uint8_t> storage(allocation);
        auto* rename = reinterpret_cast<FILE_RENAME_INFO*>(storage.data());
        rename->ReplaceIfExists = overwrite ? TRUE : FALSE;
        rename->RootDirectory = rename_parent.Get();
        rename->FileNameLength = static_cast<DWORD>(
            component.size() * sizeof(wchar_t));
        std::memcpy(
            rename->FileName,
            component.data(),
            component.size() * sizeof(wchar_t));
        const NtSetInformationFileFunction set_information =
            ResolveNtSetInformationFile();
        if (set_information == nullptr) {
            return INKPOD_STATUS_UNSUPPORTED;
        }
        IO_STATUS_BLOCK rename_status_block{};
        const NTSTATUS rename_status = set_information(
            temporary_file.Get(),
            &rename_status_block,
            rename,
            static_cast<ULONG>(allocation),
            static_cast<FILE_INFORMATION_CLASS>(10));
        if (!IsSuccess(rename_status)) {
            return INKPOD_STATUS_IO_ERROR;
        }
        temporary.handle.Reset();
        temporaries.erase(temporary_found);
        if (guard != guards.end()) {
            guards.erase(guard);
        }
        response.result_kind = 1U;
        return INKPOD_STATUS_OK;
    }

    InkpodStatus CleanupTemporaryCall(
        const InkpodInkScriptHostRequest& request) {
        const auto found = temporaries.find(TemporaryObjectKey(request.temporary));
        if (found == temporaries.end()
            || !TemporaryMatches(found->second, request.temporary)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        const InkpodStatus status = CleanupTemporary(found->second);
        if (status == INKPOD_STATUS_OK) {
            temporaries.erase(found);
        }
        return status;
    }

    DWORD owner_thread{};
    std::uint64_t authority_generation{1U};
    std::uint64_t open_session_generation{1U};
    std::uint64_t temporary_counter{};
    std::uint64_t guard_counter{};
    std::map<std::uint64_t, std::unique_ptr<Grant>> grants;
    std::map<std::string, Asset> assets;
    std::map<std::pair<std::uint64_t, std::uint64_t>, OpenSession> open_sessions;
    std::map<std::string, std::wstring> locations;
    std::map<TemporaryKey, Temporary> temporaries;
    std::map<GuardKey, Guard> guards;
    std::vector<std::unique_ptr<OwnedPath>> scratch_paths;
    std::vector<std::unique_ptr<OwnedFingerprint>> scratch_fingerprints;
    std::vector<InkpodInkScriptPathIdentity> scratch_path_records;
    std::vector<InkpodInkScriptNativeFingerprint> scratch_fingerprint_records;
    std::vector<InkpodInkScriptOpenSession> scratch_open_sessions;
    std::vector<std::uint8_t> scratch_bytes;
};

InkScriptFileAuthorityAdapter::InkScriptFileAuthorityAdapter() noexcept
    : impl_(new (std::nothrow) Impl{}) {}

InkScriptFileAuthorityAdapter::~InkScriptFileAuthorityAdapter() = default;

InkpodInkScriptHostAdapter
InkScriptFileAuthorityAdapter::HostAdapterRecord() noexcept {
    return InkpodInkScriptHostAdapter{
        sizeof(InkpodInkScriptHostAdapter),
        INKPOD_INKSCRIPT_RECORD_VERSION,
        INKPOD_FEATURE_NONE,
        impl_.get(),
        impl_ == nullptr ? nullptr : &Impl::HostCall};
}

InkpodStatus InkScriptFileAuthorityAdapter::AuthorizePath(
    std::uint64_t intent_id,
    std::uint32_t access,
    const std::wstring& path,
    InkpodInkScriptAuthorityGrant& output) noexcept {
    output = {};
    if (impl_ == nullptr || !impl_->OwnerThread()) {
        return impl_ == nullptr ? INKPOD_STATUS_IO_ERROR : INKPOD_STATUS_WRONG_THREAD;
    }
    if (intent_id == 0U
        || (access != INKPOD_INKSCRIPT_PATH_READ
            && access != INKPOD_INKSCRIPT_PATH_ENUMERATE
            && access != INKPOD_INKSCRIPT_PATH_CREATE
            && access != INKPOD_INKSCRIPT_PATH_REPLACE)
        || impl_->grants.contains(intent_id)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (impl_->authority_generation == UINT64_MAX) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        ObservedPath observed;
        const InkpodStatus status = ObserveAbsolute(path, false, observed);
        if (status != INKPOD_STATUS_OK
            || (access == INKPOD_INKSCRIPT_PATH_ENUMERATE
                && !observed.path.directory)
            || ((access == INKPOD_INKSCRIPT_PATH_CREATE
                    || access == INKPOD_INKSCRIPT_PATH_REPLACE)
                && !observed.path.directory)) {
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_ARGUMENT
                : status;
        }
        auto grant = std::make_unique<Impl::Grant>();
        grant->access = access;
        grant->path = std::move(observed.path);
        const std::uint64_t next_generation = impl_->authority_generation + 1U;
        grant->authority_id = Token(grant->path.canonical_key, next_generation);
        const auto [grant_entry, inserted] = impl_->grants.emplace(
            intent_id, std::move(grant));
        if (!inserted) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        try {
            impl_->locations.try_emplace(
                grant_entry->second->path.canonical_key,
                grant_entry->second->path.absolute_path);
        } catch (...) {
            impl_->grants.erase(grant_entry);
            throw;
        }
        impl_->authority_generation = next_generation;
        Impl::Grant* retained = grant_entry->second.get();
        output.struct_size = sizeof(output);
        output.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        output.access = access;
        output.intent_id = intent_id;
        CopyArray(retained->authority_id, output.authority_id);
        output.authority_generation = impl_->authority_generation;
        output.resolved = &retained->path.record;
        return INKPOD_STATUS_OK;
    } catch (...) {
        return INKPOD_STATUS_IO_ERROR;
    }
}

InkpodStatus InkScriptFileAuthorityAdapter::RevokePathAuthority(
    std::uint64_t intent_id) noexcept {
    if (impl_ == nullptr || !impl_->OwnerThread()) {
        return impl_ == nullptr ? INKPOD_STATUS_IO_ERROR : INKPOD_STATUS_WRONG_THREAD;
    }
    const auto found = impl_->grants.find(intent_id);
    if (found == impl_->grants.end()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodStatus status = impl_->AdvanceGeneration(
        impl_->authority_generation);
    if (status == INKPOD_STATUS_OK) {
        impl_->grants.erase(found);
    }
    return status;
}

InkpodStatus InkScriptFileAuthorityAdapter::AuthorizeAsset(
    const std::string& symbol,
    const std::wstring& path) noexcept {
    if (impl_ == nullptr || !impl_->OwnerThread()) {
        return impl_ == nullptr ? INKPOD_STATUS_IO_ERROR : INKPOD_STATUS_WRONG_THREAD;
    }
    if (symbol.empty() || symbol.find('\0') != std::string::npos
        || impl_->assets.contains(symbol)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (impl_->authority_generation == UINT64_MAX) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        ObservedPath observed;
        const InkpodStatus status = ObserveAbsolute(path, false, observed);
        if (status != INKPOD_STATUS_OK || observed.path.directory) {
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_ARGUMENT
                : status;
        }
        Impl::Asset asset{};
        asset.path = std::move(observed.path);
        const std::uint64_t next_generation = impl_->authority_generation + 1U;
        const auto [asset_entry, inserted] = impl_->assets.emplace(
            symbol, std::move(asset));
        if (!inserted) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        try {
            impl_->locations.try_emplace(
                asset_entry->second.path.canonical_key,
                asset_entry->second.path.absolute_path);
        } catch (...) {
            impl_->assets.erase(asset_entry);
            throw;
        }
        impl_->authority_generation = next_generation;
        return INKPOD_STATUS_OK;
    } catch (...) {
        return INKPOD_STATUS_IO_ERROR;
    }
}

InkpodStatus InkScriptFileAuthorityAdapter::RegisterOpenSession(
    std::uint64_t session_id,
    std::uint64_t session_generation,
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    const std::wstring& backing_path) noexcept {
    if (impl_ == nullptr || !impl_->OwnerThread()) {
        return impl_ == nullptr ? INKPOD_STATUS_IO_ERROR : INKPOD_STATUS_WRONG_THREAD;
    }
    const auto key = std::make_pair(session_id, session_generation);
    if (session_id == 0U || session_generation == 0U
        || (document_uuid_high == 0U && document_uuid_low == 0U)
        || impl_->open_sessions.contains(key)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (impl_->open_session_generation == UINT64_MAX) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        ObservedPath observed;
        const InkpodStatus status = ObserveAbsolute(
            backing_path, false, observed);
        if (status != INKPOD_STATUS_OK || observed.path.directory) {
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_ARGUMENT
                : status;
        }
        Impl::OpenSession session{};
        session.session_id = session_id;
        session.session_generation = session_generation;
        session.uuid_high = document_uuid_high;
        session.uuid_low = document_uuid_low;
        session.path = std::move(observed.path);
        const std::uint64_t next_generation =
            impl_->open_session_generation + 1U;
        const auto [session_entry, inserted] = impl_->open_sessions.emplace(
            key, std::move(session));
        if (!inserted) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        try {
            impl_->locations.try_emplace(
                session_entry->second.path.canonical_key,
                session_entry->second.path.absolute_path);
        } catch (...) {
            impl_->open_sessions.erase(session_entry);
            throw;
        }
        impl_->open_session_generation = next_generation;
        return INKPOD_STATUS_OK;
    } catch (...) {
        return INKPOD_STATUS_IO_ERROR;
    }
}

InkpodStatus InkScriptFileAuthorityAdapter::UnregisterOpenSession(
    std::uint64_t session_id,
    std::uint64_t session_generation) noexcept {
    if (impl_ == nullptr || !impl_->OwnerThread()) {
        return impl_ == nullptr ? INKPOD_STATUS_IO_ERROR : INKPOD_STATUS_WRONG_THREAD;
    }
    const auto found = impl_->open_sessions.find(
        {session_id, session_generation});
    if (found == impl_->open_sessions.end()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodStatus status = impl_->AdvanceGeneration(
        impl_->open_session_generation);
    if (status == INKPOD_STATUS_OK) {
        impl_->open_sessions.erase(found);
    }
    return status;
}

}  // namespace inkpod::app
