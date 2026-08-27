#pragma once

// Bounded blocking adapters for smoke/unit fixture code only. Production callers
// must use FileIoController and its nonblocking completion/polling path.
#include "session_recovery.h"

#include <algorithm>
#include <chrono>
#include <climits>
#include <cwchar>
#include <new>
#include <string_view>
#include <thread>
#include <utility>

namespace inkpod::app::fixture {

inline bool RecoveryPathUtf8(std::wstring_view path, std::vector<std::uint8_t>& bytes) {
    if (path.empty() || path.size() > 32767U
        || std::find(path.begin(), path.end(), L'\0') != path.end()) {
        return false;
    }
    const int count = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS,
        path.data(), static_cast<int>(path.size()), nullptr, 0, nullptr, nullptr);
    if (count <= 0) {
        return false;
    }
    bytes.resize(static_cast<std::size_t>(count));
    return WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS,
        path.data(), static_cast<int>(path.size()), reinterpret_cast<char*>(bytes.data()),
        count, nullptr, nullptr) == count;
}

inline bool RecoveryPathWide(const std::vector<std::uint8_t>& bytes, std::wstring& path) {
    if (bytes.empty() || bytes.size() > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(bytes.data()), static_cast<int>(bytes.size()), nullptr, 0);
    if (count <= 0 || count > 32767) {
        return false;
    }
    path.resize(static_cast<std::size_t>(count));
    return MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(bytes.data()), static_cast<int>(bytes.size()),
        path.data(), count) == count;
}

class RecoveryFixtureJob final {
public:
    RecoveryFixtureJob() = default;
    RecoveryFixtureJob(const RecoveryFixtureJob&) = delete;
    RecoveryFixtureJob& operator=(const RecoveryFixtureJob&) = delete;
    ~RecoveryFixtureJob() {
        if (job != nullptr) { (void)inkpod_io_job_release(&job); }
        if (manager != nullptr) { (void)inkpod_io_manager_release(&manager); }
    }

    bool Run(std::uint32_t kind, const std::wstring& path) {
        std::vector<std::uint8_t> bytes;
        if (!RecoveryPathUtf8(path, bytes)
            || inkpod_io_manager_create(nullptr, &manager) != INKPOD_STATUS_OK) {
            return false;
        }
        InkpodIoPath input{};
        input.struct_size = static_cast<std::uint32_t>(sizeof(input));
        input.path = bytes.data();
        input.path_bytes = bytes.size();
        InkpodIoRequest request{};
        request.struct_size = static_cast<std::uint32_t>(sizeof(request));
        request.kind = kind;
        request.paths = &input;
        request.path_count = 1U;
        request.path_stride_bytes = sizeof(input);
        if (inkpod_core_io_submit(nullptr, manager, &request, &job) != INKPOD_STATUS_OK) {
            return false;
        }
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
        do {
            info.struct_size = static_cast<std::uint32_t>(sizeof(info));
            if (inkpod_io_job_poll(job, &info) != INKPOD_STATUS_OK) { return false; }
            if (info.state == INKPOD_IO_COMPLETE) { return info.status == INKPOD_STATUS_OK; }
            if (info.state == INKPOD_IO_FAILED || info.state == INKPOD_IO_CANCELLED) { return false; }
            std::this_thread::sleep_for(std::chrono::milliseconds(1));
        } while (std::chrono::steady_clock::now() < deadline);
        (void)inkpod_io_job_cancel(job);
        return false;
    }

    InkpodIoManager* manager{};
    InkpodIoJob* job{};
    InkpodIoJobInfo info{};
};

inline bool EnumerateRecoveryCandidatesInDirectory(
    const std::wstring& directory,
    std::vector<RecoveryCandidate>& output) noexcept {
    try {
        RecoveryFixtureJob run;
        if (!run.Run(INKPOD_IO_RECOVERY_LIST, directory)
            || run.info.result_count > kMaximumRecoveryCandidates) {
            return false;
        }
        std::vector<RecoveryCandidate> candidates;
        candidates.reserve(static_cast<std::size_t>(run.info.result_count));
        for (std::uint64_t index = 0U; index < run.info.result_count; ++index) {
            InkpodIoItemInfo item{};
            item.struct_size = static_cast<std::uint32_t>(sizeof(item));
            if (inkpod_io_job_get_item(run.job, index, &item, nullptr, 0U, nullptr, 0U)
                    != INKPOD_STATUS_OK
                || item.path_bytes > 131068U) {
                return false;
            }
            std::vector<std::uint8_t> path(static_cast<std::size_t>(item.path_bytes));
            if (inkpod_io_job_get_item(run.job, index, &item, path.data(), path.size(), nullptr, 0U)
                != INKPOD_STATUS_OK) {
                return false;
            }
            RecoveryCandidate candidate{};
            if (!RecoveryPathWide(path, candidate.recovery_path)
                || !RecoveryMetadataPath(candidate.recovery_path, candidate.metadata_path)) {
                return false;
            }
            InkpodIoRecoveryMetadata metadata{};
            metadata.struct_size = static_cast<std::uint32_t>(sizeof(metadata));
            std::uint64_t required{};
            if (inkpod_io_job_get_recovery_metadata(run.job, index, &metadata, nullptr, 0U, &required)
                    != INKPOD_STATUS_OK
                || required > 512U * 1024U) {
                return false;
            }
            std::vector<std::uint8_t> text(static_cast<std::size_t>(required));
            if (inkpod_io_job_get_recovery_metadata(run.job, index, &metadata,
                    text.data(), text.size(), &required) != INKPOD_STATUS_OK) {
                return false;
            }
            candidate.modified.dwLowDateTime = static_cast<DWORD>(metadata.modified_time_100ns);
            candidate.modified.dwHighDateTime = static_cast<DWORD>(metadata.modified_time_100ns >> 32U);
            candidate.has_metadata = (metadata.flags & 1U) != 0U;
            if (candidate.has_metadata && !RecoveryMetadataFromAbi(metadata, candidate.metadata)) {
                return false;
            }
            candidates.push_back(std::move(candidate));
        }
        output = std::move(candidates);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

inline bool EnumerateRecoveryCandidates(std::vector<RecoveryCandidate>& output) noexcept {
    std::wstring root;
    return RecoveryRootDirectory(root) && EnumerateRecoveryCandidatesInDirectory(root, output);
}

inline bool ReadRecoveryMetadata(const std::wstring& path, RecoveryMetadata& output) noexcept {
    try {
        const auto separator = path.find_last_of(L"/\\");
        const auto directory = separator == std::wstring::npos ? std::wstring(L".") : path.substr(0U, separator);
        const auto name = separator == std::wstring::npos ? path : path.substr(separator + 1U);
        std::vector<RecoveryCandidate> candidates;
        if (!EnumerateRecoveryCandidatesInDirectory(directory, candidates)) { return false; }
        for (const auto& candidate : candidates) {
            const auto candidate_separator = candidate.recovery_path.find_last_of(L"/\\");
            const auto candidate_name = candidate.recovery_path.substr(candidate_separator == std::wstring::npos ? 0U : candidate_separator + 1U);
            if (_wcsicmp(name.c_str(), candidate_name.c_str()) == 0) {
                if (!candidate.has_metadata) { return false; }
                output = candidate.metadata;
                return true;
            }
        }
        return false;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

inline bool DiscardRecoveryArtifact(const std::wstring& path) noexcept {
    try {
        RecoveryFixtureJob run;
        return run.Run(INKPOD_IO_RECOVERY_DISCARD, path);
    } catch (const std::bad_alloc&) {
        return false;
    }
}

}  // namespace inkpod::app::fixture
