#include "ui/ui_resources.h"

#include <algorithm>
#include <cstddef>
#include <cstring>
#include <limits>

#include "ui/localization.h"

namespace inkpod::windows::ui {
namespace {

[[nodiscard]] const void* LoadLocalizedResource(
    HINSTANCE instance,
    LPCWSTR type,
    LPCWSTR name,
    DWORD& byte_count) noexcept {
    byte_count = 0U;
    if (instance == nullptr || type == nullptr || name == nullptr) {
        return nullptr;
    }
    const HRSRC resource = FindResourceExW(
        instance, type, name, CurrentUiResourceLanguageId());
    if (resource == nullptr) {
        return nullptr;
    }
    const DWORD size = SizeofResource(instance, resource);
    const HGLOBAL loaded = LoadResource(instance, resource);
    const void* bytes = loaded == nullptr ? nullptr : LockResource(loaded);
    if (bytes == nullptr || size == 0U) {
        return nullptr;
    }
    byte_count = size;
    return bytes;
}

[[nodiscard]] const DLGTEMPLATE* LoadDialogTemplate(
    HINSTANCE instance, LPCWSTR template_name) noexcept {
    DWORD byte_count{};
    const void* bytes = LoadLocalizedResource(
        instance, RT_DIALOG, template_name, byte_count);
    return bytes == nullptr || byte_count < sizeof(DLGTEMPLATE)
        ? nullptr
        : static_cast<const DLGTEMPLATE*>(bytes);
}

}  // namespace

int LoadLocalizedStringW(
    HINSTANCE instance,
    UINT identifier,
    wchar_t* buffer,
    int buffer_count) noexcept {
    if (buffer == nullptr || buffer_count <= 0) {
        return 0;
    }
    buffer[0] = L'\0';
    const UINT block = identifier / 16U + 1U;
    const UINT offset = identifier % 16U;
    DWORD byte_count{};
    const void* resource = LoadLocalizedResource(
        instance, RT_STRING, MAKEINTRESOURCEW(block), byte_count);
    if (resource == nullptr || byte_count % sizeof(wchar_t) != 0U) {
        return 0;
    }
    const auto* cursor = static_cast<const wchar_t*>(resource);
    const auto* end = cursor + byte_count / sizeof(wchar_t);
    for (UINT index = 0U; index <= offset; ++index) {
        if (cursor == end) {
            return 0;
        }
        const std::size_t length = static_cast<std::uint16_t>(*cursor++);
        if (length > static_cast<std::size_t>(end - cursor)) {
            return 0;
        }
        if (index == offset) {
            const std::size_t copied = std::min(
                length, static_cast<std::size_t>(buffer_count - 1));
            if (copied != 0U) {
                std::memcpy(buffer, cursor, copied * sizeof(wchar_t));
            }
            buffer[copied] = L'\0';
            return copied > static_cast<std::size_t>(
                       (std::numeric_limits<int>::max)())
                ? 0
                : static_cast<int>(copied);
        }
        cursor += length;
    }
    return 0;
}

HMENU LoadLocalizedMenuW(
    HINSTANCE instance, LPCWSTR resource_name) noexcept {
    DWORD byte_count{};
    const void* resource = LoadLocalizedResource(
        instance, RT_MENU, resource_name, byte_count);
    if (resource == nullptr || byte_count < sizeof(WORD)) {
        return nullptr;
    }
    return LoadMenuIndirectW(resource);
}

HWND CreateLocalizedDialogParamW(
    HINSTANCE instance,
    LPCWSTR template_name,
    HWND owner,
    DLGPROC procedure,
    LPARAM parameter) noexcept {
    const DLGTEMPLATE* dialog = LoadDialogTemplate(instance, template_name);
    return dialog == nullptr
        ? nullptr
        : CreateDialogIndirectParamW(
              instance, dialog, owner, procedure, parameter);
}

INT_PTR DialogBoxLocalizedParamW(
    HINSTANCE instance,
    LPCWSTR template_name,
    HWND owner,
    DLGPROC procedure,
    LPARAM parameter) noexcept {
    const DLGTEMPLATE* dialog = LoadDialogTemplate(instance, template_name);
    return dialog == nullptr
        ? -1
        : DialogBoxIndirectParamW(
              instance, dialog, owner, procedure, parameter);
}

}  // namespace inkpod::windows::ui
