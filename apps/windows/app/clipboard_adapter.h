#pragma once

#include <windows.h>

#include "inkpod/core_ffi.h"

namespace inkpod::app {

UINT InkpodClipboardFormat() noexcept;

// Publishes both Inkpod's coordinate-preserving private payload and a standard
// Windows DIB. Ownership of successful SetClipboardData allocations transfers
// to Windows; the caller retains ownership of the Rust clipboard handle.
bool PublishStandardClipboard(HWND owner, const InkpodClipboard* clipboard) noexcept;

// Replaces output only after a private payload or supported Windows DIB has
// been validated and copied into a Rust-owned clipboard handle.
bool ImportStandardClipboard(HWND owner, InkpodClipboard*& output) noexcept;

} // namespace inkpod::app
