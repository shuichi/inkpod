#pragma once

#include <cstdint>
#include <string>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::app {

// Engine-thread owner of the shared Rust I/O adapter. Only explicitly approved
// Windows paths are converted here; Rust owns identity, guards and publication.
class InkScriptFileAuthorityAdapter final {
public:
    InkScriptFileAuthorityAdapter() noexcept = default;
    ~InkScriptFileAuthorityAdapter();
    InkScriptFileAuthorityAdapter(const InkScriptFileAuthorityAdapter&) = delete;
    InkScriptFileAuthorityAdapter& operator=(const InkScriptFileAuthorityAdapter&) = delete;

    [[nodiscard]] InkpodStatus Initialize(
        InkpodCore* core, InkpodIoManager* manager,
        const InkpodInkScriptProgram* program,
        const std::vector<std::wstring>& approved_paths,
        std::uint64_t new_tab_capacity) noexcept;
    [[nodiscard]] InkpodInkScriptIo* Handle() const noexcept { return io_; }
    [[nodiscard]] static InkpodStatus PathUtf8(
        const std::wstring& path, std::string& output) noexcept;

private:
    InkpodCore* core_{};
    InkpodInkScriptIo* io_{};
};

}  // namespace inkpod::app
