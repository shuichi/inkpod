#include <array>
#include <string>
#include <vector>

#include "ui/batch_input_picker.h"

int wmain() {
    using inkpod::windows::ui::ParseBatchFileSelection;

    const std::array single{
        L'C', L':', L'\\', L'f', L'r', L'a', L'm', L'e', L's', L'\\',
        L'a', L'.', L'p', L'n', L'g', L'\0', L'\0'};
    std::vector<std::wstring> paths;
    if (!ParseBatchFileSelection(single, paths)
        || paths.size() != 1U
        || paths[0] != L"C:\\frames\\a.png") {
        return 1;
    }

    const std::array multiple{
        L'C', L':', L'\\', L'f', L'r', L'a', L'm', L'e', L's', L'\0',
        L'a', L'.', L'p', L'n', L'g', L'\0',
        L'b', L'.', L't', L'i', L'f', L'f', L'\0', L'\0'};
    if (!ParseBatchFileSelection(multiple, paths)
        || paths.size() != 2U
        || paths[0] != L"C:\\frames\\a.png"
        || paths[1] != L"C:\\frames\\b.tiff") {
        return 2;
    }

    const std::array malformed{
        L'C', L':', L'\\', L'a', L'.', L'p', L'n', L'g', L'\0'};
    const std::vector<std::wstring> before = paths;
    if (ParseBatchFileSelection(malformed, paths) || paths != before) {
        return 3;
    }

    const std::array empty{L'\0', L'\0'};
    if (ParseBatchFileSelection(empty, paths) || paths != before) {
        return 4;
    }

    return 0;
}
