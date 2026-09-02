#include "recent_documents.h"

#include <algorithm>
#include <utility>

namespace inkpod::app {

bool RecentDocumentList::Record(
    std::wstring path,
    DocumentIdentity identity) noexcept {
    return RecordReplacing(std::move(path), std::move(identity), {});
}

bool RecentDocumentList::RecordReplacing(
    std::wstring path,
    DocumentIdentity identity,
    const DocumentIdentity& previous_identity) noexcept {
    if (path.empty() || !identity) {
        return false;
    }
    RecentDocumentEntry candidate{
        std::move(path), std::move(identity)};
    std::size_t kept{};
    for (std::size_t index = 0U; index < count_; ++index) {
        const bool replaced = entries_[index].identity == candidate.identity
            || (previous_identity
                && entries_[index].identity == previous_identity);
        if (!replaced) {
            if (kept != index) {
                entries_[kept] = std::move(entries_[index]);
            }
            ++kept;
        }
    }
    for (std::size_t index = kept; index < count_; ++index) {
        entries_[index] = {};
    }
    count_ = kept;
    if (count_ == entries_.size()) {
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
