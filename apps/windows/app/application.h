#pragma once

#include <windows.h>

namespace inkpod::app {

struct ApplicationLaunch {
    HINSTANCE instance{};
    int show_command{};
    bool smoke_test{};
};

class Application final {
public:
    explicit Application(ApplicationLaunch launch) noexcept;

    int Run();

private:
    ApplicationLaunch launch_;
};

}  // namespace inkpod::app
