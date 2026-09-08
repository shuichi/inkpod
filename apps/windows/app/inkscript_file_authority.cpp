#include "inkscript_file_authority.h"

#include <windows.h>

#include <limits>
#include <new>

namespace inkpod::app {

InkScriptFileAuthorityAdapter::~InkScriptFileAuthorityAdapter() {
    if (io_ != nullptr) {
        (void)inkpod_core_inkscript_io_release(core_, &io_);
    }
}

InkpodStatus InkScriptFileAuthorityAdapter::PathUtf8(
    const std::wstring& path, std::string& output) noexcept {
    if (path.empty() || path.size() > static_cast<std::size_t>(INT_MAX)
        || path.find(L'\0') != std::wstring::npos) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const int count = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS,
        path.data(), static_cast<int>(path.size()), nullptr, 0, nullptr, nullptr);
    if (count <= 0) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    try {
        output.resize(static_cast<std::size_t>(count));
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS,
        path.data(), static_cast<int>(path.size()), output.data(), count,
        nullptr, nullptr) == count ? INKPOD_STATUS_OK : INKPOD_STATUS_INVALID_ARGUMENT;
}

InkpodStatus InkScriptFileAuthorityAdapter::Initialize(
    InkpodCore* core, InkpodIoManager* manager,
    const InkpodInkScriptProgram* program,
    const std::vector<std::wstring>& approved_paths,
    std::uint64_t new_tab_capacity) noexcept {
    if (core == nullptr || program == nullptr || io_ != nullptr) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    try {
        InkpodInkScriptPathIntentBuffer query{};
        query.struct_size = sizeof(query);
        query.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        InkpodStatus status = inkpod_core_inkscript_program_path_intents_copy(
            core, program, &query);
        if ((status != INKPOD_STATUS_OK && status != INKPOD_STATUS_BUFFER_TOO_SMALL)
            || query.required_records != approved_paths.size()
            || query.required_records > 65536U
            || query.required_utf8_bytes > UINT64_C(16) * 1024U * 1024U) {
            return status == INKPOD_STATUS_OK || status == INKPOD_STATUS_BUFFER_TOO_SMALL
                ? INKPOD_STATUS_INVALID_ARGUMENT : status;
        }
        const InkpodInkScriptPathIntent empty{
            sizeof(InkpodInkScriptPathIntent), INKPOD_INKSCRIPT_RECORD_VERSION};
        std::vector<InkpodInkScriptPathIntent> intents(
            static_cast<std::size_t>(query.required_records), empty);
        std::vector<std::uint8_t> text(static_cast<std::size_t>(query.required_utf8_bytes));
        query.records = intents.empty() ? nullptr : intents.data();
        query.record_capacity = intents.size();
        query.record_stride_bytes = sizeof(InkpodInkScriptPathIntent);
        query.utf8 = text.empty() ? nullptr : text.data();
        query.utf8_capacity_bytes = text.size();
        status = inkpod_core_inkscript_program_path_intents_copy(core, program, &query);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        std::vector<std::string> paths(approved_paths.size());
        std::vector<InkpodInkScriptApprovedPath> records(approved_paths.size());
        for (std::size_t index = 0U; index < paths.size(); ++index) {
            status = PathUtf8(approved_paths[index], paths[index]);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            records[index].struct_size = sizeof(records[index]);
            records[index].version = INKPOD_INKSCRIPT_RECORD_VERSION;
            records[index].intent_id = intents[index].intent_id;
            records[index].path = {
                reinterpret_cast<const std::uint8_t*>(paths[index].data()), paths[index].size()};
        }
        InkpodInkScriptIoRequest request{};
        request.struct_size = sizeof(request);
        request.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        request.approved_paths = records.empty() ? nullptr : records.data();
        request.path_count = records.size();
        request.path_stride_bytes = records.empty() ? 0U : sizeof(InkpodInkScriptApprovedPath);
        request.new_tab_capacity = new_tab_capacity;
        status = inkpod_core_inkscript_io_create(core, manager, &request, &io_);
        if (status == INKPOD_STATUS_OK) {
            core_ = core;
        }
        return status;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

}  // namespace inkpod::app
