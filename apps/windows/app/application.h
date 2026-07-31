#pragma once

#include <windows.h>

#include <memory>
#include <string>

namespace inkpod::app {

class ApplicationHost;

struct ApplicationLaunch {
    HINSTANCE instance{};
    int show_command{};
    bool smoke_test{};
    std::wstring document_path;
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
