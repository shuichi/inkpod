#include "app/launch_options.h"

#include <string>

namespace {

using inkpod::app::LaunchMode;
using inkpod::app::LaunchOptions;
using inkpod::app::LaunchParseStatus;
using inkpod::app::ParseLaunchArguments;

LaunchParseStatus Parse(
    const wchar_t* const* arguments,
    int count,
    LaunchOptions& output) noexcept {
    return ParseLaunchArguments(count, arguments, output);
}

}  // namespace

int main() {
    LaunchOptions options{};
    const wchar_t* no_arguments[]{L"inkpod.exe"};
    if (Parse(no_arguments, 1, options) != LaunchParseStatus::Ok
        || options.mode != LaunchMode::Application
        || options.open_in_new_workspace
        || !options.document_paths.empty()) {
        return 1;
    }

    const wchar_t* document[]{
        L"inkpod.exe", L"C:\\制作資料\\空白 を含むセル.inkpod"};
    if (Parse(document, 2, options) != LaunchParseStatus::Ok
        || options.mode != LaunchMode::Application
        || options.document_paths.size() != 1U
        || options.document_paths[0] != document[1]) {
        return 2;
    }

    const wchar_t* smoke[]{L"inkpod.exe", L"--smoke-test"};
    if (Parse(smoke, 2, options) != LaunchParseStatus::Ok
        || options.mode != LaunchMode::ApplicationSmoke
        || !options.document_paths.empty()) {
        return 3;
    }

    const wchar_t* abi[]{L"inkpod.exe", L"--abi-smoke-test"};
    if (Parse(abi, 2, options) != LaunchParseStatus::Ok
        || options.mode != LaunchMode::AbiSmoke
        || !options.document_paths.empty()) {
        return 4;
    }

    const wchar_t* portable[]{L"inkpod.exe", L"--portable-smoke-test"};
    if (Parse(portable, 2, options) != LaunchParseStatus::Ok
        || options.mode != LaunchMode::PortableSmoke
        || !options.document_paths.empty()) {
        return 16;
    }

    const wchar_t* performance[]{
        L"inkpod.exe", L"--performance-smoke-test"};
    if (Parse(performance, 2, options) != LaunchParseStatus::Ok
        || options.mode != LaunchMode::PerformanceSmoke
        || !options.document_paths.empty()) {
        return 14;
    }

    const wchar_t* option_like_name[]{
        L"inkpod.exe", L"C:\\cells\\--smoke-test drawing.inkpod"};
    if (Parse(option_like_name, 2, options) != LaunchParseStatus::Ok
        || options.mode != LaunchMode::Application
        || options.document_paths.size() != 1U
        || options.document_paths[0] != option_like_name[1]) {
        return 5;
    }

    const wchar_t* escaped_option[]{
        L"inkpod.exe", L"--", L"--named-cell.inkpod"};
    if (Parse(escaped_option, 3, options) != LaunchParseStatus::Ok
        || options.document_paths.size() != 1U
        || options.document_paths[0] != escaped_option[2]) {
        return 6;
    }

    const wchar_t* conflicting[]{
        L"inkpod.exe", L"--smoke-test", L"cell.inkpod"};
    if (Parse(conflicting, 3, options) != LaunchParseStatus::InvalidArguments) {
        return 7;
    }

    const wchar_t* multiple_documents[]{
        L"inkpod.exe", L"first.inkpod", L"second.inkpod"};
    if (Parse(multiple_documents, 3, options) != LaunchParseStatus::Ok
        || options.document_paths.size() != 2U
        || options.document_paths[0] != multiple_documents[1]
        || options.document_paths[1] != multiple_documents[2]) {
        return 8;
    }

    const wchar_t* unknown_option[]{L"inkpod.exe", L"--unknown"};
    if (Parse(unknown_option, 2, options)
        != LaunchParseStatus::InvalidArguments) {
        return 9;
    }

    const wchar_t* duplicate_modes[]{
        L"inkpod.exe", L"--smoke-test", L"--abi-smoke-test"};
    if (Parse(duplicate_modes, 3, options)
        != LaunchParseStatus::InvalidArguments) {
        return 10;
    }

    const wchar_t* conflicting_performance[]{
        L"inkpod.exe", L"--performance-smoke-test", L"--smoke-test"};
    if (Parse(conflicting_performance, 3, options)
        != LaunchParseStatus::InvalidArguments) {
        return 15;
    }

    const wchar_t* conflicting_portable[]{
        L"inkpod.exe", L"--portable-smoke-test", L"--abi-smoke-test"};
    if (Parse(conflicting_portable, 3, options)
        != LaunchParseStatus::InvalidArguments) {
        return 17;
    }

    const wchar_t* new_window[]{
        L"inkpod.exe", L"--new-window", L"first.inkpod", L"second.inkpod"};
    if (Parse(new_window, 4, options) != LaunchParseStatus::Ok
        || !options.open_in_new_workspace
        || options.document_paths.size() != 2U) {
        return 11;
    }

    const wchar_t* duplicate_new_window[]{
        L"inkpod.exe", L"--new-window", L"--new-window"};
    if (Parse(duplicate_new_window, 3, options)
        != LaunchParseStatus::InvalidArguments) {
        return 12;
    }

    const wchar_t* smoke_new_window[]{
        L"inkpod.exe", L"--smoke-test", L"--new-window"};
    if (Parse(smoke_new_window, 3, options)
        != LaunchParseStatus::InvalidArguments) {
        return 13;
    }

    return 0;
}
