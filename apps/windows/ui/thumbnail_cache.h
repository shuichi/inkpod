#pragma once

#include <compare>
#include <cstdint>
#include <list>
#include <span>
#include <unordered_map>
#include <vector>

#include "app/identity.h"

namespace inkpod::windows::ui {

enum class ThumbnailPixelLayout : std::uint8_t {
    Rgba8,
    Bgra8,
};

enum class ThumbnailKind : std::uint8_t {
    Layer,
    Sequence,
};

// A pane never identifies cached image data by an array position. The key
// retains the exact frontend document namespace and the Core-provided content
// revision/checksum captured when the thumbnail was built.
struct ThumbnailCacheKey final {
    app::PaneInstanceId pane{};
    app::DocumentSessionId document{};
    app::Generation document_generation{};
    std::uint64_t content_id{};
    std::uint64_t content_revision{};
    ThumbnailKind kind{ThumbnailKind::Layer};

    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        return static_cast<bool>(pane) && static_cast<bool>(document)
            && static_cast<bool>(document_generation) && content_id != 0U
            && content_revision != 0U;
    }

    constexpr auto operator<=>(const ThumbnailCacheKey&) const noexcept = default;
};

// Pixels remain borrowed only until the next mutating ThumbnailCache call.
// All callers and the cache itself are confined to the UI/Input thread.
struct ThumbnailImageView final {
    std::uint32_t width{};
    std::uint32_t height{};
    std::uint32_t stride_bytes{};
    ThumbnailPixelLayout layout{ThumbnailPixelLayout::Rgba8};
    std::span<const std::uint8_t> pixels{};
};

struct ThumbnailCacheUsage final {
    std::uint64_t budget_bytes{};
    std::uint64_t resident_bytes{};
    std::uint64_t entry_count{};
    std::uint64_t hit_count{};
    std::uint64_t miss_count{};
    std::uint64_t eviction_count{};
    std::uint64_t rejection_count{};
};

struct ThumbnailPaneUsage final {
    app::PaneInstanceId pane{};
    std::uint64_t resident_bytes{};
    std::uint64_t entry_count{};
};

// Process-wide, UI-thread-owned thumbnail cache. Entries across every
// workspace compete for one bounded budget; Get promotes an entry and eviction
// always removes the least recently used entry regardless of its source pane.
class ThumbnailCache final {
public:
    static constexpr std::uint64_t kDefaultBudgetBytes = 64U * 1024U * 1024U;
    static constexpr std::uint64_t kMaximumBudgetBytes = 256U * 1024U * 1024U;

    ThumbnailCache() noexcept;

    [[nodiscard]] bool Put(
        const ThumbnailCacheKey& key,
        std::uint32_t width,
        std::uint32_t height,
        std::uint32_t stride_bytes,
        ThumbnailPixelLayout layout,
        std::vector<std::uint8_t> pixels) noexcept;
    [[nodiscard]] bool Get(
        const ThumbnailCacheKey& key,
        ThumbnailImageView& image) noexcept;
    [[nodiscard]] bool Peek(
        const ThumbnailCacheKey& key,
        ThumbnailImageView& image) const noexcept;

    void RemovePane(app::PaneInstanceId pane) noexcept;
    void RemoveDocument(
        app::DocumentSessionId document,
        app::Generation generation) noexcept;
    void Clear() noexcept;

    [[nodiscard]] ThumbnailCacheUsage Usage() const noexcept;
    [[nodiscard]] bool GetPaneUsage(
        app::PaneInstanceId pane,
        ThumbnailPaneUsage& usage) const noexcept;
    [[nodiscard]] bool SetBudgetBytes(std::uint64_t budget_bytes) noexcept;

private:
    struct Entry final {
        ThumbnailCacheKey key{};
        std::uint32_t width{};
        std::uint32_t height{};
        std::uint32_t stride_bytes{};
        ThumbnailPixelLayout layout{ThumbnailPixelLayout::Rgba8};
        std::vector<std::uint8_t> pixels;
    };

    struct KeyHash final {
        [[nodiscard]] std::size_t operator()(
            const ThumbnailCacheKey& key) const noexcept;
    };

    using EntryList = std::list<Entry>;
    using EntryIterator = EntryList::iterator;
    using EntryIndex = std::unordered_map<
        ThumbnailCacheKey,
        EntryIterator,
        KeyHash>;

    void Erase(EntryIterator entry, bool eviction) noexcept;
    void EvictToBudget() noexcept;
    [[nodiscard]] static bool ValidImage(
        const ThumbnailCacheKey& key,
        std::uint32_t width,
        std::uint32_t height,
        std::uint32_t stride_bytes,
        std::size_t pixel_bytes) noexcept;

    EntryList entries_;
    EntryIndex index_;
    std::uint64_t budget_bytes_{kDefaultBudgetBytes};
    std::uint64_t resident_bytes_{};
    std::uint64_t hit_count_{};
    std::uint64_t miss_count_{};
    std::uint64_t eviction_count_{};
    std::uint64_t rejection_count_{};
};

}  // namespace inkpod::windows::ui
