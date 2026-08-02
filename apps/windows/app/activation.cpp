#include "activation.h"

#include <sddl.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <deque>
#include <limits>
#include <mutex>
#include <new>
#include <optional>
#include <system_error>
#include <thread>
#include <utility>

namespace inkpod::app {
namespace {

constexpr std::uint32_t kRequestMagic = UINT32_C(0x41504b49);
constexpr std::uint32_t kReplyMagic = UINT32_C(0x52504b49);
constexpr std::uint16_t kProtocolVersion = 1U;
constexpr std::uint16_t kRequestHeaderBytes = 36U;
constexpr std::uint16_t kReplyBytes = 28U;
constexpr std::size_t kMaximumQueuedActivations = 64U;
constexpr std::size_t kRememberedRequestCount = 256U;
constexpr DWORD kPipeBufferBytes =
    static_cast<DWORD>(kMaximumActivationMessageBytes + 1U);

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
        | static_cast<std::uint16_t>(bytes[cursor + 1U] << 8U);
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

bool WideToUtf8(std::wstring_view text, std::vector<std::uint8_t>& output) {
    if (text.empty() || text.size() > 32767U
        || std::find(text.begin(), text.end(), L'\0') != text.end()) {
        return false;
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
    if (required <= 0 || static_cast<std::size_t>(required)
            > kMaximumActivationMessageBytes) {
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
    if (bytes == nullptr || length == 0U
        || length > kMaximumActivationMessageBytes
        || std::find(bytes, bytes + length, std::uint8_t{0U}) != bytes + length
        || length > static_cast<std::size_t>(std::numeric_limits<int>::max())) {
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

bool ValidRequestEnums(const ActivationRequest& request) noexcept {
    return request.open_mode == ActivationOpenMode::Documents
        && (request.target == ActivationTargetPreference::LastFocusedWorkspace
            || request.target == ActivationTargetPreference::NewWorkspace);
}

bool ValidReplyStatus(ActivationReplyStatus status) noexcept {
    return status == ActivationReplyStatus::Accepted
        || status == ActivationReplyStatus::Duplicate
        || status == ActivationReplyStatus::InvalidRequest
        || status == ActivationReplyStatus::QueueFull
        || status == ActivationReplyStatus::ShuttingDown;
}

bool SafeTestSuffix(std::wstring_view suffix) noexcept {
    return suffix.size() <= 64U
        && std::all_of(suffix.begin(), suffix.end(), [](wchar_t character) {
               return (character >= L'a' && character <= L'z')
                   || (character >= L'A' && character <= L'Z')
                   || (character >= L'0' && character <= L'9')
                   || character == L'-' || character == L'_';
           });
}

struct SecurityState final {
    HANDLE token{};
    PSID sid{};
    PSECURITY_DESCRIPTOR descriptor{};
    SECURITY_ATTRIBUTES attributes{};
    std::wstring sid_text;

    ~SecurityState() {
        if (descriptor != nullptr) {
            LocalFree(descriptor);
        }
        if (sid != nullptr) {
            LocalFree(sid);
        }
        if (token != nullptr) {
            CloseHandle(token);
        }
    }
};

bool BuildSecurity(SecurityState& state) noexcept {
    if (OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &state.token) == FALSE) {
        return false;
    }
    DWORD required{};
    (void)GetTokenInformation(state.token, TokenUser, nullptr, 0U, &required);
    if (required == 0U) {
        return false;
    }
    HLOCAL storage = LocalAlloc(LPTR, required);
    if (storage == nullptr) {
        return false;
    }
    if (GetTokenInformation(
            state.token, TokenUser, storage, required, &required) == FALSE) {
        LocalFree(storage);
        return false;
    }
    const auto* user = static_cast<const TOKEN_USER*>(storage);
    const DWORD sid_bytes = GetLengthSid(user->User.Sid);
    state.sid = LocalAlloc(LPTR, sid_bytes);
    if (state.sid == nullptr
        || CopySid(sid_bytes, state.sid, user->User.Sid) == FALSE) {
        LocalFree(storage);
        return false;
    }
    LocalFree(storage);

    LPWSTR sid_text{};
    if (ConvertSidToStringSidW(state.sid, &sid_text) == FALSE) {
        return false;
    }
    try {
        state.sid_text.assign(sid_text);
    } catch (const std::bad_alloc&) {
        LocalFree(sid_text);
        return false;
    }
    LocalFree(sid_text);

    std::wstring sddl;
    try {
        sddl = L"D:P(A;;GA;;;" + state.sid_text + L")(A;;GA;;;SY)";
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.c_str(),
            SDDL_REVISION_1,
            &state.descriptor,
            nullptr)
        == FALSE) {
        return false;
    }
    state.attributes.nLength = sizeof(state.attributes);
    state.attributes.lpSecurityDescriptor = state.descriptor;
    state.attributes.bInheritHandle = FALSE;
    return true;
}

DWORD RemainingTimeout(ULONGLONG deadline) noexcept {
    const ULONGLONG now = GetTickCount64();
    if (now >= deadline) {
        return 0U;
    }
    return static_cast<DWORD>(std::min<ULONGLONG>(
        deadline - now, static_cast<ULONGLONG>(MAXDWORD - 1U)));
}

bool TimedPipeIo(
    HANDLE pipe,
    bool write,
    void* buffer,
    DWORD bytes,
    DWORD timeout,
    DWORD& transferred) noexcept {
    transferred = 0U;
    HANDLE event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (event == nullptr) {
        return false;
    }
    OVERLAPPED overlapped{};
    overlapped.hEvent = event;
    const BOOL started = write
        ? WriteFile(pipe, buffer, bytes, &transferred, &overlapped)
        : ReadFile(pipe, buffer, bytes, &transferred, &overlapped);
    bool success = started != FALSE;
    if (!success && GetLastError() == ERROR_IO_PENDING) {
        if (WaitForSingleObject(event, timeout) == WAIT_OBJECT_0) {
            success = GetOverlappedResult(pipe, &overlapped, &transferred, FALSE)
                != FALSE;
        } else {
            (void)CancelIoEx(pipe, &overlapped);
            (void)WaitForSingleObject(event, INFINITE);
        }
    }
    CloseHandle(event);
    return success;
}

}  // namespace

ActivationCodecStatus EncodeActivationRequest(
    const ActivationRequest& request,
    std::vector<std::uint8_t>& output) noexcept {
    if (request.request_id == 0U || !ValidRequestEnums(request)
        || request.paths.size() > kMaximumActivationPaths) {
        return ActivationCodecStatus::Invalid;
    }
    try {
        std::vector<std::vector<std::uint8_t>> encoded_paths;
        encoded_paths.reserve(request.paths.size());
        std::size_t total = kRequestHeaderBytes;
        for (const auto& path : request.paths) {
            std::vector<std::uint8_t> utf8;
            if (!WideToUtf8(path, utf8)) {
                return ActivationCodecStatus::Invalid;
            }
            if (utf8.size() > UINT32_MAX
                || total > kMaximumActivationMessageBytes - 4U
                || utf8.size() > kMaximumActivationMessageBytes - total - 4U) {
                return ActivationCodecStatus::TooLarge;
            }
            total += 4U + utf8.size();
            encoded_paths.push_back(std::move(utf8));
        }
        output.clear();
        output.reserve(total);
        AppendU32(output, kRequestMagic);
        AppendU16(output, kProtocolVersion);
        AppendU16(output, kRequestHeaderBytes);
        AppendU32(output, static_cast<std::uint32_t>(total));
        AppendU64(output, request.request_id);
        AppendU32(output, static_cast<std::uint32_t>(request.open_mode));
        AppendU32(output, static_cast<std::uint32_t>(request.target));
        AppendU32(output, static_cast<std::uint32_t>(request.paths.size()));
        AppendU32(output, 0U);
        for (const auto& path : encoded_paths) {
            AppendU32(output, static_cast<std::uint32_t>(path.size()));
            output.insert(output.end(), path.begin(), path.end());
        }
        return ActivationCodecStatus::Ok;
    } catch (const std::bad_alloc&) {
        output.clear();
        return ActivationCodecStatus::OutOfMemory;
    }
}

ActivationCodecStatus DecodeActivationRequest(
    const std::uint8_t* bytes,
    std::size_t length,
    ActivationRequest& output) noexcept {
    if (bytes == nullptr || length < kRequestHeaderBytes) {
        return ActivationCodecStatus::Invalid;
    }
    if (length > kMaximumActivationMessageBytes) {
        return ActivationCodecStatus::TooLarge;
    }
    std::size_t cursor{};
    std::uint32_t magic{};
    std::uint16_t version{};
    std::uint16_t header_bytes{};
    std::uint32_t total_bytes{};
    std::uint64_t request_id{};
    std::uint32_t open_mode{};
    std::uint32_t target{};
    std::uint32_t path_count{};
    std::uint32_t reserved{};
    if (!ReadU32(bytes, length, cursor, magic)
        || !ReadU16(bytes, length, cursor, version)
        || !ReadU16(bytes, length, cursor, header_bytes)
        || !ReadU32(bytes, length, cursor, total_bytes)
        || !ReadU64(bytes, length, cursor, request_id)
        || !ReadU32(bytes, length, cursor, open_mode)
        || !ReadU32(bytes, length, cursor, target)
        || !ReadU32(bytes, length, cursor, path_count)
        || !ReadU32(bytes, length, cursor, reserved)
        || magic != kRequestMagic || header_bytes != kRequestHeaderBytes
        || total_bytes != length || reserved != 0U || request_id == 0U
        || path_count > kMaximumActivationPaths) {
        return ActivationCodecStatus::Invalid;
    }
    if (version != kProtocolVersion) {
        return ActivationCodecStatus::UnsupportedVersion;
    }
    ActivationRequest decoded{};
    decoded.request_id = request_id;
    decoded.open_mode = static_cast<ActivationOpenMode>(open_mode);
    decoded.target = static_cast<ActivationTargetPreference>(target);
    if (!ValidRequestEnums(decoded)) {
        return ActivationCodecStatus::Invalid;
    }
    try {
        decoded.paths.reserve(path_count);
        for (std::uint32_t index = 0U; index < path_count; ++index) {
            std::uint32_t path_bytes{};
            if (!ReadU32(bytes, length, cursor, path_bytes)
                || path_bytes == 0U || cursor > length
                || path_bytes > length - cursor) {
                return ActivationCodecStatus::Invalid;
            }
            std::wstring path;
            if (!Utf8ToWide(bytes + cursor, path_bytes, path)) {
                return ActivationCodecStatus::Invalid;
            }
            cursor += path_bytes;
            decoded.paths.push_back(std::move(path));
        }
    } catch (const std::bad_alloc&) {
        return ActivationCodecStatus::OutOfMemory;
    }
    if (cursor != length) {
        return ActivationCodecStatus::Invalid;
    }
    output = std::move(decoded);
    return ActivationCodecStatus::Ok;
}

ActivationCodecStatus EncodeActivationReply(
    const ActivationReply& reply,
    std::vector<std::uint8_t>& output) noexcept {
    if (reply.request_id == 0U || !ValidReplyStatus(reply.status)) {
        return ActivationCodecStatus::Invalid;
    }
    try {
        output.clear();
        output.reserve(kReplyBytes);
        AppendU32(output, kReplyMagic);
        AppendU16(output, kProtocolVersion);
        AppendU16(output, kReplyBytes);
        AppendU32(output, static_cast<std::uint32_t>(reply.status));
        AppendU32(output, reply.primary_process_id);
        AppendU64(output, reply.request_id);
        AppendU32(output, 0U);
        return ActivationCodecStatus::Ok;
    } catch (const std::bad_alloc&) {
        output.clear();
        return ActivationCodecStatus::OutOfMemory;
    }
}

ActivationCodecStatus DecodeActivationReply(
    const std::uint8_t* bytes,
    std::size_t length,
    ActivationReply& output) noexcept {
    if (bytes == nullptr || length != kReplyBytes) {
        return ActivationCodecStatus::Invalid;
    }
    std::size_t cursor{};
    std::uint32_t magic{};
    std::uint16_t version{};
    std::uint16_t reply_bytes{};
    std::uint32_t status{};
    std::uint32_t process_id{};
    std::uint64_t request_id{};
    std::uint32_t reserved{};
    if (!ReadU32(bytes, length, cursor, magic)
        || !ReadU16(bytes, length, cursor, version)
        || !ReadU16(bytes, length, cursor, reply_bytes)
        || !ReadU32(bytes, length, cursor, status)
        || !ReadU32(bytes, length, cursor, process_id)
        || !ReadU64(bytes, length, cursor, request_id)
        || !ReadU32(bytes, length, cursor, reserved)
        || magic != kReplyMagic || reply_bytes != kReplyBytes
        || request_id == 0U || reserved != 0U
        || !ValidReplyStatus(static_cast<ActivationReplyStatus>(status))) {
        return ActivationCodecStatus::Invalid;
    }
    if (version != kProtocolVersion) {
        return ActivationCodecStatus::UnsupportedVersion;
    }
    output = ActivationReply{
        static_cast<ActivationReplyStatus>(status), request_id, process_id};
    return ActivationCodecStatus::Ok;
}

struct ActivationService::Impl final {
    struct Queued final {
        std::uint64_t token{};
        ActivationRequest request;
    };

    ActivationRole role{ActivationRole::Failed};
    DWORD ui_thread_id{};
    std::wstring mutex_name;
    std::wstring pipe_name;
    SecurityState security;
    HANDLE mutex_handle{};
    HANDLE stop_event{};
    std::thread worker;
    std::atomic<bool> stopping{false};
    std::mutex queue_mutex;
    std::deque<Queued> queue;
    std::deque<std::uint64_t> remembered_requests;
    std::uint64_t next_token{1U};

    ~Impl() {
        Stop();
    }

    ActivationRole Start(
        DWORD next_ui_thread_id,
        std::wstring_view suffix) noexcept {
        if (role != ActivationRole::Failed || next_ui_thread_id == 0U
            || !SafeTestSuffix(suffix) || !BuildSecurity(security)) {
            return ActivationRole::Failed;
        }
        DWORD session_id{};
        if (ProcessIdToSessionId(GetCurrentProcessId(), &session_id) == FALSE) {
            return ActivationRole::Failed;
        }
        try {
            std::wstring suffix_text;
            if (!suffix.empty()) {
                suffix_text = L".";
                suffix_text.append(suffix);
            }
            const std::wstring base = L"inkpod.activation.v1."
                + security.sid_text + L"." + std::to_wstring(session_id)
                + suffix_text;
            mutex_name = L"Local\\" + base;
            pipe_name = L"\\\\.\\pipe\\" + base;
        } catch (const std::bad_alloc&) {
            return ActivationRole::Failed;
        }
        mutex_handle = CreateMutexW(&security.attributes, FALSE, mutex_name.c_str());
        if (mutex_handle == nullptr) {
            return ActivationRole::Failed;
        }
        if (GetLastError() == ERROR_ALREADY_EXISTS) {
            role = ActivationRole::Secondary;
            return role;
        }
        stop_event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
        if (stop_event == nullptr) {
            CloseHandle(mutex_handle);
            mutex_handle = nullptr;
            return ActivationRole::Failed;
        }
        ui_thread_id = next_ui_thread_id;
        try {
            worker = std::thread([this] { ServerLoop(); });
        } catch (const std::system_error&) {
            CloseHandle(stop_event);
            stop_event = nullptr;
            CloseHandle(mutex_handle);
            mutex_handle = nullptr;
            return ActivationRole::Failed;
        }
        role = ActivationRole::Primary;
        return role;
    }

    void Stop() noexcept {
        if (role == ActivationRole::Primary) {
            stopping.store(true, std::memory_order_release);
            if (stop_event != nullptr) {
                SetEvent(stop_event);
            }
            if (worker.joinable()) {
                worker.join();
            }
        }
        if (stop_event != nullptr) {
            CloseHandle(stop_event);
            stop_event = nullptr;
        }
        if (mutex_handle != nullptr) {
            CloseHandle(mutex_handle);
            mutex_handle = nullptr;
        }
        {
            std::lock_guard lock(queue_mutex);
            queue.clear();
            remembered_requests.clear();
        }
        role = ActivationRole::Failed;
    }

    void ServerLoop() noexcept {
        while (!stopping.load(std::memory_order_acquire)) {
            HANDLE pipe = CreateNamedPipeW(
                pipe_name.c_str(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT
                    | PIPE_REJECT_REMOTE_CLIENTS,
                1U,
                kPipeBufferBytes,
                kPipeBufferBytes,
                0U,
                &security.attributes);
            if (pipe == INVALID_HANDLE_VALUE) {
                return;
            }
            HANDLE connected = CreateEventW(nullptr, TRUE, FALSE, nullptr);
            if (connected == nullptr) {
                CloseHandle(pipe);
                return;
            }
            OVERLAPPED overlap{};
            overlap.hEvent = connected;
            const BOOL started = ConnectNamedPipe(pipe, &overlap);
            const DWORD error = started == FALSE ? GetLastError() : ERROR_SUCCESS;
            bool ready = started != FALSE || error == ERROR_PIPE_CONNECTED;
            if (!ready && error == ERROR_IO_PENDING) {
                const HANDLE waits[]{connected, stop_event};
                ready = WaitForMultipleObjects(2U, waits, FALSE, INFINITE)
                    == WAIT_OBJECT_0;
            }
            if (!ready) {
                (void)CancelIoEx(pipe, &overlap);
                (void)WaitForSingleObject(connected, INFINITE);
                CloseHandle(connected);
                CloseHandle(pipe);
                continue;
            }
            CloseHandle(connected);
            if (stopping.load(std::memory_order_acquire)) {
                DisconnectNamedPipe(pipe);
                CloseHandle(pipe);
                break;
            }
            ProcessClient(pipe);
            FlushFileBuffers(pipe);
            DisconnectNamedPipe(pipe);
            CloseHandle(pipe);
        }
    }

    bool WaitServerIo(
        HANDLE pipe,
        bool write,
        void* buffer,
        DWORD bytes,
        DWORD& transferred) noexcept {
        transferred = 0U;
        HANDLE event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
        if (event == nullptr) {
            return false;
        }
        OVERLAPPED overlap{};
        overlap.hEvent = event;
        const BOOL started = write
            ? WriteFile(pipe, buffer, bytes, &transferred, &overlap)
            : ReadFile(pipe, buffer, bytes, &transferred, &overlap);
        bool success = started != FALSE;
        if (!success && GetLastError() == ERROR_IO_PENDING) {
            const HANDLE waits[]{event, stop_event};
            if (WaitForMultipleObjects(2U, waits, FALSE, INFINITE)
                == WAIT_OBJECT_0) {
                success = GetOverlappedResult(
                    pipe, &overlap, &transferred, FALSE) != FALSE;
            } else {
                (void)CancelIoEx(pipe, &overlap);
                (void)WaitForSingleObject(event, INFINITE);
            }
        }
        CloseHandle(event);
        return success;
    }

    void ProcessClient(HANDLE pipe) noexcept {
        std::vector<std::uint8_t> input;
        try {
            input.resize(kMaximumActivationMessageBytes + 1U);
        } catch (const std::bad_alloc&) {
            return;
        }
        DWORD read{};
        ActivationReply reply{
            ActivationReplyStatus::InvalidRequest,
            1U,
            GetCurrentProcessId()};
        if (WaitServerIo(
                pipe,
                false,
                input.data(),
                static_cast<DWORD>(input.size()),
                read)) {
            ActivationRequest request{};
            const ActivationCodecStatus decoded = DecodeActivationRequest(
                input.data(), read, request);
            if (decoded == ActivationCodecStatus::Ok) {
                reply.request_id = request.request_id;
                reply.status = QueueRequest(std::move(request));
            }
        }
        std::vector<std::uint8_t> encoded;
        if (EncodeActivationReply(reply, encoded) != ActivationCodecStatus::Ok) {
            return;
        }
        DWORD written{};
        (void)WaitServerIo(
            pipe,
            true,
            encoded.data(),
            static_cast<DWORD>(encoded.size()),
            written);
    }

    ActivationReplyStatus QueueRequest(ActivationRequest request) noexcept {
        std::uint64_t token{};
        const std::uint64_t request_id = request.request_id;
        {
            std::lock_guard lock(queue_mutex);
            if (stopping.load(std::memory_order_acquire)) {
                return ActivationReplyStatus::ShuttingDown;
            }
            if (std::find(
                    remembered_requests.begin(),
                    remembered_requests.end(),
                    request.request_id)
                != remembered_requests.end()) {
                return ActivationReplyStatus::Duplicate;
            }
            if (queue.size() >= kMaximumQueuedActivations) {
                return ActivationReplyStatus::QueueFull;
            }
            token = next_token++;
            if (token == 0U) {
                token = next_token++;
            }
            try {
                remembered_requests.push_back(request_id);
                queue.push_back(Queued{token, std::move(request)});
                if (remembered_requests.size() > kRememberedRequestCount) {
                    remembered_requests.pop_front();
                }
            } catch (const std::bad_alloc&) {
                if (!remembered_requests.empty()
                    && remembered_requests.back() == request_id) {
                    remembered_requests.pop_back();
                }
                return ActivationReplyStatus::QueueFull;
            }
        }
        const WPARAM low = static_cast<WPARAM>(token & UINT64_C(0xffffffff));
        const LPARAM high = static_cast<LPARAM>(token >> 32U);
        if (PostThreadMessageW(
                ui_thread_id, kApplicationActivationMessage, low, high)
            == FALSE) {
            std::lock_guard lock(queue_mutex);
            const auto found = std::find_if(
                queue.begin(), queue.end(), [token](const Queued& item) {
                    return item.token == token;
                });
            if (found != queue.end()) {
                queue.erase(found);
            }
            const auto remembered = std::find(
                remembered_requests.begin(),
                remembered_requests.end(),
                request_id);
            if (remembered != remembered_requests.end()) {
                remembered_requests.erase(remembered);
            }
            return ActivationReplyStatus::QueueFull;
        }
        return ActivationReplyStatus::Accepted;
    }

    bool Take(std::uint64_t token, ActivationRequest& request) noexcept {
        std::lock_guard lock(queue_mutex);
        const auto found = std::find_if(
            queue.begin(), queue.end(), [token](const Queued& item) {
                return item.token == token;
            });
        if (found == queue.end()) {
            return false;
        }
        request = std::move(found->request);
        queue.erase(found);
        return true;
    }

    ActivationSendStatus Send(
        const ActivationRequest& request,
        DWORD timeout_milliseconds,
        ActivationReply* reply_output) noexcept {
        if (role != ActivationRole::Secondary || timeout_milliseconds == 0U) {
            return ActivationSendStatus::Unavailable;
        }
        std::vector<std::uint8_t> encoded;
        if (EncodeActivationRequest(request, encoded) != ActivationCodecStatus::Ok) {
            return ActivationSendStatus::Rejected;
        }
        const ULONGLONG deadline = GetTickCount64() + timeout_milliseconds;
        HANDLE pipe = INVALID_HANDLE_VALUE;
        while (RemainingTimeout(deadline) != 0U) {
            pipe = CreateFileW(
                pipe_name.c_str(),
                GENERIC_READ | GENERIC_WRITE,
                0U,
                nullptr,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT
                    | SECURITY_IDENTIFICATION,
                nullptr);
            if (pipe != INVALID_HANDLE_VALUE) {
                break;
            }
            const DWORD error = GetLastError();
            if (error != ERROR_PIPE_BUSY && error != ERROR_FILE_NOT_FOUND) {
                return ActivationSendStatus::Unavailable;
            }
            const DWORD remaining = RemainingTimeout(deadline);
            if (remaining == 0U) {
                return ActivationSendStatus::Timeout;
            }
            if (WaitNamedPipeW(pipe_name.c_str(), remaining) == FALSE
                && GetLastError() == ERROR_SEM_TIMEOUT) {
                return ActivationSendStatus::Timeout;
            }
        }
        if (pipe == INVALID_HANDLE_VALUE) {
            return ActivationSendStatus::Timeout;
        }
        DWORD mode = PIPE_READMODE_MESSAGE;
        (void)SetNamedPipeHandleState(pipe, &mode, nullptr, nullptr);
        DWORD transferred{};
        DWORD remaining = RemainingTimeout(deadline);
        bool success = remaining != 0U && TimedPipeIo(
            pipe,
            true,
            encoded.data(),
            static_cast<DWORD>(encoded.size()),
            remaining,
            transferred)
            && transferred == encoded.size();
        std::array<std::uint8_t, kReplyBytes> reply_bytes{};
        if (success) {
            remaining = RemainingTimeout(deadline);
            success = remaining != 0U && TimedPipeIo(
                pipe,
                false,
                reply_bytes.data(),
                static_cast<DWORD>(reply_bytes.size()),
                remaining,
                transferred)
                && transferred == reply_bytes.size();
        }
        CloseHandle(pipe);
        if (!success) {
            return RemainingTimeout(deadline) == 0U
                ? ActivationSendStatus::Timeout
                : ActivationSendStatus::Unavailable;
        }
        ActivationReply reply{};
        if (DecodeActivationReply(reply_bytes.data(), reply_bytes.size(), reply)
                != ActivationCodecStatus::Ok
            || reply.request_id != request.request_id) {
            return ActivationSendStatus::Rejected;
        }
        if (reply_output != nullptr) {
            *reply_output = reply;
        }
        if (reply.primary_process_id != 0U) {
            (void)AllowSetForegroundWindow(reply.primary_process_id);
        }
        return reply.status == ActivationReplyStatus::Accepted
            ? ActivationSendStatus::Accepted
            : (reply.status == ActivationReplyStatus::Duplicate
                      ? ActivationSendStatus::Duplicate
                      : ActivationSendStatus::Rejected);
    }
};

ActivationService::ActivationService() : impl_(std::make_unique<Impl>()) {}

ActivationService::~ActivationService() = default;

ActivationRole ActivationService::Start(
    DWORD ui_thread_id,
    std::wstring_view test_namespace_suffix) noexcept {
    return impl_ == nullptr
        ? ActivationRole::Failed
        : impl_->Start(ui_thread_id, test_namespace_suffix);
}

ActivationSendStatus ActivationService::Send(
    const ActivationRequest& request,
    DWORD timeout_milliseconds,
    ActivationReply* reply) noexcept {
    return impl_ == nullptr
        ? ActivationSendStatus::Unavailable
        : impl_->Send(request, timeout_milliseconds, reply);
}

bool ActivationService::Take(
    std::uint64_t token,
    ActivationRequest& request) noexcept {
    return impl_ != nullptr && impl_->Take(token, request);
}

void ActivationService::Stop() noexcept {
    if (impl_ != nullptr) {
        impl_->Stop();
    }
}

ActivationRole ActivationService::Role() const noexcept {
    return impl_ == nullptr ? ActivationRole::Failed : impl_->role;
}

const std::wstring& ActivationService::PipeNameForTesting() const noexcept {
    static const std::wstring empty;
    return impl_ == nullptr ? empty : impl_->pipe_name;
}

std::uint64_t NewActivationRequestId() noexcept {
    static std::atomic<std::uint64_t> sequence{[] {
        LARGE_INTEGER counter{};
        QueryPerformanceCounter(&counter);
        FILETIME now{};
        GetSystemTimeAsFileTime(&now);
        std::uint64_t seed = static_cast<std::uint64_t>(counter.QuadPart)
            ^ (static_cast<std::uint64_t>(now.dwHighDateTime) << 32U)
            ^ static_cast<std::uint64_t>(now.dwLowDateTime)
            ^ (static_cast<std::uint64_t>(GetCurrentProcessId()) << 32U);
        return seed == 0U ? UINT64_C(1) : seed;
    }()};
    std::uint64_t value = sequence.fetch_add(1U, std::memory_order_relaxed);
    if (value == 0U) {
        value = sequence.fetch_add(1U, std::memory_order_relaxed);
    }
    return value;
}

}  // namespace inkpod::app
