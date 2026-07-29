#pragma once

#include <windows.h>

#include <string>

namespace inkpod::app {

struct ApplicationLaunch {
    HINSTANCE instance{};
    int show_command{};
    bool smoke_test{};
    std::wstring document_path;
};

class Application final {
public:
    explicit Application(ApplicationLaunch launch) noexcept;

    int Run();

private:
    ApplicationLaunch launch_;
};

}  // namespace inkpod::app
