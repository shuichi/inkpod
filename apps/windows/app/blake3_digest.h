#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <span>

namespace inkpod::app {

// Computes the unkeyed BLAKE3-256 digest used by the Rust InkScript runner for
// exact native-byte fingerprints. This is a frontend byte-integrity primitive;
// it does not interpret the native document format.
[[nodiscard]] std::array<std::uint8_t, 32U> Blake3Digest(
    std::span<const std::uint8_t> bytes) noexcept;

}  // namespace inkpod::app
