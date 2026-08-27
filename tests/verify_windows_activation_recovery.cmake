if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(APP_DIR "${INKPOD_SOURCE_DIR}/apps/windows/app")
set(UI_DIR "${INKPOD_SOURCE_DIR}/apps/windows/ui")
set(ACTIVATION "${APP_DIR}/activation.cpp")
set(RECOVERY "${APP_DIR}/session_recovery.cpp")
set(APPLICATION "${APP_DIR}/application.cpp")
set(PRESENTER "${UI_DIR}/main_window_document_presenter.cpp")

foreach(FILE IN ITEMS
        "${ACTIVATION}"
        "${RECOVERY}"
        "${APPLICATION}"
        "${PRESENTER}")
    if(NOT EXISTS "${FILE}")
        message(FATAL_ERROR "Missing G12 source: ${FILE}")
    endif()
endforeach()

file(READ "${ACTIVATION}" ACTIVATION_TEXT)
foreach(REQUIRED IN ITEMS
        "CreateMutexW"
        "CreateNamedPipeW"
        "PIPE_REJECT_REMOTE_CLIENTS"
        "ConvertStringSecurityDescriptorToSecurityDescriptorW"
        "kProtocolVersion"
        "kMaximumActivationMessageBytes"
        "PostThreadMessageW"
        "ActivationReplyStatus::Duplicate"
        "CancelIoEx")
    string(FIND "${ACTIVATION_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G12 activation boundary is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(FORBIDDEN IN ITEMS
        "reinterpret_cast<LPARAM>(&"
        "reinterpret_cast<WPARAM>(&"
        "InkpodCore"
        "CreateWindowExW")
    string(FIND "${ACTIVATION_TEXT}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR "Activation transport contains forbidden ownership: ${FORBIDDEN}")
    endif()
endforeach()

file(READ "${RECOVERY}" RECOVERY_TEXT)
foreach(REQUIRED IN ITEMS
        "inkpod_recovery_metadata_encode"
        "inkpod_recovery_metadata_decode"
        "RecoveryMetadataToAbi"
        "RecoveryMetadataFromAbi"
        "ResolveApplicationDataDirectory"
        "ResolveApplicationSessionPath"
        "kSessionPathsVersion"
        "WriteFileAtomic")
    string(FIND "${RECOVERY_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G12 recovery boundary is missing: ${REQUIRED}")
    endif()
endforeach()

foreach(FORBIDDEN IN ITEMS
        "kMetadataVersion"
        "bool WriteRecoveryMetadata("
        "bool ReadRecoveryMetadata("
        "bool EnumerateRecoveryCandidatesInDirectory("
        "bool DiscardRecoveryArtifact("
        "FindFirstFileW("
        "GetSystemTimeAsFileTime(")
    string(FIND "${RECOVERY_TEXT}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR "Recovery must not duplicate Rust file I/O or codec: ${FORBIDDEN}")
    endif()
endforeach()

file(READ "${INKPOD_SOURCE_DIR}/rust/inkpod-io/src/recovery.rs" RUST_RECOVERY_TEXT)
foreach(REQUIRED IN ITEMS
        "pub fn write_recovery("
        "pub fn list_recovery_candidates("
        "pub fn discard_recovery("
        "pub fn recovery_is_newer("
        "MAX_CANDIDATES"
        "MAX_RETAINED_METADATA"
        "metadata_error")
    string(FIND "${RUST_RECOVERY_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Rust recovery boundary is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${APPLICATION}" APPLICATION_TEXT)
string(FIND "${APPLICATION_TEXT}" "host_->activation->Start" ACTIVATION_OFFSET)
string(FIND "${APPLICATION_TEXT}" "InitCommonControlsEx" CONTROLS_OFFSET)
if(ACTIVATION_OFFSET LESS 0 OR CONTROLS_OFFSET LESS 0
    OR NOT ACTIVATION_OFFSET LESS CONTROLS_OFFSET)
    message(FATAL_ERROR
        "Secondary activation must exit before Common Controls/Core/renderer initialization")
endif()
foreach(REQUIRED IN ITEMS
        "ReviewRecoveryCandidates"
        "RestorePreviousDocuments"
        "SavePreviousDocuments"
        "kApplicationActivationMessage"
        "HandleApplicationActivation")
    string(FIND "${APPLICATION_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Application G12 lifecycle is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${PRESENTER}" PRESENTER_TEXT)
foreach(REQUIRED IN ITEMS
        "LastFocused"
        "ActivationTargetPreference::NewWorkspace"
        "CreateWorkspaceWindow"
        "OpenDocumentFromPath"
        "SetForegroundWindow")
    string(FIND "${PRESENTER_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Activation UI routing is missing: ${REQUIRED}")
    endif()
endforeach()

message(STATUS
    "Verified G12 current-user/versioned activation, value-only UI routing, "
    "all-candidate recovery, and opt-in session restoration boundaries")
