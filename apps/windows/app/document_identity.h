#pragma once

#include <array>
#include <cstdint>
#include <string>

struct InkpodIoManager;

namespace inkpod::app {

enum class DocumentIdentityKind : std::uint8_t {
    None,
    WindowsFile,
    NormalizedPath,
    Untitled,
};

struct DocumentIdentity final {
    DocumentIdentityKind kind{DocumentIdentityKind::None};
    std::uint64_t volume_serial{};
    std::array<std::uint8_t, 16U> file_id{};
    std::wstring normalized_path;
    std::uint64_t uuid_high{};
    std::uint64_t uuid_low{};

    [[nodiscard]] explicit operator bool() const noexcept {
        return kind != DocumentIdentityKind::None;
    }
};

[[nodiscard]] bool operator==(
    const DocumentIdentity& left,
    const DocumentIdentity& right) noexcept;

// Pure Windows path normalization, without opening or querying the filesystem.
// Equivalent ordinary/verbatim drive and UNC paths share a key. Verbatim names
// requiring different Win32 parsing retain their prefix and literal components.
[[nodiscard]] bool NormalizeDocumentFilePath(
    const std::wstring& path, std::wstring& output) noexcept;

[[nodiscard]] bool ResolveDocumentFileIdentity(
    const InkpodIoManager* manager,
    const std::wstring& path,
    DocumentIdentity& output) noexcept;

[[nodiscard]] DocumentIdentity UntitledDocumentIdentity(
    std::uint64_t uuid_high,
    std::uint64_t uuid_low) noexcept;

}  // namespace inkpod::app
