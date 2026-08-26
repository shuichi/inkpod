#pragma once

#include <string>
#include <string_view>
#include <vector>

namespace inkpod::windows::ui {

inline constexpr std::wstring_view kBatchSetExtension = L".inkbatch";

// Resolves and creates %LOCALAPPDATA%\inkpod\batch-sets. The returned path never
// contains a batch-set filename.
bool PrepareDefaultBatchSetDirectory(std::wstring& directory) noexcept;

// Creates an explicitly supplied batch-set directory. This entry point keeps
// path handling testable without writing to the user's application data.
bool EnsureBatchSetDirectory(const std::wstring& directory) noexcept;

// Returns extension-free set names for .inkbatch files only. Version checking
// remains the responsibility of the graph decoder when a listed set is loaded.
bool EnumerateBatchSetNames(
    const std::wstring& directory,
    std::vector<std::wstring>& names) noexcept;

// Validates an editable set name and joins it to directory. An optional
// .inkbatch suffix is accepted, but the resulting path always has one suffix.
bool ResolveBatchSetPath(
    const std::wstring& directory,
    std::wstring_view requested_name,
    std::wstring& path,
    std::wstring* canonical_name = nullptr) noexcept;

} // namespace inkpod::windows::ui
