#pragma once

#include <array>
#include <cstddef>
#include <string>

#include "document_identity.h"

namespace inkpod::app {

struct RecentDocumentEntry final {
    std::wstring path;
    DocumentIdentity identity;
};

class RecentDocumentList final {
public:
    static constexpr std::size_t kCapacity = 8U;

    [[nodiscard]] bool Record(
        std::wstring path,
        DocumentIdentity identity) noexcept;
    // Replaces either the final physical identity or its issue-time logical
    // identity without allocating after a durable SavePair commit.
    [[nodiscard]] bool RecordReplacing(
        std::wstring path,
        DocumentIdentity identity,
        const DocumentIdentity& previous_identity) noexcept;
    [[nodiscard]] bool Remove(std::size_t index) noexcept;
    void Clear() noexcept;
    [[nodiscard]] const RecentDocumentEntry* At(
        std::size_t index) const noexcept;
    [[nodiscard]] std::size_t Count() const noexcept;

private:
    std::array<RecentDocumentEntry, kCapacity> entries_{};
    std::size_t count_{};
};

}  // namespace inkpod::app
