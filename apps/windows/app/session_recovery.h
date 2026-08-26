#pragma once

#include <windows.h>

#include <cstdint>
#include <span>
#include <string>
#include <vector>

#include "document_identity.h"
#include "identity.h"

namespace inkpod::app {

struct RecoveryMetadata final {
    DocumentSessionId session{};
    Generation generation{};
    std::uint64_t document_uuid_high{};
    std::uint64_t document_uuid_low{};
    DocumentIdentity original_identity{};
    std::wstring original_path;
    std::wstring source_path;
    std::uint64_t written_file_time{};
};

struct RecoveryCandidate final {
    std::wstring recovery_path;
    std::wstring metadata_path;
    FILETIME modified{};
    bool has_metadata{};
    RecoveryMetadata metadata{};
};

inline constexpr std::size_t kMaximumRecoveryCandidates = 4096U;

enum class SequenceCellSwitchPolicy : std::uint32_t {
    Prompt = 1U,
    AutosaveBeforeSwitch = 2U,
};

enum class SequenceEndpointPolicy : std::uint32_t {
    Stop = 1U,
    Wrap = 2U,
};

enum class OutputColorGuardProfileSetting : std::uint32_t {
    Bt709ConservativeYcbcr = 1U,
};

[[nodiscard]] bool RecoveryRootDirectory(std::wstring& output) noexcept;
[[nodiscard]] bool SequenceRecoveryPath(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation,
    std::wstring& output) noexcept;
[[nodiscard]] bool RecoveryMetadataPath(
    const std::wstring& recovery_path,
    std::wstring& output) noexcept;
[[nodiscard]] bool BuildRecoveryMetadata(
    DocumentSessionId session,
    Generation generation,
    const DocumentIdentity& identity,
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    const std::wstring& current_path,
    const std::wstring& source_path,
    RecoveryMetadata& output) noexcept;
[[nodiscard]] bool EncodeRecoveryMetadata(
    const RecoveryMetadata& metadata,
    std::vector<std::uint8_t>& output) noexcept;
[[nodiscard]] bool DecodeRecoveryMetadata(
    const std::uint8_t* bytes,
    std::size_t length,
    RecoveryMetadata& output) noexcept;
[[nodiscard]] bool WriteRecoveryMetadata(
    const std::wstring& recovery_path,
    const RecoveryMetadata& metadata) noexcept;
[[nodiscard]] bool ReadRecoveryMetadata(
    const std::wstring& recovery_path,
    RecoveryMetadata& metadata) noexcept;
[[nodiscard]] bool EnumerateRecoveryCandidates(
    std::vector<RecoveryCandidate>& output) noexcept;
[[nodiscard]] bool EnumerateRecoveryCandidatesInDirectory(
    const std::wstring& directory,
    std::vector<RecoveryCandidate>& output) noexcept;
[[nodiscard]] bool DiscardRecoveryArtifact(
    const std::wstring& recovery_path) noexcept;

[[nodiscard]] bool LoadPreviousDocumentPaths(
    std::vector<std::wstring>& paths) noexcept;
[[nodiscard]] bool EncodePreviousDocumentPaths(
    std::span<const std::wstring> paths,
    std::vector<std::uint8_t>& output) noexcept;
[[nodiscard]] bool DecodePreviousDocumentPaths(
    const std::uint8_t* bytes,
    std::size_t length,
    std::vector<std::wstring>& paths) noexcept;
[[nodiscard]] bool SavePreviousDocumentPaths(
    std::span<const std::wstring> paths) noexcept;
[[nodiscard]] bool ClearPreviousDocumentPaths() noexcept;

}  // namespace inkpod::app
