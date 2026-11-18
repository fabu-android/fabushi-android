//! Dedicated Parallelized Media/File Chunk Loader
//! Upstream Source: `Telegram/SourceFiles/mtproto/dedicated_file_loader.h`, `dedicated_file_loader.cpp`

use crate::mtproto::core_types::DcId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DedicatedFileLoader {
    pub dc_id: DcId,
    pub location_id: u64,
    pub total_size: usize,
    pub downloaded_bytes: usize,
    pub chunk_size: usize,
    pub is_downloading: bool,
    pub is_finished: bool,
}

impl Default for DedicatedFileLoader {
    fn default() -> Self {
        Self {
            dc_id: 2,
            location_id: 0,
            total_size: 0,
            downloaded_bytes: 0,
            chunk_size: 128 * 1024, // 128 KB standard chunk
            is_downloading: false,
            is_finished: false,
        }
    }
}

impl DedicatedFileLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_downloading && !self.is_finished
    }

    pub fn start_download(&mut self, dc_id: DcId, location_id: u64, total_size: usize) {
        self.dc_id = dc_id;
        self.location_id = location_id;
        self.total_size = total_size;
        self.downloaded_bytes = 0;
        self.is_downloading = true;
        self.is_finished = false;
    }

    pub fn feed_chunk(&mut self, bytes_count: usize) {
        self.downloaded_bytes = (self.downloaded_bytes + bytes_count).min(self.total_size);
        if self.downloaded_bytes >= self.total_size && self.total_size > 0 {
            self.is_finished = true;
            self.is_downloading = false;
        }
    }

    pub fn progress(&self) -> f64 {
        if self.total_size == 0 {
            0.0
        } else {
            (self.downloaded_bytes as f64) / (self.total_size as f64)
        }
    }

    pub fn cancel(&mut self) {
        self.is_downloading = false;
        self.is_finished = false;
    }
}
