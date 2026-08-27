use crate::{IoError, IoResult};

/// Hard application-wide limits, including cached values still held by a consumer.
/// Smaller values are supported for constrained machines and deterministic tests.
#[derive(Clone, Debug)]
pub struct IoConfig {
    pub max_images: usize,
    pub max_file_bytes: u64,
    pub max_encoded_bytes: u64,
    pub max_decoded_bytes: u64,
    pub worker_count: usize,
    pub queue_capacity: usize,
}

impl Default for IoConfig {
    fn default() -> Self {
        Self {
            max_images: 10_000,
            max_file_bytes: 512 * 1024 * 1024,
            max_encoded_bytes: 8 * 1024 * 1024 * 1024,
            max_decoded_bytes: 8 * 1024 * 1024 * 1024,
            worker_count: std::thread::available_parallelism()
                .map_or(2, usize::from)
                .clamp(1, 8),
            queue_capacity: 256,
        }
    }
}

impl IoConfig {
    pub(crate) fn validate(&self) -> IoResult<()> {
        if self.max_images == 0 || self.max_images > 10_000 {
            return Err(IoError::InvalidInput(
                "image cache count is outside 1..=10000",
            ));
        }
        if self.max_file_bytes == 0 || self.max_file_bytes > 512 * 1024 * 1024 {
            return Err(IoError::InvalidInput("image file limit exceeds 512 MiB"));
        }
        if self.max_encoded_bytes == 0
            || self.max_decoded_bytes == 0
            || self.max_encoded_bytes > 8 * 1024 * 1024 * 1024
            || self.max_decoded_bytes > 8 * 1024 * 1024 * 1024
        {
            return Err(IoError::InvalidInput(
                "image cache byte limits exceed 8 GiB",
            ));
        }
        if !(1..=64).contains(&self.worker_count) || !(1..=65_536).contains(&self.queue_capacity) {
            return Err(IoError::InvalidInput(
                "I/O worker or queue limit is invalid",
            ));
        }
        Ok(())
    }
}
