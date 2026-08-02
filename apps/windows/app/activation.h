#pragma once

#include <windows.h>

#include <cstdint>
#include <memory>
#include <string>
#include <string_view>
#include <vector>

namespace inkpod::app {

inline constexpr UINT kApplicationActivationMessage = WM_APP + 0x180U;
inline constexpr std::size_t kMaximumActivationPaths = 64U;
inline constexpr std::size_t kMaximumActivationMessageBytes = 1024U * 1024U;

enum class ActivationOpenMode : std::uint32_t {
    Documents = 1U,
};

enum class ActivationTargetPreference : std::uint32_t {
    LastFocusedWorkspace = 1U,
    NewWorkspace = 2U,
};

struct ActivationRequest final {
    std::uint64_t request_id{};
    ActivationOpenMode open_mode{ActivationOpenMode::Documents};
    ActivationTargetPreference target{
        ActivationTargetPreference::LastFocusedWorkspace};
    std::vector<std::wstring> paths;
};

enum class ActivationReplyStatus : std::uint32_t {
    Accepted = 0U,
    Duplicate = 1U,
    InvalidRequest = 2U,
    QueueFull = 3U,
    ShuttingDown = 4U,
};

struct ActivationReply final {
    ActivationReplyStatus status{ActivationReplyStatus::InvalidRequest};
    std::uint64_t request_id{};
    std::uint32_t primary_process_id{};
};

enum class ActivationCodecStatus {
    Ok,
    Invalid,
    UnsupportedVersion,
    TooLarge,
    OutOfMemory,
};

[[nodiscard]] ActivationCodecStatus EncodeActivationRequest(
    const ActivationRequest& request,
    std::vector<std::uint8_t>& output) noexcept;
[[nodiscard]] ActivationCodecStatus DecodeActivationRequest(
    const std::uint8_t* bytes,
    std::size_t length,
    ActivationRequest& output) noexcept;
[[nodiscard]] ActivationCodecStatus EncodeActivationReply(
    const ActivationReply& reply,
    std::vector<std::uint8_t>& output) noexcept;
[[nodiscard]] ActivationCodecStatus DecodeActivationReply(
    const std::uint8_t* bytes,
    std::size_t length,
    ActivationReply& output) noexcept;

enum class ActivationRole {
    Primary,
    Secondary,
    Failed,
};

enum class ActivationSendStatus {
    Accepted,
    Duplicate,
    Rejected,
    Timeout,
    Unavailable,
};

class ActivationService final {
public:
    ActivationService();
    ~ActivationService();

    ActivationService(const ActivationService&) = delete;
    ActivationService& operator=(const ActivationService&) = delete;

    [[nodiscard]] ActivationRole Start(
        DWORD ui_thread_id,
        std::wstring_view test_namespace_suffix = {}) noexcept;
    [[nodiscard]] ActivationSendStatus Send(
        const ActivationRequest& request,
        DWORD timeout_milliseconds,
        ActivationReply* reply = nullptr) noexcept;
    [[nodiscard]] bool Take(
        std::uint64_t token,
        ActivationRequest& request) noexcept;
    void Stop() noexcept;

    [[nodiscard]] ActivationRole Role() const noexcept;
    [[nodiscard]] const std::wstring& PipeNameForTesting() const noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

[[nodiscard]] std::uint64_t NewActivationRequestId() noexcept;

}  // namespace inkpod::app
