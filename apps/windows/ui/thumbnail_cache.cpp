#include "thumbnail_cache.h"

#include <functional>
#include <limits>
#include <new>
#include <utility>

namespace inkpod::windows::ui {
namespace {

void HashCombine(std::size_t& seed, std::uint64_t value) noexcept {
    const std::size_t hashed = std::hash<std::uint64_t>{}(value);
    seed ^= hashed + static_cast<std::size_t>(0x9e3779b9U) + (seed << 6U)
        + (seed >> 2U);
}

}  // namespace

ThumbnailCache::ThumbnailCache() noexcept {
    try {
        index_.reserve(256U);
    } catch (const std::bad_alloc&) {
        // A later Put can still grow the index or fail without affecting UI
        // ownership. Construction itself remains noexcept.
    }
}

std::size_t ThumbnailCache::KeyHash::operator()(
    const ThumbnailCacheKey& key) const noexcept {
    std::size_t seed{};
    HashCombine(seed, key.pane.Value());
    HashCombine(seed, key.document.Value());
    HashCombine(seed, key.document_generation.Value());
    HashCombine(seed, key.content_id);
    HashCombine(seed, key.content_revision);
    HashCombine(seed, static_cast<std::uint64_t>(key.kind));
    return seed;
}

bool ThumbnailCache::ValidImage(
    const ThumbnailCacheKey& key,
    std::uint32_t width,
    std::uint32_t height,
    std::uint32_t stride_bytes,
    std::size_t pixel_bytes) noexcept {
    if (!key || width == 0U || height == 0U || width > 256U || height > 256U) {
        return false;
    }
    const std::uint64_t expected_stride = static_cast<std::uint64_t>(width) * 4U;
    if (expected_stride != stride_bytes) {
        return false;
    }
    const std::uint64_t expected_bytes = expected_stride * height;
    return expected_bytes <= std::numeric_limits<std::size_t>::max()
        && static_cast<std::size_t>(expected_bytes) == pixel_bytes;
}

bool ThumbnailCache::Put(
    const ThumbnailCacheKey& key,
    std::uint32_t width,
    std::uint32_t height,
    std::uint32_t stride_bytes,
    ThumbnailPixelLayout layout,
    std::vector<std::uint8_t> pixels) noexcept {
    if (!ValidImage(key, width, height, stride_bytes, pixels.size())
        || pixels.size() > budget_bytes_) {
        ++rejection_count_;
        return false;
    }

    const auto existing = index_.find(key);
    if (existing != index_.end()) {
        Entry& entry = *existing->second;
        if (entry.width == width && entry.height == height
            && entry.stride_bytes == stride_bytes && entry.layout == layout
            && entry.pixels == pixels) {
            entries_.splice(entries_.begin(), entries_, existing->second);
            return true;
        }
        Erase(existing->second, false);
    }

    try {
        entries_.push_front(Entry{
            key,
            width,
            height,
            stride_bytes,
            layout,
            std::move(pixels)});
        try {
            const auto [inserted, created] = index_.emplace(key, entries_.begin());
            if (!created) {
                entries_.pop_front();
                ++rejection_count_;
                return false;
            }
            static_cast<void>(inserted);
        } catch (const std::bad_alloc&) {
            entries_.pop_front();
            ++rejection_count_;
            return false;
        }
    } catch (const std::bad_alloc&) {
        ++rejection_count_;
        return false;
    }

    resident_bytes_ += entries_.front().pixels.size();
    EvictToBudget();
    return index_.contains(key);
}

bool ThumbnailCache::Get(
    const ThumbnailCacheKey& key,
    ThumbnailImageView& image) noexcept {
    image = {};
    const auto found = index_.find(key);
    if (found == index_.end()) {
        ++miss_count_;
        return false;
    }
    entries_.splice(entries_.begin(), entries_, found->second);
    const Entry& entry = entries_.front();
    image = ThumbnailImageView{
        entry.width,
        entry.height,
        entry.stride_bytes,
        entry.layout,
        std::span<const std::uint8_t>(entry.pixels)};
    ++hit_count_;
    return true;
}

bool ThumbnailCache::Peek(
    const ThumbnailCacheKey& key,
    ThumbnailImageView& image) const noexcept {
    image = {};
    const auto found = index_.find(key);
    if (found == index_.end()) {
        return false;
    }
    const Entry& entry = *found->second;
    image = ThumbnailImageView{
        entry.width,
        entry.height,
        entry.stride_bytes,
        entry.layout,
        std::span<const std::uint8_t>(entry.pixels)};
    return true;
}

void ThumbnailCache::Erase(EntryIterator entry, bool eviction) noexcept {
    if (entry == entries_.end()) {
        return;
    }
    const std::uint64_t bytes = entry->pixels.size();
    AdvanceInvalidationGeneration(entry->key.kind);
    index_.erase(entry->key);
    entries_.erase(entry);
    resident_bytes_ = bytes > resident_bytes_ ? 0U : resident_bytes_ - bytes;
    if (eviction) {
        ++eviction_count_;
    }
}

void ThumbnailCache::EvictToBudget() noexcept {
    while (resident_bytes_ > budget_bytes_ && !entries_.empty()) {
        Erase(std::prev(entries_.end()), true);
    }
}

void ThumbnailCache::RemovePane(app::PaneInstanceId pane) noexcept {
    if (!pane) {
        return;
    }
    for (auto entry = entries_.begin(); entry != entries_.end();) {
        if (entry->key.pane == pane) {
            const auto removing = entry++;
            Erase(removing, false);
        } else {
            ++entry;
        }
    }
}

void ThumbnailCache::RemoveDocument(
    app::DocumentSessionId document,
    app::Generation generation,
    std::optional<ThumbnailKind> kind) noexcept {
    if (!document || !generation) {
        return;
    }
    for (auto entry = entries_.begin(); entry != entries_.end();) {
        if (entry->key.document == document
            && entry->key.document_generation == generation
            && (!kind.has_value() || entry->key.kind == kind.value())) {
            const auto removing = entry++;
            Erase(removing, false);
        } else {
            ++entry;
        }
    }
}

void ThumbnailCache::Clear() noexcept {
    if (!entries_.empty()) {
        AdvanceInvalidationGeneration(ThumbnailKind::Layer);
        AdvanceInvalidationGeneration(ThumbnailKind::Sequence);
    }
    entries_.clear();
    index_.clear();
    resident_bytes_ = 0U;
}

ThumbnailCacheUsage ThumbnailCache::Usage() const noexcept {
    return ThumbnailCacheUsage{
        budget_bytes_,
        resident_bytes_,
        static_cast<std::uint64_t>(entries_.size()),
        hit_count_,
        miss_count_,
        eviction_count_,
        rejection_count_};
}

std::uint64_t ThumbnailCache::InvalidationGeneration(ThumbnailKind kind) const noexcept {
    switch (kind) {
        case ThumbnailKind::Layer:
            return layer_invalidation_generation_;
        case ThumbnailKind::Sequence:
            return sequence_invalidation_generation_;
    }
    return 0U;
}

void ThumbnailCache::AdvanceInvalidationGeneration(ThumbnailKind kind) noexcept {
    std::uint64_t* generation{};
    switch (kind) {
        case ThumbnailKind::Layer:
            generation = &layer_invalidation_generation_;
            break;
        case ThumbnailKind::Sequence:
            generation = &sequence_invalidation_generation_;
            break;
    }
    if (generation != nullptr && *generation != 0U) {
        // Unsigned wrap reaches the permanently disabled zero value.
        ++*generation;
    }
}

bool ThumbnailCache::GetPaneUsage(
    app::PaneInstanceId pane,
    ThumbnailPaneUsage& usage) const noexcept {
    usage = {};
    if (!pane) {
        return false;
    }
    usage.pane = pane;
    for (const Entry& entry : entries_) {
        if (entry.key.pane == pane) {
            usage.resident_bytes += entry.pixels.size();
            ++usage.entry_count;
        }
    }
    return true;
}

bool ThumbnailCache::SetBudgetBytes(std::uint64_t budget_bytes) noexcept {
    if (budget_bytes < 4U || budget_bytes > kMaximumBudgetBytes) {
        return false;
    }
    budget_bytes_ = budget_bytes;
    EvictToBudget();
    return true;
}

}  // namespace inkpod::windows::ui
