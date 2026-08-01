#include "recent_documents.h"

#include <algorithm>
#include <utility>

namespace inkpod::app {

bool RecentDocumentList::Record(
    std::wstring path,
    DocumentIdentity identity) noexcept {
    if (path.empty() || !identity) {
        return false;
    }
    RecentDocumentEntry candidate{
        std::move(path), std::move(identity)};
    const auto end = entries_.begin() + count_;
    const auto existing = std::find_if(
        entries_.begin(), end, [&candidate](const RecentDocumentEntry& entry) {
            return entry.identity == candidate.identity;
        });
    if (existing != end) {
        std::move(existing + 1, end, existing);
        --count_;
        entries_[count_] = {};
    } else if (count_ == entries_.size()) {
        --count_;
        entries_[count_] = {};
    }
    std::move_backward(
        entries_.begin(),
        entries_.begin() + count_,
        entries_.begin() + count_ + 1U);
    entries_[0] = std::move(candidate);
    ++count_;
    return true;
}

bool RecentDocumentList::Remove(std::size_t index) noexcept {
    if (index >= count_) {
        return false;
    }
    std::move(
        entries_.begin() + static_cast<std::ptrdiff_t>(index + 1U),
        entries_.begin() + static_cast<std::ptrdiff_t>(count_),
        entries_.begin() + static_cast<std::ptrdiff_t>(index));
    --count_;
    entries_[count_] = {};
    return true;
}

void RecentDocumentList::Clear() noexcept {
    for (std::size_t index = 0U; index < count_; ++index) {
        entries_[index] = {};
    }
    count_ = 0U;
}

const RecentDocumentEntry* RecentDocumentList::At(
    std::size_t index) const noexcept {
    return index < count_ ? &entries_[index] : nullptr;
}

std::size_t RecentDocumentList::Count() const noexcept {
    return count_;
}

}  // namespace inkpod::app
