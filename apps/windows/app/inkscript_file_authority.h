#pragma once

#include <cstdint>
#include <memory>
#include <string>

#include "inkpod/core_ffi.h"

namespace inkpod::app {

// Owner-thread Windows implementation of the exact-current InkScript host callbacks.
// Construct, call, and destroy this object on the same thread. The adapter record
// borrows this object as its context and must not outlive it. Windows handles and
// reparse metadata remain hidden behind the pimpl and are never copied into the
// Rust-owned path/authority DTOs.
class InkScriptFileAuthorityAdapter final {
public:
    InkScriptFileAuthorityAdapter() noexcept;
    ~InkScriptFileAuthorityAdapter();

    InkScriptFileAuthorityAdapter(const InkScriptFileAuthorityAdapter&) = delete;
    InkScriptFileAuthorityAdapter& operator=(
        const InkScriptFileAuthorityAdapter&) = delete;

    [[nodiscard]] InkpodInkScriptHostAdapter HostAdapterRecord() noexcept;

    [[nodiscard]] InkpodStatus AuthorizePath(
        std::uint64_t intent_id,
        std::uint32_t access,
        const std::wstring& path,
        InkpodInkScriptAuthorityGrant& output) noexcept;

    [[nodiscard]] InkpodStatus RevokePathAuthority(
        std::uint64_t intent_id) noexcept;

    [[nodiscard]] InkpodStatus AuthorizeAsset(
        const std::string& symbol,
        const std::wstring& path) noexcept;

    [[nodiscard]] InkpodStatus RegisterOpenSession(
        std::uint64_t session_id,
        std::uint64_t session_generation,
        std::uint64_t document_uuid_high,
        std::uint64_t document_uuid_low,
        const std::wstring& backing_path) noexcept;

    [[nodiscard]] InkpodStatus UnregisterOpenSession(
        std::uint64_t session_id,
        std::uint64_t session_generation) noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

}  // namespace inkpod::app
