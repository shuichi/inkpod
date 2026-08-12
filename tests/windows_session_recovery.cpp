#include "app/session_recovery.h"

#include <windows.h>

#include <array>
#include <cstdint>
#include <string>
#include <vector>

namespace {

using inkpod::app::DecodePreviousDocumentPaths;
using inkpod::app::DecodeRecoveryMetadata;
using inkpod::app::DecodeSequenceCellSwitchPolicy;
using inkpod::app::DecodeSequenceEndpointPolicy;
using inkpod::app::DecodeOutputColorGuardProfileSetting;
using inkpod::app::DiscardRecoveryArtifact;
using inkpod::app::DocumentIdentity;
using inkpod::app::DocumentIdentityKind;
using inkpod::app::DocumentSessionId;
using inkpod::app::EncodePreviousDocumentPaths;
using inkpod::app::EncodeRecoveryMetadata;
using inkpod::app::EncodeSequenceCellSwitchPolicy;
using inkpod::app::EncodeSequenceEndpointPolicy;
using inkpod::app::EncodeOutputColorGuardProfileSetting;
using inkpod::app::EnumerateRecoveryCandidatesInDirectory;
using inkpod::app::Generation;
using inkpod::app::ReadRecoveryMetadata;
using inkpod::app::RecoveryMetadata;
using inkpod::app::SequenceCellSwitchPolicy;
using inkpod::app::SequenceEndpointPolicy;
using inkpod::app::OutputColorGuardProfileSetting;
using inkpod::app::SequenceRecoveryPath;
using inkpod::app::WriteRecoveryMetadata;

bool WriteDummy(const std::wstring& path) {
    HANDLE file = CreateFileW(
        path.c_str(),
        GENERIC_WRITE,
        0U,
        nullptr,
        CREATE_NEW,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    constexpr std::array<std::uint8_t, 4U> bytes{1U, 2U, 3U, 4U};
    DWORD written{};
    const bool success = WriteFile(
        file,
        bytes.data(),
        static_cast<DWORD>(bytes.size()),
        &written,
        nullptr) != FALSE
        && written == bytes.size();
    CloseHandle(file);
    return success;
}

RecoveryMetadata ExampleMetadata(std::uint64_t session) {
    RecoveryMetadata metadata{};
    metadata.session = DocumentSessionId(session);
    metadata.generation = Generation(9U);
    metadata.document_uuid_high = UINT64_C(0x1111222233334444) + session;
    metadata.document_uuid_low = UINT64_C(0x5555666677778888) + session;
    metadata.original_identity.kind = DocumentIdentityKind::NormalizedPath;
    metadata.original_identity.normalized_path = L"c:\\制作\\セル "
        + std::to_wstring(session) + L".inkpod";
    metadata.original_path = L"C:\\制作\\セル " + std::to_wstring(session)
        + L".inkpod";
    metadata.source_path = L"C:\\素材\\原画.png";
    metadata.written_file_time = UINT64_C(133800000000000000) + session;
    return metadata;
}

int TestMetadataCodec() {
    const RecoveryMetadata metadata = ExampleMetadata(7U);
    std::vector<std::uint8_t> bytes;
    RecoveryMetadata decoded{};
    if (!EncodeRecoveryMetadata(metadata, bytes)
        || !DecodeRecoveryMetadata(bytes.data(), bytes.size(), decoded)
        || decoded.session != metadata.session
        || decoded.generation != metadata.generation
        || decoded.document_uuid_high != metadata.document_uuid_high
        || decoded.original_identity.kind != metadata.original_identity.kind
        || decoded.original_identity.normalized_path
            != metadata.original_identity.normalized_path
        || decoded.original_path != metadata.original_path
        || decoded.source_path != metadata.source_path) {
        return 1;
    }
    std::vector<std::uint8_t> truncated = bytes;
    truncated.pop_back();
    if (DecodeRecoveryMetadata(
            truncated.data(), truncated.size(), decoded)) {
        return 2;
    }
    std::vector<std::uint8_t> wrong_version = bytes;
    wrong_version[4] = 2U;
    if (DecodeRecoveryMetadata(
            wrong_version.data(), wrong_version.size(), decoded)) {
        return 3;
    }
    RecoveryMetadata invalid_identity = metadata;
    invalid_identity.original_identity.normalized_path.clear();
    if (EncodeRecoveryMetadata(invalid_identity, bytes)) {
        return 4;
    }
    return 0;
}

int TestSessionPathCodec() {
    const std::vector<std::wstring> paths{
        L"C:\\制作\\セル 1.inkpod", L"D:\\長い名前\\セル 2.inkpod"};
    std::vector<std::uint8_t> bytes;
    std::vector<std::wstring> decoded;
    if (!EncodePreviousDocumentPaths(paths, bytes)
        || !DecodePreviousDocumentPaths(bytes.data(), bytes.size(), decoded)
        || decoded != paths) {
        return 10;
    }
    bytes.push_back(0U);
    if (DecodePreviousDocumentPaths(bytes.data(), bytes.size(), decoded)) {
        return 11;
    }
    return 0;
}

int TestSequenceSwitchPolicyCodec() {
    std::vector<std::uint8_t> bytes;
    SequenceCellSwitchPolicy decoded{SequenceCellSwitchPolicy::Prompt};
    if (!EncodeSequenceCellSwitchPolicy(
            SequenceCellSwitchPolicy::AutosaveBeforeSwitch, bytes)
        || bytes.size() != 16U
        || !DecodeSequenceCellSwitchPolicy(bytes.data(), bytes.size(), decoded)
        || decoded != SequenceCellSwitchPolicy::AutosaveBeforeSwitch) {
        return 15;
    }
    std::vector<std::uint8_t> wrong_version = bytes;
    wrong_version[4] = 2U;
    if (DecodeSequenceCellSwitchPolicy(
            wrong_version.data(), wrong_version.size(), decoded)) {
        return 16;
    }
    bytes[12] = 3U;
    if (DecodeSequenceCellSwitchPolicy(bytes.data(), bytes.size(), decoded)
        || EncodeSequenceCellSwitchPolicy(
            static_cast<SequenceCellSwitchPolicy>(3U), bytes)) {
        return 17;
    }
    std::wstring first_path;
    std::wstring second_path;
    std::wstring repeated_path;
    if (!SequenceRecoveryPath(1U, 2U, 3U, first_path)
        || !SequenceRecoveryPath(1U, 2U, 4U, second_path)
        || !SequenceRecoveryPath(1U, 2U, 3U, repeated_path)
        || first_path.empty() || first_path == second_path
        || first_path != repeated_path
        || SequenceRecoveryPath(1U, 2U, 0U, repeated_path)
        || SequenceRecoveryPath(0U, 0U, 1U, repeated_path)) {
        return 18;
    }
    return 0;
}

int TestSequenceEndpointPolicyCodec() {
    std::vector<std::uint8_t> bytes;
    SequenceEndpointPolicy decoded{SequenceEndpointPolicy::Stop};
    if (!EncodeSequenceEndpointPolicy(SequenceEndpointPolicy::Wrap, bytes)
        || bytes.size() != 16U
        || !DecodeSequenceEndpointPolicy(bytes.data(), bytes.size(), decoded)
        || decoded != SequenceEndpointPolicy::Wrap) {
        return 30;
    }
    std::vector<std::uint8_t> wrong_version = bytes;
    wrong_version[4] = 2U;
    if (DecodeSequenceEndpointPolicy(
            wrong_version.data(), wrong_version.size(), decoded)) {
        return 31;
    }
    bytes[12] = 3U;
    if (DecodeSequenceEndpointPolicy(bytes.data(), bytes.size(), decoded)
        || EncodeSequenceEndpointPolicy(
            static_cast<SequenceEndpointPolicy>(3U), bytes)) {
        return 32;
    }
    return 0;
}

int TestOutputColorGuardProfileCodec() {
    std::vector<std::uint8_t> bytes;
    OutputColorGuardProfileSetting decoded{
        OutputColorGuardProfileSetting::Bt709ConservativeYcbcr};
    if (!EncodeOutputColorGuardProfileSetting(
            OutputColorGuardProfileSetting::Bt709ConservativeYcbcr, bytes)
        || bytes.size() != 16U
        || !DecodeOutputColorGuardProfileSetting(bytes.data(), bytes.size(), decoded)
        || decoded != OutputColorGuardProfileSetting::Bt709ConservativeYcbcr) {
        return 19;
    }
    std::vector<std::uint8_t> wrong_version = bytes;
    wrong_version[4] = 2U;
    if (DecodeOutputColorGuardProfileSetting(
            wrong_version.data(), wrong_version.size(), decoded)) {
        return 28;
    }
    bytes[12] = 2U;
    if (DecodeOutputColorGuardProfileSetting(bytes.data(), bytes.size(), decoded)
        || EncodeOutputColorGuardProfileSetting(
            static_cast<OutputColorGuardProfileSetting>(2U), bytes)) {
        return 29;
    }
    return 0;
}

int TestCatalog() {
    std::array<wchar_t, MAX_PATH> root{};
    if (GetTempPathW(static_cast<DWORD>(root.size()), root.data()) == 0U) {
        return 20;
    }
    const std::wstring directory = std::wstring(root.data())
        + L"inkpod-recovery-test-" + std::to_wstring(GetCurrentProcessId())
        + L"-" + std::to_wstring(GetTickCount64());
    if (CreateDirectoryW(directory.c_str(), nullptr) == FALSE) {
        return 21;
    }
    const std::wstring first = directory + L"\\first.inkpod";
    const std::wstring second = directory + L"\\second.inkpod";
    const std::wstring legacy = directory + L"\\legacy.inkpod";
    if (!WriteDummy(first) || !WriteDummy(second) || !WriteDummy(legacy)
        || !WriteRecoveryMetadata(first, ExampleMetadata(1U))
        || !WriteRecoveryMetadata(second, ExampleMetadata(2U))) {
        return 22;
    }
    RecoveryMetadata read{};
    if (!ReadRecoveryMetadata(first, read)
        || read.session != DocumentSessionId(1U)) {
        return 23;
    }
    std::vector<inkpod::app::RecoveryCandidate> candidates;
    if (!EnumerateRecoveryCandidatesInDirectory(directory, candidates)
        || candidates.size() != 3U) {
        return 24;
    }
    std::size_t metadata_count{};
    for (const auto& candidate : candidates) {
        metadata_count += candidate.has_metadata ? 1U : 0U;
    }
    if (metadata_count != 2U || !DiscardRecoveryArtifact(first)) {
        return 25;
    }
    if (!EnumerateRecoveryCandidatesInDirectory(directory, candidates)
        || candidates.size() != 2U) {
        return 26;
    }
    (void)DiscardRecoveryArtifact(second);
    (void)DiscardRecoveryArtifact(legacy);
    if (RemoveDirectoryW(directory.c_str()) == FALSE) {
        return 27;
    }
    return 0;
}

}  // namespace

int main() {
    const int metadata = TestMetadataCodec();
    if (metadata != 0) {
        return metadata;
    }
    const int session_paths = TestSessionPathCodec();
    if (session_paths != 0) {
        return session_paths;
    }
    const int switch_policy = TestSequenceSwitchPolicyCodec();
    if (switch_policy != 0) {
        return switch_policy;
    }
    const int endpoint_policy = TestSequenceEndpointPolicyCodec();
    if (endpoint_policy != 0) {
        return endpoint_policy;
    }
    const int output_color_guard_profile = TestOutputColorGuardProfileCodec();
    return output_color_guard_profile == 0
        ? TestCatalog()
        : output_color_guard_profile;
}
