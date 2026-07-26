#include "com_runtime.h"

#include <objbase.h>

namespace inkpod::app {

HRESULT ComApartment::Initialize() noexcept {
    const HRESULT result = CoInitializeEx(
        nullptr, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    initialized_ = SUCCEEDED(result);
    return result;
}

ComApartment::~ComApartment() {
    if (initialized_) {
        CoUninitialize();
    }
}

}  // namespace inkpod::app
