#pragma once

#include <windows.h>

#include <memory>
#include <string>
#include <vector>

namespace inkpod::app {

class ApplicationHost;

struct ApplicationLaunch {
    HINSTANCE instance{};
    int show_command{};
    bool smoke_test{};
    bool performance_smoke_test{};
    bool open_in_new_workspace{};
    std::vector<std::wstring> document_paths;
};

class Application final {
public:
    explicit Application(ApplicationLaunch launch) noexcept;
    ~Application();

    int Run();

private:
    ApplicationLaunch launch_;
    std::unique_ptr<ApplicationHost> host_;
};

}  // namespace inkpod::app
