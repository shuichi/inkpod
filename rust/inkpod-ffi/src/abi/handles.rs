use super::*;

pub struct InkpodCore {
    pub(crate) owner_thread: ThreadId,
    pub(crate) core: Core,
    pub(crate) objects: crate::v3::ObjectRegistry,
}

pub struct InkpodSnapshot {
    pub(crate) snapshot: RenderSnapshot,
    pub(crate) tiles: Box<[InkpodSnapshotTile]>,
    pub(crate) guides: Box<[InkpodSnapshotGuide]>,
    pub(crate) render_passes: Box<[InkpodSnapshotRenderPass]>,
    pub(crate) shooting_frames: Box<[InkpodShootingFrameInfo]>,
}

pub struct InkpodSubpalette {
    pub(crate) owner_thread: ThreadId,
    pub(crate) catalog: SubpaletteCatalog,
}

pub struct InkpodCut {
    pub(crate) owner_thread: ThreadId,
    pub(crate) cut: CutCore,
}

pub struct InkpodCellCreationPlan {
    pub(crate) plan: CellCreationPlan,
    pub(crate) sizing_mode: u32,
}

pub struct InkpodClipboard {
    pub(crate) payload: ClipboardPayload,
}

pub struct InkpodByteBuffer {
    pub(crate) bytes: Box<[u8]>,
}

pub struct InkpodHistoryVisualization {
    pub(crate) rows: Box<[HistoryVisualizationRow]>,
}

pub struct InkpodHistoryVisualizationBuilder {
    pub(crate) owner_thread: ThreadId,
    pub(crate) task_address: usize,
    pub(crate) builder: Option<CoreHistoryVisualizationBuilder>,
}

pub(crate) struct EncodedSequenceFile {
    pub(crate) name: Box<[u8]>,
    pub(crate) bytes: Box<[u8]>,
}

pub struct InkpodEncodedSequence {
    pub(crate) files: Vec<EncodedSequenceFile>,
}

pub struct InkpodTask {
    pub(crate) state: AtomicU32,
    pub(crate) cancelled: AtomicBool,
    pub(crate) completed_work: AtomicU64,
    pub(crate) total_work: AtomicU64,
}

impl InkpodTask {
    pub(crate) fn new() -> Self {
        Self {
            state: AtomicU32::new(INKPOD_TASK_READY),
            cancelled: AtomicBool::new(false),
            completed_work: AtomicU64::new(0),
            total_work: AtomicU64::new(0),
        }
    }

    pub(crate) fn begin(&self) -> bool {
        if self
            .state
            .compare_exchange(
                INKPOD_TASK_READY,
                INKPOD_TASK_RUNNING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            return true;
        }
        self.state
            .compare_exchange(
                INKPOD_TASK_CANCELLED,
                INKPOD_TASK_RUNNING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub(crate) fn progress(&self, completed: u64, total: u64) -> bool {
        self.total_work.store(total, Ordering::Release);
        self.completed_work
            .store(completed.min(total), Ordering::Release);
        !self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn finish(&self, status: u32) {
        let state = match status {
            INKPOD_STATUS_OK => INKPOD_TASK_COMPLETED,
            INKPOD_STATUS_CANCELLED => INKPOD_TASK_CANCELLED,
            _ => INKPOD_TASK_FAILED,
        };
        self.state.store(state, Ordering::Release);
    }
}
