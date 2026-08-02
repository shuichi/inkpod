#include "app/activation.h"

#include <windows.h>

#include <array>
#include <atomic>
#include <cstdint>
#include <string>
#include <thread>
#include <vector>

namespace {

using inkpod::app::ActivationCodecStatus;
using inkpod::app::ActivationReply;
using inkpod::app::ActivationReplyStatus;
using inkpod::app::ActivationRequest;
using inkpod::app::ActivationRole;
using inkpod::app::ActivationSendStatus;
using inkpod::app::ActivationService;
using inkpod::app::ActivationTargetPreference;
using inkpod::app::DecodeActivationReply;
using inkpod::app::DecodeActivationRequest;
using inkpod::app::EncodeActivationRequest;
using inkpod::app::NewActivationRequestId;
using inkpod::app::kApplicationActivationMessage;
using inkpod::app::kMaximumActivationMessageBytes;

bool TakePostedActivation(
    ActivationService& service,
    ActivationRequest& request,
    DWORD timeout_milliseconds = 3000U) {
    const ULONGLONG deadline = GetTickCount64() + timeout_milliseconds;
    for (;;) {
        MSG message{};
        while (PeekMessageW(
            &message, nullptr, kApplicationActivationMessage,
            kApplicationActivationMessage, PM_REMOVE) != FALSE) {
            const std::uint64_t token =
                static_cast<std::uint64_t>(message.wParam & UINT32_MAX)
                | (static_cast<std::uint64_t>(
                       static_cast<std::uint32_t>(message.lParam))
                   << 32U);
            if (service.Take(token, request)) {
                return true;
            }
        }
        if (GetTickCount64() >= deadline) {
            return false;
        }
        Sleep(1U);
    }
}

int TestCodec() {
    ActivationRequest request{};
    request.request_id = UINT64_C(0x1122334455667788);
    request.target = ActivationTargetPreference::NewWorkspace;
    request.paths = {
        L"C:\\制作 資料\\セル 01.inkpod",
        L"C:\\長い名前\\セル02.png"};
    std::vector<std::uint8_t> bytes;
    if (EncodeActivationRequest(request, bytes) != ActivationCodecStatus::Ok
        || bytes.empty() || bytes.size() > kMaximumActivationMessageBytes) {
        return 1;
    }
    ActivationRequest decoded{};
    if (DecodeActivationRequest(bytes.data(), bytes.size(), decoded)
            != ActivationCodecStatus::Ok
        || decoded.request_id != request.request_id
        || decoded.target != request.target || decoded.paths != request.paths) {
        return 2;
    }
    std::vector<std::uint8_t> truncated = bytes;
    truncated.pop_back();
    if (DecodeActivationRequest(
            truncated.data(), truncated.size(), decoded)
        == ActivationCodecStatus::Ok) {
        return 3;
    }
    std::vector<std::uint8_t> wrong_version = bytes;
    wrong_version[4] = 2U;
    if (DecodeActivationRequest(
            wrong_version.data(), wrong_version.size(), decoded)
        != ActivationCodecStatus::UnsupportedVersion) {
        return 4;
    }
    std::vector<std::uint8_t> oversized(
        kMaximumActivationMessageBytes + 1U, 0U);
    if (DecodeActivationRequest(oversized.data(), oversized.size(), decoded)
        != ActivationCodecStatus::TooLarge) {
        return 5;
    }
    request.paths = {std::wstring(32768U, L'x')};
    if (EncodeActivationRequest(request, bytes) == ActivationCodecStatus::Ok) {
        return 6;
    }
    return 0;
}

int TestInvalidPipeMessage(const std::wstring& pipe_name) {
    const ULONGLONG deadline = GetTickCount64() + 3000U;
    while (WaitNamedPipeW(pipe_name.c_str(), 50U) == FALSE) {
        const DWORD error = GetLastError();
        if ((error != ERROR_FILE_NOT_FOUND && error != ERROR_SEM_TIMEOUT)
            || GetTickCount64() >= deadline) {
            return 20;
        }
        Sleep(1U);
    }
    HANDLE pipe = CreateFileW(
        pipe_name.c_str(),
        GENERIC_READ | GENERIC_WRITE,
        0U,
        nullptr,
        OPEN_EXISTING,
        0U,
        nullptr);
    if (pipe == INVALID_HANDLE_VALUE) {
        return 21;
    }
    const std::array<std::uint8_t, 8U> forged{1U, 2U, 3U, 4U, 5U, 6U, 7U, 8U};
    DWORD transferred{};
    std::array<std::uint8_t, 28U> response{};
    const bool exchanged = WriteFile(
        pipe,
        forged.data(),
        static_cast<DWORD>(forged.size()),
        &transferred,
        nullptr) != FALSE
        && transferred == forged.size()
        && ReadFile(
            pipe,
            response.data(),
            static_cast<DWORD>(response.size()),
            &transferred,
            nullptr) != FALSE
        && transferred == response.size();
    CloseHandle(pipe);
    ActivationReply reply{};
    return exchanged
            && DecodeActivationReply(response.data(), response.size(), reply)
                == ActivationCodecStatus::Ok
            && reply.status == ActivationReplyStatus::InvalidRequest
        ? 0
        : 22;
}

int TestSingleInstanceTransport() {
    MSG queue_probe{};
    (void)PeekMessageW(&queue_probe, nullptr, WM_USER, WM_USER, PM_NOREMOVE);
    const std::wstring suffix = L"test_" + std::to_wstring(GetCurrentProcessId())
        + L"_" + std::to_wstring(GetTickCount64());
    ActivationService primary;
    if (primary.Start(GetCurrentThreadId(), suffix) != ActivationRole::Primary) {
        return 30;
    }
    ActivationService secondary;
    if (secondary.Start(GetCurrentThreadId(), suffix)
        != ActivationRole::Secondary) {
        return 31;
    }
    const int malformed = TestInvalidPipeMessage(primary.PipeNameForTesting());
    if (malformed != 0) {
        return malformed;
    }

    ActivationRequest request{};
    request.request_id = NewActivationRequestId();
    request.paths = {L"C:\\制作 資料\\同時起動.inkpod"};
    if (secondary.Send(request, 3000U) != ActivationSendStatus::Accepted) {
        return 32;
    }
    ActivationRequest received{};
    if (!TakePostedActivation(primary, received)
        || received.request_id != request.request_id
        || received.paths != request.paths) {
        return 33;
    }
    if (secondary.Send(request, 3000U) != ActivationSendStatus::Duplicate) {
        return 34;
    }

    constexpr std::size_t concurrent_count = 4U;
    std::array<std::atomic<int>, concurrent_count> results{};
    std::array<std::thread, concurrent_count> clients;
    for (std::size_t index = 0U; index < concurrent_count; ++index) {
        clients[index] = std::thread([&, index] {
            ActivationService client;
            if (client.Start(GetCurrentThreadId(), suffix)
                != ActivationRole::Secondary) {
                results[index].store(1);
                return;
            }
            ActivationRequest concurrent{};
            concurrent.request_id = NewActivationRequestId();
            concurrent.paths = {
                L"C:\\同時\\cell " + std::to_wstring(index) + L".inkpod"};
            results[index].store(
                client.Send(concurrent, 5000U)
                        == ActivationSendStatus::Accepted
                    ? 0
                    : 2);
        });
    }
    for (auto& client : clients) {
        client.join();
    }
    for (const auto& result : results) {
        if (result.load() != 0) {
            return 35;
        }
    }
    for (std::size_t index = 0U; index < concurrent_count; ++index) {
        if (!TakePostedActivation(primary, received)) {
            return 36;
        }
    }

    primary.Stop();
    request.request_id = NewActivationRequestId();
    if (secondary.Send(request, 100U) != ActivationSendStatus::Timeout) {
        return 37;
    }
    return 0;
}

int TestUiQueueFailureDoesNotDedupe() {
    const std::wstring suffix = L"queue_failure_"
        + std::to_wstring(GetCurrentProcessId()) + L"_"
        + std::to_wstring(GetTickCount64());
    ActivationService primary;
    if (primary.Start(MAXDWORD, suffix) != ActivationRole::Primary) {
        return 40;
    }
    ActivationService secondary;
    if (secondary.Start(GetCurrentThreadId(), suffix)
        != ActivationRole::Secondary) {
        return 41;
    }
    ActivationRequest request{};
    request.request_id = NewActivationRequestId();
    request.paths = {L"C:\\queue-failure.inkpod"};
    if (secondary.Send(request, 3000U) != ActivationSendStatus::Rejected
        || secondary.Send(request, 3000U) != ActivationSendStatus::Rejected) {
        return 42;
    }
    primary.Stop();
    return 0;
}

}  // namespace

int main() {
    const int codec = TestCodec();
    if (codec != 0) {
        return codec;
    }
    const int transport = TestSingleInstanceTransport();
    return transport == 0 ? TestUiQueueFailureDoesNotDedupe() : transport;
}
