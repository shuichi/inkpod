#include "blake3_digest.h"

#include <algorithm>
#include <array>
#include <bit>
#include <cstring>

namespace inkpod::app {
namespace {

constexpr std::size_t block_bytes = 64U;
constexpr std::size_t chunk_bytes = 1024U;
constexpr std::uint32_t flag_chunk_start = 1U << 0U;
constexpr std::uint32_t flag_chunk_end = 1U << 1U;
constexpr std::uint32_t flag_parent = 1U << 2U;
constexpr std::uint32_t flag_root = 1U << 3U;

constexpr std::array<std::uint32_t, 8U> initial_vector{
    0x6a09e667U,
    0xbb67ae85U,
    0x3c6ef372U,
    0xa54ff53aU,
    0x510e527fU,
    0x9b05688cU,
    0x1f83d9abU,
    0x5be0cd19U};

constexpr std::array<std::uint8_t, 16U> message_permutation{
    2U, 6U, 3U, 10U, 7U, 0U, 4U, 13U,
    1U, 11U, 12U, 5U, 9U, 14U, 15U, 8U};

using ChainingValue = std::array<std::uint32_t, 8U>;
using BlockWords = std::array<std::uint32_t, 16U>;
using CompressionWords = std::array<std::uint32_t, 16U>;

struct Output final {
    ChainingValue input_chaining_value{};
    BlockWords block_words{};
    std::uint64_t counter{};
    std::uint32_t block_length{};
    std::uint32_t flags{};
};

void Mix(
    CompressionWords& state,
    std::size_t a,
    std::size_t b,
    std::size_t c,
    std::size_t d,
    std::uint32_t x,
    std::uint32_t y) noexcept {
    state[a] = state[a] + state[b] + x;
    state[d] = std::rotr(state[d] ^ state[a], 16);
    state[c] += state[d];
    state[b] = std::rotr(state[b] ^ state[c], 12);
    state[a] = state[a] + state[b] + y;
    state[d] = std::rotr(state[d] ^ state[a], 8);
    state[c] += state[d];
    state[b] = std::rotr(state[b] ^ state[c], 7);
}

void Round(
    CompressionWords& state,
    const BlockWords& message,
    const std::array<std::uint8_t, 16U>& schedule) noexcept {
    Mix(state, 0U, 4U, 8U, 12U, message[schedule[0U]], message[schedule[1U]]);
    Mix(state, 1U, 5U, 9U, 13U, message[schedule[2U]], message[schedule[3U]]);
    Mix(state, 2U, 6U, 10U, 14U, message[schedule[4U]], message[schedule[5U]]);
    Mix(state, 3U, 7U, 11U, 15U, message[schedule[6U]], message[schedule[7U]]);
    Mix(state, 0U, 5U, 10U, 15U, message[schedule[8U]], message[schedule[9U]]);
    Mix(state, 1U, 6U, 11U, 12U, message[schedule[10U]], message[schedule[11U]]);
    Mix(state, 2U, 7U, 8U, 13U, message[schedule[12U]], message[schedule[13U]]);
    Mix(state, 3U, 4U, 9U, 14U, message[schedule[14U]], message[schedule[15U]]);
}

CompressionWords Compress(
    const ChainingValue& chaining_value,
    const BlockWords& block,
    std::uint64_t counter,
    std::uint32_t block_length,
    std::uint32_t flags) noexcept {
    CompressionWords state{};
    std::copy(chaining_value.begin(), chaining_value.end(), state.begin());
    std::copy_n(initial_vector.begin(), 4U, state.begin() + 8U);
    state[12U] = static_cast<std::uint32_t>(counter);
    state[13U] = static_cast<std::uint32_t>(counter >> 32U);
    state[14U] = block_length;
    state[15U] = flags;

    std::array<std::uint8_t, 16U> schedule{};
    for (std::size_t index = 0; index < schedule.size(); ++index) {
        schedule[index] = static_cast<std::uint8_t>(index);
    }
    for (std::size_t round = 0; round < 7U; ++round) {
        Round(state, block, schedule);
        std::array<std::uint8_t, 16U> next{};
        for (std::size_t index = 0; index < schedule.size(); ++index) {
            next[index] = schedule[message_permutation[index]];
        }
        schedule = next;
    }
    for (std::size_t index = 0; index < 8U; ++index) {
        state[index] ^= state[index + 8U];
        state[index + 8U] ^= chaining_value[index];
    }
    return state;
}

BlockWords LoadBlock(std::span<const std::uint8_t> bytes) noexcept {
    std::array<std::uint8_t, block_bytes> padded{};
    if (!bytes.empty()) {
        std::memcpy(padded.data(), bytes.data(), bytes.size());
    }
    BlockWords words{};
    for (std::size_t index = 0; index < words.size(); ++index) {
        const std::size_t offset = index * 4U;
        words[index] = static_cast<std::uint32_t>(padded[offset])
            | (static_cast<std::uint32_t>(padded[offset + 1U]) << 8U)
            | (static_cast<std::uint32_t>(padded[offset + 2U]) << 16U)
            | (static_cast<std::uint32_t>(padded[offset + 3U]) << 24U);
    }
    return words;
}

ChainingValue ChainingValueOf(const Output& output) noexcept {
    const CompressionWords words = Compress(
        output.input_chaining_value,
        output.block_words,
        output.counter,
        output.block_length,
        output.flags);
    ChainingValue result{};
    std::copy_n(words.begin(), result.size(), result.begin());
    return result;
}

Output ChunkOutput(
    std::span<const std::uint8_t> chunk,
    std::uint64_t chunk_counter) noexcept {
    ChainingValue chaining_value = initial_vector;
    const std::size_t block_count = std::max<std::size_t>(
        1U, (chunk.size() + block_bytes - 1U) / block_bytes);
    for (std::size_t block_index = 0; block_index + 1U < block_count; ++block_index) {
        const BlockWords block = LoadBlock(
            chunk.subspan(block_index * block_bytes, block_bytes));
        const CompressionWords words = Compress(
            chaining_value,
            block,
            chunk_counter,
            static_cast<std::uint32_t>(block_bytes),
            block_index == 0U ? flag_chunk_start : 0U);
        std::copy_n(words.begin(), chaining_value.size(), chaining_value.begin());
    }
    const std::size_t final_offset = (block_count - 1U) * block_bytes;
    const std::size_t final_length = chunk.size() - final_offset;
    return Output{
        chaining_value,
        LoadBlock(chunk.subspan(final_offset, final_length)),
        chunk_counter,
        static_cast<std::uint32_t>(final_length),
        flag_chunk_end | (block_count == 1U ? flag_chunk_start : 0U)};
}

Output ParentOutput(
    const ChainingValue& left,
    const ChainingValue& right) noexcept {
    BlockWords block{};
    std::copy(left.begin(), left.end(), block.begin());
    std::copy(right.begin(), right.end(), block.begin() + left.size());
    return Output{
        initial_vector,
        block,
        0U,
        static_cast<std::uint32_t>(block_bytes),
        flag_parent};
}

std::size_t LeftSubtreeChunks(std::size_t chunk_count) noexcept {
    const std::size_t highest = std::bit_floor(chunk_count);
    return highest == chunk_count ? highest / 2U : highest;
}

Output SubtreeOutput(
    std::span<const std::uint8_t> bytes,
    std::size_t first_chunk,
    std::size_t chunk_count) noexcept {
    if (chunk_count == 1U) {
        const std::size_t offset = first_chunk * chunk_bytes;
        const std::size_t length = std::min(chunk_bytes, bytes.size() - offset);
        return ChunkOutput(
            bytes.subspan(offset, length),
            static_cast<std::uint64_t>(first_chunk));
    }
    const std::size_t left_chunks = LeftSubtreeChunks(chunk_count);
    const Output left = SubtreeOutput(bytes, first_chunk, left_chunks);
    const Output right = SubtreeOutput(
        bytes,
        first_chunk + left_chunks,
        chunk_count - left_chunks);
    return ParentOutput(ChainingValueOf(left), ChainingValueOf(right));
}

}  // namespace

std::array<std::uint8_t, 32U> Blake3Digest(
    std::span<const std::uint8_t> bytes) noexcept {
    const std::size_t chunk_count = std::max<std::size_t>(
        1U, (bytes.size() + chunk_bytes - 1U) / chunk_bytes);
    const Output root = SubtreeOutput(bytes, 0U, chunk_count);
    const CompressionWords words = Compress(
        root.input_chaining_value,
        root.block_words,
        0U,
        root.block_length,
        root.flags | flag_root);
    std::array<std::uint8_t, 32U> result{};
    for (std::size_t index = 0; index < 8U; ++index) {
        result[index * 4U] = static_cast<std::uint8_t>(words[index]);
        result[index * 4U + 1U] = static_cast<std::uint8_t>(words[index] >> 8U);
        result[index * 4U + 2U] = static_cast<std::uint8_t>(words[index] >> 16U);
        result[index * 4U + 3U] = static_cast<std::uint8_t>(words[index] >> 24U);
    }
    return result;
}

}  // namespace inkpod::app
