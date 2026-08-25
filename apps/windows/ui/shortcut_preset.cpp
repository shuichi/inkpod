#include "shortcut_preset.h"

#include <algorithm>
#include <array>
#include <atomic>
#include <limits>
#include <new>
#include <string>

#include "command_catalog.h"

namespace inkpod::windows::ui {
namespace {

constexpr std::array<std::uint8_t, 8U> kMagic{
    'I', 'N', 'K', 'K', 'E', 'Y', '1', 0};
constexpr std::uint32_t kVersion = 1U;
constexpr std::size_t kMaximumPresetBytes = 2U * 1024U * 1024U;
constexpr std::uint32_t kFixedBindingBytes = 80U;
std::atomic<std::uint32_t> g_temporary_sequence{1U};

void PushU32(std::vector<std::uint8_t>& output, std::uint32_t value) {
    output.push_back(static_cast<std::uint8_t>(value));
    output.push_back(static_cast<std::uint8_t>(value >> 8U));
    output.push_back(static_cast<std::uint8_t>(value >> 16U));
    output.push_back(static_cast<std::uint8_t>(value >> 24U));
}

class Reader final {
public:
    explicit Reader(std::span<const std::uint8_t> bytes) noexcept : bytes_(bytes) {}

    bool U32(std::uint32_t& value) noexcept {
        if (cursor_ > bytes_.size() || bytes_.size() - cursor_ < 4U) {
            return false;
        }
        value = static_cast<std::uint32_t>(bytes_[cursor_])
            | (static_cast<std::uint32_t>(bytes_[cursor_ + 1U]) << 8U)
            | (static_cast<std::uint32_t>(bytes_[cursor_ + 2U]) << 16U)
            | (static_cast<std::uint32_t>(bytes_[cursor_ + 3U]) << 24U);
        cursor_ += 4U;
        return true;
    }

    bool Bytes(std::size_t count, std::span<const std::uint8_t>& output) noexcept {
        if (cursor_ > bytes_.size() || count > bytes_.size() - cursor_) {
            return false;
        }
        output = bytes_.subspan(cursor_, count);
        cursor_ += count;
        return true;
    }

    [[nodiscard]] bool Empty() const noexcept { return cursor_ == bytes_.size(); }

private:
    std::span<const std::uint8_t> bytes_;
    std::size_t cursor_{};
};

bool WideToUtf8(std::wstring_view input, std::string& output) noexcept {
    if (input.empty()) {
        return false;
    }
    const int required = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        input.data(),
        static_cast<int>(input.size()),
        nullptr,
        0,
        nullptr,
        nullptr);
    if (required <= 0) {
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(required));
    } catch (const std::bad_alloc&) {
        return false;
    }
    return WideCharToMultiByte(
               CP_UTF8,
               WC_ERR_INVALID_CHARS,
               input.data(),
               static_cast<int>(input.size()),
               output.data(),
               required,
               nullptr,
               nullptr)
        == required;
}

bool Utf8ToWide(std::span<const std::uint8_t> input, std::wstring& output) noexcept {
    if (input.empty() || input.size() > static_cast<std::size_t>(std::numeric_limits<int>::max())) {
        return false;
    }
    const auto* text = reinterpret_cast<const char*>(input.data());
    const int required = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        text,
        static_cast<int>(input.size()),
        nullptr,
        0);
    if (required <= 0) {
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(required));
    } catch (const std::bad_alloc&) {
        return false;
    }
    return MultiByteToWideChar(
               CP_UTF8,
               MB_ERR_INVALID_CHARS,
               text,
               static_cast<int>(input.size()),
               output.data(),
               required)
        == required;
}

bool WriteAll(HANDLE file, std::span<const std::uint8_t> bytes) noexcept {
    std::size_t cursor{};
    while (cursor < bytes.size()) {
        const DWORD chunk = static_cast<DWORD>(std::min<std::size_t>(
            bytes.size() - cursor, std::numeric_limits<DWORD>::max()));
        DWORD written{};
        if (WriteFile(file, bytes.data() + cursor, chunk, &written, nullptr) == FALSE
            || written != chunk) {
            return false;
        }
        cursor += written;
    }
    return true;
}

std::wstring TemporaryPath(const wchar_t* path, std::uint32_t sequence) {
    std::wstring result(path);
    result += L".tmp.";
    result += std::to_wstring(GetCurrentProcessId());
    result += L'.';
    result += std::to_wstring(sequence);
    return result;
}

}  // namespace

ShortcutPresetStatus EncodeShortcutPreset(
    const ShortcutProfile& profile,
    std::vector<std::uint8_t>& output) noexcept {
    if (ValidateShortcutProfile(profile, false) != ShortcutProfileValidation::Ok) {
        return ShortcutPresetStatus::Invalid;
    }
    std::string name;
    if (!WideToUtf8(profile.name, name)
        || name.size() > kMaximumShortcutProfileNameLength * 4U
        || profile.bindings.size() > kMaximumShortcutProfileBindings) {
        return ShortcutPresetStatus::Invalid;
    }
    try {
        std::vector<std::uint8_t> encoded;
        encoded.reserve(32U + name.size() + profile.bindings.size() * 96U);
        encoded.insert(encoded.end(), kMagic.begin(), kMagic.end());
        PushU32(encoded, kVersion);
        PushU32(encoded, static_cast<std::uint32_t>(name.size()));
        PushU32(encoded, static_cast<std::uint32_t>(profile.bindings.size()));
        PushU32(encoded, 0U);
        encoded.insert(encoded.end(), name.begin(), name.end());
        for (const auto& binding : profile.bindings) {
            const std::string key = CommandStableKey(binding.command_id);
            if (key.empty() || key.size() > 1'024U) {
                return ShortcutPresetStatus::Invalid;
            }
            PushU32(encoded, kFixedBindingBytes + static_cast<std::uint32_t>(key.size()));
            PushU32(encoded, static_cast<std::uint32_t>(key.size()));
            PushU32(encoded, static_cast<std::uint32_t>(binding.slot));
            PushU32(encoded, static_cast<std::uint32_t>(binding.context));
            PushU32(encoded, static_cast<std::uint32_t>(binding.action));
            PushU32(encoded, static_cast<std::uint32_t>(binding.key_match));
            PushU32(encoded, binding.stroke_count);
            PushU32(encoded, 0U);
            for (const auto& stroke : binding.strokes) {
                PushU32(encoded, stroke.logical_key);
                PushU32(encoded, stroke.physical_key);
                PushU32(encoded, stroke.modifiers);
            }
            encoded.insert(encoded.end(), key.begin(), key.end());
        }
        if (encoded.size() > kMaximumPresetBytes) {
            return ShortcutPresetStatus::CapacityExceeded;
        }
        output = std::move(encoded);
        return ShortcutPresetStatus::Ok;
    } catch (const std::bad_alloc&) {
        return ShortcutPresetStatus::CapacityExceeded;
    }
}

ShortcutPresetStatus DecodeShortcutPreset(
    std::span<const std::uint8_t> bytes,
    ShortcutProfile& output) noexcept {
    if (bytes.size() > kMaximumPresetBytes || bytes.size() < 24U) {
        return ShortcutPresetStatus::CapacityExceeded;
    }
    if (!std::equal(kMagic.begin(), kMagic.end(), bytes.begin())) {
        return ShortcutPresetStatus::UnsupportedVersion;
    }
    Reader reader(bytes.subspan(kMagic.size()));
    std::uint32_t version{};
    std::uint32_t name_size{};
    std::uint32_t binding_count{};
    std::uint32_t reserved{};
    if (!reader.U32(version) || !reader.U32(name_size) || !reader.U32(binding_count)
        || !reader.U32(reserved) || version != kVersion || reserved != 0U) {
        return version != kVersion
            ? ShortcutPresetStatus::UnsupportedVersion
            : ShortcutPresetStatus::Invalid;
    }
    if (name_size == 0U || name_size > kMaximumShortcutProfileNameLength * 4U
        || binding_count > kMaximumShortcutProfileBindings) {
        return ShortcutPresetStatus::CapacityExceeded;
    }
    std::span<const std::uint8_t> name_bytes;
    ShortcutProfile decoded{};
    if (!reader.Bytes(name_size, name_bytes) || !Utf8ToWide(name_bytes, decoded.name)
        || decoded.name.size() > kMaximumShortcutProfileNameLength) {
        return ShortcutPresetStatus::Invalid;
    }
    decoded.built_in = false;
    try {
        decoded.bindings.reserve(binding_count);
    } catch (const std::bad_alloc&) {
        return ShortcutPresetStatus::CapacityExceeded;
    }
    for (std::uint32_t binding_index = 0U; binding_index < binding_count; ++binding_index) {
        std::uint32_t record_size{};
        std::uint32_t key_size{};
        std::uint32_t slot{};
        std::uint32_t context{};
        std::uint32_t action{};
        std::uint32_t key_match{};
        std::uint32_t stroke_count{};
        if (!reader.U32(record_size) || !reader.U32(key_size) || !reader.U32(slot)
            || !reader.U32(context) || !reader.U32(action) || !reader.U32(key_match)
            || !reader.U32(stroke_count) || !reader.U32(reserved)
            || key_size == 0U || key_size > 1'024U
            || record_size != kFixedBindingBytes + key_size || reserved != 0U) {
            return ShortcutPresetStatus::Invalid;
        }
        ShortcutProfileBinding binding{};
        binding.slot = static_cast<ShortcutSlot>(slot);
        binding.context = static_cast<ShortcutContext>(context);
        binding.action = static_cast<ShortcutAction>(action);
        binding.key_match = static_cast<ShortcutKeyMatch>(key_match);
        binding.stroke_count = stroke_count;
        for (auto& stroke : binding.strokes) {
            if (!reader.U32(stroke.logical_key) || !reader.U32(stroke.physical_key)
                || !reader.U32(stroke.modifiers)) {
                return ShortcutPresetStatus::Invalid;
            }
        }
        std::span<const std::uint8_t> key_bytes;
        if (!reader.Bytes(key_size, key_bytes)) {
            return ShortcutPresetStatus::Invalid;
        }
        const std::string_view key(
            reinterpret_cast<const char*>(key_bytes.data()), key_bytes.size());
        binding.command_id = CommandFromStableKey(key);
        if (binding.command_id == 0U) {
            return ShortcutPresetStatus::Invalid;
        }
        try {
            decoded.bindings.push_back(binding);
        } catch (const std::bad_alloc&) {
            return ShortcutPresetStatus::CapacityExceeded;
        }
    }
    if (!reader.Empty()
        || ValidateShortcutProfile(decoded, false) != ShortcutProfileValidation::Ok) {
        return ShortcutPresetStatus::Invalid;
    }
    output = std::move(decoded);
    return ShortcutPresetStatus::Ok;
}

ShortcutPresetStatus ReadShortcutPreset(
    const wchar_t* path,
    ShortcutProfile& output) noexcept {
    if (path == nullptr || path[0] == L'\0') {
        return ShortcutPresetStatus::Invalid;
    }
    HANDLE file = CreateFileW(
        path,
        GENERIC_READ,
        FILE_SHARE_READ,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return ShortcutPresetStatus::IoError;
    }
    LARGE_INTEGER size{};
    if (GetFileSizeEx(file, &size) == FALSE || size.QuadPart < 0
        || size.QuadPart > static_cast<LONGLONG>(kMaximumPresetBytes)) {
        CloseHandle(file);
        return ShortcutPresetStatus::CapacityExceeded;
    }
    std::vector<std::uint8_t> bytes;
    try {
        bytes.resize(static_cast<std::size_t>(size.QuadPart));
    } catch (const std::bad_alloc&) {
        CloseHandle(file);
        return ShortcutPresetStatus::CapacityExceeded;
    }
    DWORD read{};
    const bool ok = bytes.size() <= std::numeric_limits<DWORD>::max()
        && ReadFile(file, bytes.data(), static_cast<DWORD>(bytes.size()), &read, nullptr) != FALSE
        && static_cast<std::size_t>(read) == bytes.size();
    CloseHandle(file);
    return ok ? DecodeShortcutPreset(bytes, output) : ShortcutPresetStatus::IoError;
}

ShortcutPresetStatus SaveShortcutPresetAtomic(
    const wchar_t* path,
    const ShortcutProfile& profile) noexcept {
    if (path == nullptr || path[0] == L'\0') {
        return ShortcutPresetStatus::Invalid;
    }
    std::vector<std::uint8_t> bytes;
    const ShortcutPresetStatus encode = EncodeShortcutPreset(profile, bytes);
    if (encode != ShortcutPresetStatus::Ok) {
        return encode;
    }
    std::wstring temporary;
    HANDLE file = INVALID_HANDLE_VALUE;
    try {
        for (std::uint32_t attempt = 0U; attempt < 32U; ++attempt) {
            temporary = TemporaryPath(path, g_temporary_sequence.fetch_add(1U));
            file = CreateFileW(
                temporary.c_str(),
                GENERIC_WRITE,
                0U,
                nullptr,
                CREATE_NEW,
                FILE_ATTRIBUTE_TEMPORARY,
                nullptr);
            if (file != INVALID_HANDLE_VALUE || GetLastError() != ERROR_FILE_EXISTS) {
                break;
            }
        }
    } catch (const std::bad_alloc&) {
        return ShortcutPresetStatus::CapacityExceeded;
    }
    if (file == INVALID_HANDLE_VALUE) {
        return ShortcutPresetStatus::IoError;
    }
    const bool written = WriteAll(file, bytes) && FlushFileBuffers(file) != FALSE;
    const bool closed = CloseHandle(file) != FALSE;
    if (!written || !closed
        || MoveFileExW(
               temporary.c_str(),
               path,
               MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
            == FALSE) {
        DeleteFileW(temporary.c_str());
        return ShortcutPresetStatus::IoError;
    }
    return ShortcutPresetStatus::Ok;
}

}  // namespace inkpod::windows::ui
