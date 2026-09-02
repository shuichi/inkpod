#include "file_io_controller.h"

#include <algorithm>
#include <array>
#include <atomic>
#include <climits>
#include <cstring>
#include <mutex>
#include <new>
#include <utility>

#include "core_host.h"
#include "document_session.h"
#include "document_shell.h"

namespace inkpod::app {
namespace {

constexpr std::size_t kMaximumJobs = 128U;
constexpr std::uint64_t kMaximumResultItems = 10'000U;
constexpr std::uint64_t kMaximumPathBytes = 128U * 1024U;

bool FromUtf8(const std::vector<std::uint8_t>& input, std::wstring& output) {
    if (input.empty()) {
        output.clear();
        return true;
    }
    if (input.size() > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(input.data()), static_cast<int>(input.size()), nullptr, 0);
    if (count <= 0) {
        return false;
    }
    output.resize(static_cast<std::size_t>(count));
    return MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(input.data()), static_cast<int>(input.size()),
        output.data(), count) == count;
}

bool ReferenceKind(std::uint32_t kind) noexcept {
    return kind == INKPOD_IO_REFERENCE_FILES || kind == INKPOD_IO_REFERENCE_FOLDER;
}

bool RecoveryCatalogKind(std::uint32_t kind) noexcept {
    return kind == INKPOD_IO_RECOVERY_LIST || kind == INKPOD_IO_RECOVERY_PROBE;
}

bool DocumentlessKind(std::uint32_t kind) noexcept {
    return ReferenceKind(kind) || RecoveryCatalogKind(kind)
        || kind == INKPOD_IO_RECOVERY_DISCARD;
}

bool ReplacesDocument(std::uint32_t kind) noexcept {
    return kind == INKPOD_IO_OPEN_NATIVE || kind == INKPOD_IO_OPEN_RECOVERY
        || kind == INKPOD_IO_OPEN_RASTER || kind == INKPOD_IO_OPEN_RASTER_PAIR
        || kind == INKPOD_IO_SEQUENCE_SWITCH;
}

bool BatchKind(std::uint32_t kind) noexcept {
    return kind == INKPOD_IO_BATCH_PLAN || kind == INKPOD_IO_BATCH_RUN
        || kind == INKPOD_IO_BATCH_PREVIEW;
}

struct PendingFileIo final {
    FileIoRequest request;
    FileIoResult result;
    InkpodIoManager* manager{};
    InkpodIoJob* job{};
    std::atomic<bool> cancelled{};
    std::atomic<bool> finished{};
    bool preflight_required{};
    std::atomic<bool> preflight_requested{};
    std::atomic<bool> preflight_started{};
    std::atomic<bool> preflight_complete{};
    InkpodStatus preflight_status{INKPOD_STATUS_INVALID_STATE};
    InkpodStatus presentation_setup_status{INKPOD_STATUS_OK};
    FileIoResult preflight_result;
    mutable std::mutex progress_mutex;
    InkpodIoJobInfo progress{};
    bool items_loaded{};
    bool recovery_loaded{};
    bool recovery_artifact_proof_loaded{};
    bool recovery_metadata_loaded{};
    bool installing{};

    ~PendingFileIo() {
        (void)inkpod_batch_preview_release(&result.batch_preview);
        (void)inkpod_batch_report_release(&result.batch_report);
        if (job != nullptr) {
            (void)inkpod_io_job_release(&job);
        }
    }

    InkpodStatus Submit(InkpodCore* core) {
        if (BatchKind(request.kind)) {
            return inkpod_core_io_batch_submit(core, manager, request.batch_graph,
                request.kind, request.batch_scope, request.flags,
                request.new_tab_capacity, &job);
        }
        if (request.kind == INKPOD_IO_AUTOSAVE && request.recovery_metadata.has_value()) {
            return SubmitAutosave(core);
        }
        if (request.kind == INKPOD_IO_SEQUENCE_SWITCH) {
            return SubmitSequenceSwitch(core);
        }
        if (request.kind == INKPOD_IO_RECOVERY_DISCARD
            && request.discard_recovery_proof.has_value()) {
            if (request.paths.size() != 1U
                || !ValidRecoveryArtifactProof(
                    request.discard_recovery_proof.value())) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            std::vector<std::uint8_t> path;
            if (!WidePathToUtf8(request.paths[0], path)) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            return inkpod_core_io_recovery_discard_exact_submit(
                core, manager, path.data(), path.size(),
                &request.discard_recovery_proof.value(), &job);
        }
        if (request.kind == INKPOD_IO_COMPACTED_COPY) {
            if (request.paths.size() != 1U || !request.compaction_plan.has_value()) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            std::vector<std::uint8_t> path;
            if (!WidePathToUtf8(request.paths[0], path)) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            return inkpod_core_io_compacted_copy_submit(core, manager,
                path.data(), path.size(), &request.compaction_plan.value(), &job);
        }
        std::vector<std::vector<std::uint8_t>> paths;
        std::vector<InkpodIoPath> records;
        paths.resize(request.paths.size());
        records.reserve(paths.size());
        for (std::size_t index = 0U; index < paths.size(); ++index) {
            if (!WidePathToUtf8(request.paths[index], paths[index])) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            records.push_back(InkpodIoPath{
                sizeof(InkpodIoPath), 0U, paths[index].data(), paths[index].size()});
        }
        InkpodIoRequest input{};
        input.struct_size = sizeof(input);
        input.kind = request.kind;
        input.flags = request.flags;
        input.paths = records.empty() ? nullptr : records.data();
        input.path_count = records.size();
        input.path_stride_bytes = sizeof(InkpodIoPath);
        input.object_id = request.object_id;
        input.document_uuid_high = request.document_uuid_high;
        input.document_uuid_low = request.document_uuid_low;
        input.raster_format = request.raster_format;
        return inkpod_core_io_submit(core, manager, &input, &job);
    }

    InkpodStatus SubmitAutosave(InkpodCore* core) {
        if (request.paths.size() != 1U) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        std::vector<std::uint8_t> path;
        std::vector<std::uint8_t> metadata_text;
        InkpodIoRecoveryMetadata metadata{};
        if (!WidePathToUtf8(request.paths[0], path)
            || !RecoveryMetadataToAbi(request.recovery_metadata.value(), metadata,
                metadata_text)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        return inkpod_core_io_autosave_submit(core, manager, path.data(), path.size(),
            &metadata, &job);
    }

    InkpodStatus SubmitSequenceSwitch(InkpodCore* core) {
        if (request.paths.size() != 2U || !request.sequence_switch.has_value()) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        std::array<std::vector<std::uint8_t>, 2U> paths;
        std::array<InkpodIoPath, 2U> records{};
        for (std::size_t index = 0U; index < paths.size(); ++index) {
            if (!request.paths[index].empty()
                && !WidePathToUtf8(request.paths[index], paths[index])) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            records[index] = {sizeof(InkpodIoPath), 0U, paths[index].data(), paths[index].size()};
        }
        InkpodIoRecoveryMetadata metadata{};
        std::vector<std::uint8_t> metadata_text;
        if (request.recovery_metadata.has_value()
            && !RecoveryMetadataToAbi(request.recovery_metadata.value(), metadata, metadata_text)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        InkpodSequenceSwitchRequest sequence_request = request.sequence_switch.value();
        if (sequence_request.feature_flags != 0U) {
            return INKPOD_STATUS_UNSUPPORTED;
        }
        const bool target_recovery = !request.sequence_target_raster_pair
            && !request.paths[1].empty();
        if (target_recovery != request.target_recovery_proof.has_value()
            || (target_recovery
                && !ValidRecoveryArtifactProof(
                    request.target_recovery_proof.value()))) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        if (request.sequence_target_raster_pair) {
            sequence_request.feature_flags = INKPOD_SEQUENCE_SWITCH_TARGET_RASTER_PAIR;
        }
        return inkpod_core_io_sequence_switch_submit(core, manager,
            &sequence_request,
            paths[0].empty() ? nullptr : &records[0],
            paths[1].empty() ? nullptr : &records[1],
            request.target_recovery_proof.has_value()
                ? &request.target_recovery_proof.value() : nullptr,
            request.recovery_metadata.has_value() ? &metadata : nullptr, &job);
    }

    [[nodiscard]] bool RequiresRecoveryArtifactProof() const noexcept {
        return request.kind == INKPOD_IO_AUTOSAVE
            || (request.kind == INKPOD_IO_SEQUENCE_SWITCH
                && !request.paths.empty() && !request.paths[0].empty());
    }

    InkpodStatus ReadRecoveryArtifactProof() noexcept {
        if (recovery_artifact_proof_loaded) {
            return INKPOD_STATUS_OK;
        }
        InkpodIoRecoveryArtifactProof proof{};
        proof.struct_size = sizeof(proof);
        proof.native.struct_size = sizeof(proof.native);
        proof.native.identity.struct_size = sizeof(proof.native.identity);
        proof.metadata.struct_size = sizeof(proof.metadata);
        proof.metadata.identity.struct_size = sizeof(proof.metadata.identity);
        const InkpodStatus status =
            inkpod_io_job_get_recovery_artifact_proof(job, &proof);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        result.recovery_artifact_proof = proof;
        result.has_recovery_artifact_proof = true;
        recovery_artifact_proof_loaded = true;
        return INKPOD_STATUS_OK;
    }

    InkpodStatus ReadPublishedRecoveryMetadata() noexcept {
        try {
            if (recovery_metadata_loaded) {
                return INKPOD_STATUS_OK;
            }
            InkpodIoRecoveryMetadata metadata{};
            metadata.struct_size = sizeof(metadata);
            std::uint64_t required{};
            InkpodStatus status = inkpod_io_job_get_recovery_metadata(
                job, 0U, &metadata, nullptr, 0U, &required);
            if ((status != INKPOD_STATUS_OK
                    && status != INKPOD_STATUS_BUFFER_TOO_SMALL)
                || required > 3U * kMaximumPathBytes) {
                return status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_STATE : status;
            }
            std::vector<std::uint8_t> packed(static_cast<std::size_t>(required));
            status = inkpod_io_job_get_recovery_metadata(job, 0U, &metadata,
                packed.empty() ? nullptr : packed.data(), packed.size(), &required);
            RecoveryMetadata effective{};
            if (status != INKPOD_STATUS_OK
                || (metadata.flags & 1U) == 0U
                || !RecoveryMetadataFromAbi(metadata, effective)) {
                return status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_STATE : status;
            }
            result.recovery_metadata = std::move(effective);
            recovery_metadata_loaded = true;
            return INKPOD_STATUS_OK;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }

    InkpodStatus ReadItems(const InkpodIoJobInfo& info) {
        if (items_loaded) {
            return INKPOD_STATUS_OK;
        }
        if (info.result_count > kMaximumResultItems) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        std::vector<FileIoItem> items;
        items.reserve(static_cast<std::size_t>(info.result_count));
        for (std::uint64_t index = 0U; index < info.result_count; ++index) {
            FileIoItem item{};
            item.info.struct_size = sizeof(item.info);
            item.info.identity.struct_size = sizeof(item.info.identity);
            InkpodStatus status = inkpod_io_job_get_item(
                job, index, &item.info, nullptr, 0U, nullptr, 0U);
            if ((status != INKPOD_STATUS_OK && status != INKPOD_STATUS_BUFFER_TOO_SMALL)
                || item.info.path_bytes > kMaximumPathBytes
                || item.info.name_bytes > kMaximumPathBytes) {
                return status == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : status;
            }
            std::vector<std::uint8_t> path(static_cast<std::size_t>(item.info.path_bytes));
            std::vector<std::uint8_t> name(static_cast<std::size_t>(item.info.name_bytes));
            status = inkpod_io_job_get_item(job, index, &item.info,
                path.empty() ? nullptr : path.data(), path.size(),
                name.empty() ? nullptr : name.data(), name.size());
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            if (!FromUtf8(path, item.path) || !FromUtf8(name, item.name)
                || !NormalizeDocumentFilePath(item.path, item.normalized_path)) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            if (item.info.identity.kind == 1U) {
                item.identity.kind = DocumentIdentityKind::WindowsFile;
                item.identity.volume_serial = item.info.identity.volume;
                std::memcpy(item.identity.file_id.data(), &item.info.identity.object_low, 8U);
                std::memcpy(item.identity.file_id.data() + 8U, &item.info.identity.object_high, 8U);
            } else if (item.info.identity.kind == 2U) {
                item.identity.kind = DocumentIdentityKind::NormalizedPath;
                item.identity.normalized_path = item.normalized_path;
            } else if (item.info.identity.kind != 0U) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            if (request.kind == INKPOD_IO_SEQUENCE_AUTO
                || request.kind == INKPOD_IO_SEQUENCE_FILES) {
                InkpodIoSequenceResidentInfo resident{};
                resident.struct_size = sizeof(resident);
                resident.native_identity.struct_size =
                    sizeof(resident.native_identity);
                status = inkpod_io_job_get_sequence_resident(
                    job, index, &resident, nullptr, 0U);
                if (status != INKPOD_STATUS_OK
                    || resident.source_generation != item.info.source_generation
                    || resident.document_uuid_high
                        != item.info.document_uuid_high
                    || resident.document_uuid_low
                        != item.info.document_uuid_low
                    || (resident.flags
                        & ~INKPOD_IO_SEQUENCE_RESIDENT_AVAILABLE) != 0U
                    || resident.native_path_bytes > kMaximumPathBytes) {
                    return status == INKPOD_STATUS_OK
                        ? INKPOD_STATUS_INVALID_STATE : status;
                }
                if ((resident.flags
                        & INKPOD_IO_SEQUENCE_RESIDENT_AVAILABLE) != 0U) {
                    std::vector<std::uint8_t> native_path(
                        static_cast<std::size_t>(resident.native_path_bytes));
                    status = inkpod_io_job_get_sequence_resident(
                        job, index, &resident,
                        native_path.empty() ? nullptr : native_path.data(),
                        native_path.size());
                    FileIoItem::SequenceResidentNative candidate{};
                    std::wstring normalized;
                    if (status != INKPOD_STATUS_OK
                        || native_path.empty()
                        || !FromUtf8(native_path, candidate.path)
                        || !NormalizeDocumentFilePath(
                            candidate.path, normalized)) {
                        return status == INKPOD_STATUS_OK
                            ? INKPOD_STATUS_INVALID_STATE : status;
                    }
                    if (resident.native_identity.kind == 1U) {
                        candidate.identity.kind =
                            DocumentIdentityKind::WindowsFile;
                        candidate.identity.volume_serial =
                            resident.native_identity.volume;
                        std::memcpy(candidate.identity.file_id.data(),
                            &resident.native_identity.object_low, 8U);
                        std::memcpy(candidate.identity.file_id.data() + 8U,
                            &resident.native_identity.object_high, 8U);
                    } else if (resident.native_identity.kind == 2U) {
                        candidate.identity.kind =
                            DocumentIdentityKind::NormalizedPath;
                        candidate.identity.normalized_path =
                            std::move(normalized);
                    } else {
                        return INKPOD_STATUS_INVALID_STATE;
                    }
                    item.sequence_resident_native = std::move(candidate);
                } else if (resident.native_path_bytes != 0U
                    || resident.native_identity.kind != 0U) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
            }
            items.push_back(std::move(item));
        }
        result.items = std::move(items);
        items_loaded = true;
        return (RecoveryCatalogKind(request.kind) || request.kind == INKPOD_IO_OPEN_RECOVERY)
            ? ReadRecoveryItems() : INKPOD_STATUS_OK;
    }

    InkpodStatus RefreshRepairedPairItems(const InkpodIoJobInfo& info) noexcept {
        if (request.kind != INKPOD_IO_SAVE_PAIR || !installing
            || (info.flags & INKPOD_IO_RESULT_AUTHORITY_REPAIRED) == 0U
            || info.result_count != 2U || result.items.size() != 2U) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        for (std::uint64_t index = 0U; index < 2U; ++index) {
            InkpodIoItemInfo refreshed{};
            refreshed.struct_size = sizeof(refreshed);
            refreshed.identity.struct_size = sizeof(refreshed.identity);
            const InkpodStatus queried = inkpod_io_job_get_item(
                job, index, &refreshed, nullptr, 0U, nullptr, 0U);
            auto& item = result.items[static_cast<std::size_t>(index)];
            if ((queried != INKPOD_STATUS_OK
                    && queried != INKPOD_STATUS_BUFFER_TOO_SMALL)
                || refreshed.path_bytes != item.info.path_bytes
                || refreshed.name_bytes != item.info.name_bytes
                || refreshed.raster_format != item.info.raster_format
                || refreshed.source_generation != item.info.source_generation
                || refreshed.document_uuid_high != item.info.document_uuid_high
                || refreshed.document_uuid_low != item.info.document_uuid_low
                || (refreshed.identity.kind != 1U
                    && refreshed.identity.kind != 2U)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            item.info = refreshed;
            if (refreshed.identity.kind == 1U) {
                item.identity.kind = DocumentIdentityKind::WindowsFile;
                item.identity.volume_serial = refreshed.identity.volume;
                std::memcpy(item.identity.file_id.data(),
                    &refreshed.identity.object_low, 8U);
                std::memcpy(item.identity.file_id.data() + 8U,
                    &refreshed.identity.object_high, 8U);
                item.identity.normalized_path.clear();
            } else {
                item.identity.kind = DocumentIdentityKind::NormalizedPath;
                item.identity.volume_serial = 0U;
                item.identity.file_id.fill(0U);
                item.identity.normalized_path = std::move(item.normalized_path);
            }
        }
        result.authority_repaired = true;
        return INKPOD_STATUS_OK;
    }

    InkpodStatus ReadRecoveryItems() {
        if (recovery_loaded) {
            return INKPOD_STATUS_OK;
        }
        std::vector<RecoveryCandidate> candidates;
        candidates.reserve(result.items.size());
        for (std::size_t index = 0U; index < result.items.size(); ++index) {
            InkpodIoRecoveryMetadata metadata{};
            metadata.struct_size = sizeof(metadata);
            std::uint64_t required{};
            InkpodStatus status = inkpod_io_job_get_recovery_metadata(
                job, index, &metadata, nullptr, 0U, &required);
            if ((status != INKPOD_STATUS_OK && status != INKPOD_STATUS_BUFFER_TOO_SMALL)
                || required > 3U * kMaximumPathBytes) {
                return status == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : status;
            }
            std::vector<std::uint8_t> packed(static_cast<std::size_t>(required));
            status = inkpod_io_job_get_recovery_metadata(job, index, &metadata,
                packed.empty() ? nullptr : packed.data(), packed.size(), &required);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            RecoveryCandidate candidate{};
            candidate.recovery_path = result.items[index].path;
            candidate.metadata_path = candidate.recovery_path + L".metadata";
            candidate.modified.dwLowDateTime = static_cast<DWORD>(metadata.modified_time_100ns);
            candidate.modified.dwHighDateTime = static_cast<DWORD>(metadata.modified_time_100ns >> 32U);
            candidate.has_metadata = (metadata.flags & 1U) != 0U;
            if (candidate.has_metadata && !RecoveryMetadataFromAbi(metadata, candidate.metadata)) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            candidates.push_back(std::move(candidate));
        }
        result.recovery_candidates = std::move(candidates);
        recovery_loaded = true;
        return INKPOD_STATUS_OK;
    }

    void ReadError() {
        std::uint64_t required{};
        const InkpodStatus status = inkpod_io_job_copy_error(job, nullptr, 0U, &required);
        if ((status != INKPOD_STATUS_OK && status != INKPOD_STATUS_BUFFER_TOO_SMALL)
            || required == 0U || required > kMaximumPathBytes) {
            return;
        }
        std::vector<std::uint8_t> error(static_cast<std::size_t>(required));
        if (inkpod_io_job_copy_error(job, error.data(), error.size(), &required)
                == INKPOD_STATUS_OK) {
            if (!error.empty() && error.back() == 0U) {
                error.pop_back();
            }
            (void)FromUtf8(error, result.error);
        }
    }

    InkpodStatus Step(InkpodCore* core, bool host_cancelled, bool& fence) {
        if (host_cancelled && request.kind != INKPOD_IO_RECOVERY_DISCARD) {
            cancelled.store(true, std::memory_order_release);
        }
        if (job == nullptr) {
            if (cancelled.load(std::memory_order_acquire)) {
                return INKPOD_STATUS_CANCELLED;
            }
            const InkpodStatus submitted = Submit(core);
            if (submitted != INKPOD_STATUS_OK) {
                return submitted;
            }
        }
        if (cancelled.load(std::memory_order_acquire)) {
            (void)inkpod_io_job_cancel(job);
        }
        InkpodIoJobInfo info{};
        info.struct_size = sizeof(info);
        InkpodStatus status = inkpod_io_job_poll(job, &info);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        {
            std::lock_guard lock(progress_mutex);
            progress = info;
        }
        result.progress = info;
        if (cancelled.load(std::memory_order_acquire) && !installing
            && info.state != INKPOD_IO_COMPLETE) {
            return INKPOD_STATUS_CANCELLED;
        }
        if (info.state == INKPOD_IO_QUEUED || info.state == INKPOD_IO_RUNNING) {
            fence = installing || (info.flags & INKPOD_IO_RESULT_INSTALLING) != 0U;
            return INKPOD_STATUS_PENDING;
        }
        if (info.state == INKPOD_IO_READY) {
            InkpodStatus repaired_refresh_status = INKPOD_STATUS_OK;
            status = ReadItems(info);
            if (status != INKPOD_STATUS_OK && !installing) {
                (void)inkpod_io_job_cancel(job);
                return status;
            }
            if (preflight_required && !installing) {
                if (!preflight_requested.load(std::memory_order_relaxed)) {
                    preflight_result = result;
                    preflight_requested.store(true, std::memory_order_release);
                }
                if (!preflight_complete.load(std::memory_order_acquire)) {
                    return INKPOD_STATUS_PENDING;
                }
                if (preflight_status != INKPOD_STATUS_OK) {
                    (void)inkpod_io_job_cancel(job);
                    return preflight_status;
                }
            }
            if (installing
                && (info.flags & INKPOD_IO_RESULT_AUTHORITY_REPAIRED) != 0U) {
                // The worker has already rolled back durable bytes. Refresh
                // only fixed item authority. If that fixed-width query cannot
                // be trusted, cancel asks Core finalization to revoke the pair;
                // owner final apply remains mandatory to release its fence.
                repaired_refresh_status =
                    request.smoke_fail_repaired_item_refresh
                    ? INKPOD_STATUS_INVALID_STATE
                    : RefreshRepairedPairItems(info);
                if (repaired_refresh_status != INKPOD_STATUS_OK) {
                    (void)inkpod_io_job_cancel(job);
                }
            }
            const bool proof_ready = request.kind == INKPOD_IO_AUTOSAVE
                || (request.kind == INKPOD_IO_SEQUENCE_SWITCH && installing);
            InkpodStatus recovery_publication_status = INKPOD_STATUS_OK;
            if (proof_ready && info.status == INKPOD_STATUS_OK
                && RequiresRecoveryArtifactProof()) {
                recovery_publication_status = ReadRecoveryArtifactProof();
                if (recovery_publication_status == INKPOD_STATUS_OK) {
                    recovery_publication_status =
                        ReadPublishedRecoveryMetadata();
                }
                if (recovery_publication_status != INKPOD_STATUS_OK
                    && installing) {
                    // Installation already ran. Cancellation asks Core final
                    // apply to retain the source rather than commit a switch
                    // whose recovery association cannot be published. In all
                    // cases final apply below is mandatory to release its fence.
                    (void)inkpod_io_job_cancel(job);
                } else if (recovery_publication_status != INKPOD_STATUS_OK) {
                    return recovery_publication_status;
                }
            }
            result.document.struct_size = sizeof(result.document);
            result.subpalette.struct_size = sizeof(result.subpalette);
            status = ReferenceKind(request.kind)
                ? inkpod_subpalette_io_job_apply(request.subpalette, job, &result.subpalette)
                : inkpod_core_io_job_apply(core, job, &result.document, &result.object_id);
            if (status == INKPOD_STATUS_PENDING) {
                installing = request.kind == INKPOD_IO_SAVE_PAIR
                    || request.kind == INKPOD_IO_SEQUENCE_SWITCH
                    || request.kind == INKPOD_IO_COMPACTED_COPY;
                fence = installing;
                return status;
            }
            result.document_applied = !ReferenceKind(request.kind)
                && status == INKPOD_STATUS_OK;
            installing = false;
            fence = false;
            InkpodIoJobInfo finalized{};
            finalized.struct_size = sizeof(finalized);
            if (inkpod_io_job_poll(job, &finalized) == INKPOD_STATUS_OK) {
                result.progress = finalized;
                result.authority_revoked =
                    (finalized.flags
                        & INKPOD_IO_RESULT_AUTHORITY_REVOKED) != 0U;
            }
            if (repaired_refresh_status != INKPOD_STATUS_OK
                && !result.authority_revoked) {
                status = repaired_refresh_status;
            } else if (recovery_publication_status != INKPOD_STATUS_OK) {
                status = recovery_publication_status;
            }
        } else if (installing) {
            InkpodStatus recovery_publication_status = INKPOD_STATUS_OK;
            if (info.status == INKPOD_STATUS_OK
                && RequiresRecoveryArtifactProof()) {
                recovery_publication_status = ReadRecoveryArtifactProof();
                if (recovery_publication_status == INKPOD_STATUS_OK) {
                    recovery_publication_status =
                        ReadPublishedRecoveryMetadata();
                }
                if (recovery_publication_status != INKPOD_STATUS_OK) {
                    (void)inkpod_io_job_cancel(job);
                }
            }
            result.document.struct_size = sizeof(result.document);
            status = inkpod_core_io_job_apply(core, job, &result.document, &result.object_id);
            if (status == INKPOD_STATUS_PENDING) {
                fence = true;
                return status;
            }
            result.document_applied = status == INKPOD_STATUS_OK;
            installing = false;
            fence = false;
            InkpodIoJobInfo finalized{};
            finalized.struct_size = sizeof(finalized);
            if (inkpod_io_job_poll(job, &finalized) == INKPOD_STATUS_OK) {
                result.progress = finalized;
                result.authority_revoked =
                    (finalized.flags
                        & INKPOD_IO_RESULT_AUTHORITY_REVOKED) != 0U;
            }
            if (recovery_publication_status != INKPOD_STATUS_OK) {
                status = recovery_publication_status;
            }
        } else if (info.state == INKPOD_IO_COMPLETE) {
            status = ReadItems(info);
        } else {
            status = info.status == INKPOD_STATUS_OK
                ? (info.state == INKPOD_IO_CANCELLED ? INKPOD_STATUS_CANCELLED : INKPOD_STATUS_IO_ERROR)
                : info.status;
        }
        if (status != INKPOD_STATUS_OK) {
            ReadError();
        } else if (request.kind == INKPOD_IO_BATCH_PLAN) {
            status = inkpod_io_job_take_batch_preview(job, &result.batch_preview);
        } else if (BatchKind(request.kind)) {
            status = inkpod_io_job_take_batch_report(job, &result.batch_report);
        }
        return status;
    }
};

}  // namespace

struct FileIoController::Impl final {
    struct IdleEntry final {
        CommandContext context;
        std::function<void()> completion;
    };
    struct Entry final {
        std::shared_ptr<PendingFileIo> pending;
        Completion completion;
        Preflight preflight;
    };
    InkpodIoManager* manager{};
    std::vector<Entry> entries;
    std::vector<IdleEntry> idle;
    std::uint64_t next_request_id{1U};

    ~Impl() {
        entries.clear();
        (void)inkpod_io_manager_release(&manager);
    }
};

FileIoController::FileIoController() = default;
FileIoController::~FileIoController() = default;

InkpodStatus FileIoController::Initialize(
    std::uint32_t validated_sidecar_cache_mib) noexcept {
    if (impl_ != nullptr) {
        return INKPOD_STATUS_OK;
    }
    try {
        auto next = std::make_unique<Impl>();
        next->entries.reserve(kMaximumJobs);
        next->idle.reserve(kMaximumJobs);
        const InkpodStatus status = inkpod_io_manager_create(nullptr, &next->manager);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        const InkpodStatus cache_status =
            inkpod_io_manager_set_validated_target_cache_bytes(
                next->manager,
                static_cast<std::uint64_t>(validated_sidecar_cache_mib)
                    * UINT64_C(1024) * UINT64_C(1024));
        if (cache_status != INKPOD_STATUS_OK) {
            return cache_status;
        }
        impl_ = std::move(next);
        return INKPOD_STATUS_OK;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

InkpodIoManager* FileIoController::Manager() const noexcept {
    return impl_ == nullptr ? nullptr : impl_->manager;
}

bool FileIoController::Queue(CoreHost& engine, FileIoRequest request,
    Completion completion, std::uint64_t* out_request_id, Preflight preflight) noexcept {
    if (impl_ == nullptr || impl_->entries.size() >= kMaximumJobs
        || impl_->next_request_id == 0U || request.paths.size() > kMaximumResultItems
        || (ReferenceKind(request.kind) && request.subpalette == nullptr)) {
        return false;
    }
    try {
        auto pending = std::make_shared<PendingFileIo>();
        pending->request = std::move(request);
        pending->preflight_required = static_cast<bool>(preflight);
        pending->manager = impl_->manager;
        pending->result.request_id = impl_->next_request_id;
        pending->result.context = pending->request.context;
        pending->result.kind = pending->request.kind;
        pending->progress.struct_size = sizeof(pending->progress);
        pending->progress.kind = pending->request.kind;
        const std::uint64_t id = impl_->next_request_id;
        const bool modifies = pending->request.publish_snapshot
            || pending->request.kind == INKPOD_IO_LIGHT_TABLE_ADD
            || pending->request.kind == INKPOD_IO_LIGHT_TABLE_RELOAD
            || pending->request.kind == INKPOD_IO_BATCH_RUN;
        const bool requires_document =
            !DocumentlessKind(pending->request.kind);
        const bool refresh = !DocumentlessKind(pending->request.kind)
            && pending->request.kind != INKPOD_IO_EXPORT_RASTER
            && pending->request.kind != INKPOD_IO_AUTOSAVE;
        CoreHost::FileIoOperation operation =
            [pending, &engine](
                InkpodCore* core,
                bool cancelled,
                bool& installing,
                bool& document_replaced) {
                const InkpodStatus status = pending->Step(core, cancelled, installing);
                document_replaced = pending->result.document_applied
                    && ReplacesDocument(pending->request.kind);
                if (pending->result.document_applied
                    && (pending->request.presentation_epoch != 0U
                        || ReplacesDocument(pending->request.kind))
                    && (!pending->request.context.document_session.has_value()
                        || !pending->request.context.generation.has_value()
                        || !engine.SetPresentationEpoch(
                            pending->request.context.document_session.value(),
                            pending->request.context.generation.value(),
                            pending->request.presentation_epoch))) {
                    // The Core apply has already committed. Preserve its
                    // durable status and report epoch/snapshot setup only on
                    // the presentation channel so the UI can retry without
                    // applying the file operation a second time.
                    pending->presentation_setup_status =
                        INKPOD_STATUS_INVALID_STATE;
                }
                return status;
            };
        CoreHost::FileIoCompletion completed = [pending](
            InkpodStatus status, InkpodStatus presentation_status) {
            // Drop staged Core/image leases on the engine thread before the
            // UI sees completion. The immutable copied result remains valid.
            const InkpodStatus released = inkpod_io_job_release(&pending->job);
            if (status == INKPOD_STATUS_OK && released != INKPOD_STATUS_OK) {
                status = released;
            }
            if (pending->presentation_setup_status != INKPOD_STATUS_OK) {
                presentation_status = pending->presentation_setup_status;
            }
            pending->result.status = status;
            pending->result.presentation_status = presentation_status;
            pending->finished.store(true, std::memory_order_release);
        };
        impl_->entries.push_back(Impl::Entry{pending, std::move(completion), std::move(preflight)});
        if (!engine.EnqueueFileIo(pending->request.context,
                requires_document, std::move(operation),
                modifies, refresh, std::move(completed))) {
            impl_->entries.pop_back();
            return false;
        }
        ++impl_->next_request_id;
        if (out_request_id != nullptr) {
            *out_request_id = id;
        }
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

void FileIoController::Poll() noexcept {
    if (impl_ == nullptr) {
        return;
    }
    // Remove before invoking the continuation; it may enqueue another request.
    for (std::size_t index = 0U; index < impl_->entries.size();) {
        auto pending = impl_->entries[index].pending;
        if (!pending->finished.load(std::memory_order_acquire)
            && !pending->cancelled.load(std::memory_order_acquire)
            && pending->preflight_requested.load(std::memory_order_acquire)
            && !pending->preflight_started.exchange(true, std::memory_order_acq_rel)) {
            auto preflight = std::move(impl_->entries[index].preflight);
            InkpodStatus status = INKPOD_STATUS_INVALID_STATE;
            try {
                status = preflight(pending->preflight_result);
            } catch (...) {
            }
            pending->preflight_status = status;
            pending->preflight_complete.store(true, std::memory_order_release);
            // A dialog may have dispatched a nested timer and changed entries.
            index = 0U;
            continue;
        }
        if (!impl_->entries[index].pending->finished.load(std::memory_order_acquire)) {
            ++index;
            continue;
        }
        auto entry = std::move(impl_->entries[index]);
        impl_->entries.erase(impl_->entries.begin() + static_cast<std::ptrdiff_t>(index));
        if (entry.completion) {
            try {
                entry.completion(std::move(entry.pending->result));
            } catch (...) {
            }
        }
    }
    for (std::size_t index = 0U; index < impl_->idle.size();) {
        const auto& context = impl_->idle[index].context;
        const bool pending = context.document_session.has_value() && context.generation.has_value()
            ? HasPending(context.document_session.value(), context.generation.value())
            : context.workspace.has_value() ? HasPending(context.workspace.value()) : HasPending();
        if (pending) {
            ++index;
            continue;
        }
        auto completion = std::move(impl_->idle[index].completion);
        impl_->idle.erase(impl_->idle.begin() + static_cast<std::ptrdiff_t>(index));
        try {
            completion();
        } catch (...) {
        }
        index = 0U;
    }
}

void FileIoController::Cancel(std::uint64_t request_id) noexcept {
    if (impl_ != nullptr) {
        for (const auto& entry : impl_->entries) {
            if (entry.pending->result.request_id == request_id) {
                if (entry.pending->request.kind != INKPOD_IO_RECOVERY_DISCARD) {
                    entry.pending->cancelled.store(true, std::memory_order_release);
                }
            }
        }
    }
}

void FileIoController::CancelSession(DocumentSessionId session, Generation generation) noexcept {
    if (impl_ != nullptr) {
        for (const auto& entry : impl_->entries) {
            if (entry.pending->request.context.document_session == session
                && entry.pending->request.context.generation == generation) {
                if (entry.pending->request.kind != INKPOD_IO_RECOVERY_DISCARD) {
                    entry.pending->cancelled.store(true, std::memory_order_release);
                }
            }
        }
    }
}

void FileIoController::CancelWorkspace(WorkspaceWindowId workspace) noexcept {
    if (impl_ != nullptr) {
        for (const auto& entry : impl_->entries) {
            if (entry.pending->request.context.workspace == workspace) {
                if (entry.pending->request.kind != INKPOD_IO_RECOVERY_DISCARD) {
                    entry.pending->cancelled.store(true, std::memory_order_release);
                }
            }
        }
    }
}

void FileIoController::CancelAll() noexcept {
    if (impl_ != nullptr) {
        for (const auto& entry : impl_->entries) {
            if (entry.pending->request.kind != INKPOD_IO_RECOVERY_DISCARD) {
                entry.pending->cancelled.store(true, std::memory_order_release);
            }
        }
    }
}

void FileIoController::ClearCompleted() noexcept {
    if (impl_ != nullptr) {
        impl_->idle.clear();
        std::erase_if(impl_->entries, [](const auto& entry) {
            return entry.pending->finished.load(std::memory_order_acquire);
        });
    }
}

bool FileIoController::WhenIdle(CommandContext context, std::function<void()> completion) noexcept {
    if (impl_ == nullptr || !completion || impl_->idle.size() >= kMaximumJobs) {
        return false;
    }
    try {
        impl_->idle.push_back(Impl::IdleEntry{std::move(context), std::move(completion)});
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool FileIoController::HasPending() const noexcept {
    return impl_ != nullptr && !impl_->entries.empty();
}

bool FileIoController::HasPending(WorkspaceWindowId workspace) const noexcept {
    return impl_ != nullptr && std::any_of(impl_->entries.cbegin(), impl_->entries.cend(),
        [workspace](const auto& entry) { return entry.pending->request.context.workspace == workspace; });
}

bool FileIoController::HasPending(DocumentSessionId session, Generation generation) const noexcept {
    return impl_ != nullptr && std::any_of(impl_->entries.cbegin(), impl_->entries.cend(),
        [session, generation](const auto& entry) {
            return entry.pending->request.context.document_session == session
                && entry.pending->request.context.generation == generation;
        });
}

bool FileIoController::ConflictsWithPendingWrite(
    const FileIoItem& item, std::uint64_t except_request_id) const noexcept {
    if (impl_ == nullptr) {
        return false;
    }
    for (const auto& entry : impl_->entries) {
        const auto& pending = *entry.pending;
        if (pending.result.request_id == except_request_id
            || (pending.request.kind != INKPOD_IO_SAVE_PAIR
                && pending.request.kind != INKPOD_IO_COMPACTED_COPY)
            || !pending.preflight_complete.load(std::memory_order_acquire)
            || pending.preflight_status != INKPOD_STATUS_OK) {
            continue;
        }
        for (const auto& destination : pending.preflight_result.items) {
            if ((item.identity && destination.identity && item.identity == destination.identity)
                || (!item.normalized_path.empty()
                    && item.normalized_path == destination.normalized_path)) {
                return true;
            }
        }
    }
    return false;
}

InkpodStatus FileIoController::SetValidatedSidecarCacheMiB(
    std::uint32_t maximum_mib) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return inkpod_io_manager_set_validated_target_cache_bytes(
        impl_->manager,
        static_cast<std::uint64_t>(maximum_mib)
            * UINT64_C(1024) * UINT64_C(1024));
}

bool FileIoController::HasPendingKind(
    DocumentSessionId session,
    Generation generation,
    std::uint32_t kind) const noexcept {
    return impl_ != nullptr && std::any_of(
        impl_->entries.cbegin(), impl_->entries.cend(),
        [session, generation, kind](const auto& entry) {
            return entry.pending->request.context.document_session == session
                && entry.pending->request.context.generation == generation
                && entry.pending->request.kind == kind;
        });
}

bool FileIoController::HasPendingExceptKind(
    DocumentSessionId session,
    Generation generation,
    std::uint32_t allowed_kind) const noexcept {
    return impl_ != nullptr && std::any_of(
        impl_->entries.cbegin(), impl_->entries.cend(),
        [session, generation, allowed_kind](const auto& entry) {
            return entry.pending->request.context.document_session == session
                && entry.pending->request.context.generation == generation
                && entry.pending->request.kind != allowed_kind;
        });
}

bool FileIoController::ConflictsWithPendingAuthority(
    const FileIoItem& item, std::uint64_t except_request_id) const noexcept {
    if (impl_ == nullptr) {
        return false;
    }
    for (const auto& entry : impl_->entries) {
        const auto& pending = *entry.pending;
        const bool owns_file_authority = pending.request.kind == INKPOD_IO_SAVE_PAIR
            || pending.request.kind == INKPOD_IO_COMPACTED_COPY
            || pending.request.kind == INKPOD_IO_OPEN_NATIVE
            || pending.request.kind == INKPOD_IO_OPEN_RASTER_PAIR;
        if (pending.result.request_id == except_request_id || !owns_file_authority
            || !pending.preflight_complete.load(std::memory_order_acquire)
            || pending.preflight_status != INKPOD_STATUS_OK) {
            continue;
        }
        for (const auto& reserved : pending.preflight_result.items) {
            if ((item.identity && reserved.identity && item.identity == reserved.identity)
                || (!item.normalized_path.empty()
                    && item.normalized_path == reserved.normalized_path)) {
                return true;
            }
        }
    }
    return false;
}

bool FileIoController::Progress(std::uint64_t request_id, InkpodIoJobInfo& output) const noexcept {
    if (impl_ != nullptr) {
        for (const auto& entry : impl_->entries) {
            if (entry.pending->result.request_id == request_id) {
                std::lock_guard lock(entry.pending->progress_mutex);
                output = entry.pending->progress;
                return true;
            }
        }
    }
    return false;
}

bool FileIoController::Progress(WorkspaceWindowId workspace, InkpodIoJobInfo& output) const noexcept {
    if (impl_ != nullptr) {
        for (const auto& entry : impl_->entries) {
            if (entry.pending->request.context.workspace == workspace) {
                std::lock_guard lock(entry.pending->progress_mutex);
                output = entry.pending->progress;
                return true;
            }
        }
    }
    return false;
}

std::size_t FileIoController::CopyProgress(
    WorkspaceWindowId workspace, std::span<FileIoProgressEntry> output) const noexcept {
    if (impl_ == nullptr || !workspace || output.empty()) {
        return 0U;
    }
    std::size_t copied{};
    for (const auto& entry : impl_->entries) {
        if (copied == output.size()) {
            break;
        }
        const auto& pending = *entry.pending;
        if (pending.request.context.workspace != workspace) {
            continue;
        }
        auto& value = output[copied];
        value.request_id = pending.result.request_id;
        value.context = pending.request.context;
        {
            std::lock_guard lock(pending.progress_mutex);
            value.progress = pending.progress;
        }
        value.cancelling = pending.cancelled.load(std::memory_order_acquire);
        ++copied;
    }
    return copied;
}

}  // namespace inkpod::app
