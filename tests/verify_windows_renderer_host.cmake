if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(RENDERER_DIR "${INKPOD_SOURCE_DIR}/apps/windows/renderer")
set(HOST_HEADER "${RENDERER_DIR}/renderer_host.h")
set(CANVAS_HEADER "${RENDERER_DIR}/canvas.h")
set(CANVAS_SOURCE "${RENDERER_DIR}/canvas.cpp")
set(APPLICATION_HEADER
    "${INKPOD_SOURCE_DIR}/apps/windows/app/application_host.h")
set(APPLICATION_SOURCE
    "${INKPOD_SOURCE_DIR}/apps/windows/app/application.cpp")
set(CORE_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/core_host.cpp")
set(TEST_SOURCE "${INKPOD_SOURCE_DIR}/tests/windows_renderer_host.cpp")
set(CMAKE_SOURCE "${INKPOD_SOURCE_DIR}/CMakeLists.txt")

foreach(FILE IN ITEMS
        "${HOST_HEADER}"
        "${CANVAS_HEADER}"
        "${CANVAS_SOURCE}"
        "${APPLICATION_HEADER}"
        "${APPLICATION_SOURCE}"
        "${CORE_SOURCE}"
        "${TEST_SOURCE}")
    if(NOT EXISTS "${FILE}")
        message(FATAL_ERROR "Missing G4 source: ${FILE}")
    endif()
endforeach()

file(READ "${HOST_HEADER}" HOST)
foreach(REQUIRED IN ITEMS
        "class RendererHost final"
        "struct SnapshotRoute"
        "DocumentSessionId document_session"
        "DocumentViewId document_view"
        "CanvasId canvas"
        "Generation document_generation"
        "Generation surface_generation"
        "struct SnapshotEnvelope"
        "document_revision"
        "view_revision"
        "RegisterSurface("
        "UnregisterSurface("
        "BindSurface("
        "UnbindSurface("
        "SurfaceAcceptsSnapshots("
        "bool Submit(SnapshotEnvelope envelope)"
        "DeviceGeneration()")
    string(FIND "${HOST}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "RendererHost contract is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${CANVAS_SOURCE}" CANVAS)
foreach(REQUIRED IN ITEMS
        "class SharedRendererDevice final"
        "class CanvasSurface final"
        "class RendererHostState final"
        "struct SurfaceRecord"
        "std::vector<SurfaceRecord> surfaces_"
        "std::deque<HostWork> work_"
        "work_.empty() && in_flight_work_ == 0U"
        "++in_flight_work_"
        "--in_flight_work_"
        "RecoverDevice()"
        "RecreateAfterSharedDeviceReset()"
        "ReleaseEnvelope("
        "route.surface_generation"
        "view.revision != envelope.document_revision"
        "transform.view_revision != envelope.view_revision"
        "TakeCanvasStrokeEvent("
        "TakeCanvasViewGesture("
        "kCanvasActivated"
        "static_cast<WPARAM>(host->Canvas().Value())"
        "static_cast<WPARAM>(token)"
        "static_cast<LPARAM>(surface_generation_.Value())"
        "const bool supersedes_surface"
        "kReservedHostControlWork"
        "published->visible = false"
        "WS_HSCROLL | WS_VSCROLL"
        "SIF_DISABLENOSCROLL"
        "SIF_TRACKPOS"
        "kCanvasScrollProjectionRetryTimer"
        "kMaximumScrollRefreshDeliveryAttempts"
        "scroll_projection_apply_active_"
        "scroll_projection_recovery_required_"
        "horizontal_interaction_shrink_pending_"
        "vertical_interaction_shrink_pending_"
        "WakeSupersedingScrollProjection(token)"
        "DeliverScrollProjectionViewportRefresh()"
        "GetWindowThreadProcessId(parent, nullptr) != GetCurrentThreadId()"
        "ScrollInfoMatches(restored_horizontal, previous_horizontal)"
        "RDW_INVALIDATE | RDW_FRAME | RDW_UPDATENOW")
    string(FIND "${CANVAS}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "RendererHost implementation is missing: ${REQUIRED}")
    endif()
endforeach()
if(CANVAS MATCHES "reinterpret_cast<LPARAM>\\(&event\\)"
        OR CANVAS MATCHES "reinterpret_cast<LPARAM>\\(&gesture\\)"
        OR CANVAS MATCHES "reinterpret_cast<LPARAM>\\(&preview\\)"
        OR CANVAS MATCHES "reinterpret_cast<LPARAM>\\(&bounds\\)")
    message(FATAL_ERROR "Canvas custom messages still expose C++ object pointers")
endif()
if(CANVAS MATCHES "class RenderThread" OR CANVAS MATCHES "RenderThread renderer_")
    message(FATAL_ERROR "Retired per-Canvas RenderThread still exists")
endif()
string(REGEX MATCHALL "D3D11CreateDevice\\(" D3D_CREATES "${CANVAS}")
list(LENGTH D3D_CREATES D3D_CREATE_COUNT)
if(NOT D3D_CREATE_COUNT EQUAL 3)
    message(FATAL_ERROR
        "Shared device fallback path changed unexpectedly: ${D3D_CREATE_COUNT}")
endif()
string(REGEX MATCHALL "D2D1CreateFactory\\(" D2D_CREATES "${CANVAS}")
list(LENGTH D2D_CREATES D2D_CREATE_COUNT)
if(NOT D2D_CREATE_COUNT EQUAL 1)
    message(FATAL_ERROR "D2D factory is not process-renderer shared")
endif()

file(READ "${APPLICATION_HEADER}" APPLICATION_HOST)
if(NOT APPLICATION_HOST MATCHES
        "std::unique_ptr<renderer::RendererHost> renderer")
    message(FATAL_ERROR "ApplicationHost does not own RendererHost")
endif()
file(READ "${APPLICATION_SOURCE}" APPLICATION)
foreach(REQUIRED IN ITEMS
        "StartRenderer(state)"
        "state.renderer->Start()"
        "state.renderer->Stop()"
        "StartCore(state)")
    string(FIND "${APPLICATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Renderer lifecycle integration is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${CORE_SOURCE}" CORE)
foreach(REQUIRED IN ITEMS
        "for (renderer::CanvasSnapshotSink* sink : snapshot_sinks)"
        "const renderer::SnapshotRoute route = sink->Route()"
        "sink->AcceptsSnapshots()"
        "view.frontend_view == route.document_view"
        "inkpod_core_build_snapshot_for_view"
        "renderer::SnapshotEnvelope envelope"
        "sink->Submit(envelope)")
    string(FIND "${CORE}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "CoreHost snapshot envelope integration is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${TEST_SOURCE}" TEST)
foreach(REQUIRED IN ITEMS
        "host.SurfaceCount() != 2U"
        "kCanvasGetRendererThreadId"
        "first_sink->Submit(first_envelope)"
        "second_sink->Submit(second_envelope)"
        "host.SimulateDeviceLoss"
        "host.Submit(stale)"
        "host.SetQueuePausedForSmokeTest(true)"
        "frames_before_queue_drain"
        "queued_render_work"
        "first_sink->Submit(queue_failure)"
        "VerifyCanvasScrollbarProjection("
        "VerifyDeferredScrollInteractionShrink("
        "horizontal_info.nPos != accepted_position"
        "UnbindCanvasSnapshotSink("
        "host.Submit(unbound_stale)"
        "first_canvas_window.Reset()"
        "host.Stop()")
    string(FIND "${TEST}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G4 native test evidence is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${CMAKE_SOURCE}" CMAKE_TEXT)
foreach(REQUIRED IN ITEMS
        "inkpod_windows_renderer_host_tests"
        "tests/windows_renderer_host.cpp"
        "verify_windows_renderer_host.cmake")
    string(FIND "${CMAKE_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G4 test registration is missing: ${REQUIRED}")
    endif()
endforeach()

message(STATUS
    "Verified process-owned RendererHost, shared device, multi-surface routing, "
    "snapshot ownership, device-loss recovery, and native G4 test boundaries")
