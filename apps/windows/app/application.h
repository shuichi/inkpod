#pragma once

#include <windows.h>

#include <memory>
#include <string>
#include <vector>

namespace inkpod::app {

class ApplicationHost;

// One message through the production dialog/shortcut/DispatchMessage order.
// Kept separate from GetMessage so native input regressions use the same path.
void DispatchApplicationMessage(ApplicationHost& state, MSG& message) noexcept;

struct ApplicationLaunch {
    HINSTANCE instance{};
    int show_command{};
    bool smoke_test{};
    bool performance_smoke_test{};
    bool open_in_new_workspace{};
    std::vector<std::wstring> document_paths;
    bool sequence_performance_smoke_test{};
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
