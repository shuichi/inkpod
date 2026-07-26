#pragma once

#include <windows.h>

namespace inkpod::app {

class ComApartment final {
public:
    ComApartment() = default;
    ~ComApartment();

    ComApartment(const ComApartment&) = delete;
    ComApartment& operator=(const ComApartment&) = delete;

    HRESULT Initialize() noexcept;

private:
    bool initialized_{};
};

}  // namespace inkpod::app
