use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::agent_worker_pool::{
    AgentBlobWorkerBackend, AgentWorkerPool, AgentWorkerPoolError,
};

pub struct WorkerBlobStore<Backend>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    pub pool: Arc<AgentWorkerPool<Backend>>,
    pub agent_id: String,
    pub blob_db_path: PathBuf,
    pub legacy_blob_db_path: Option<PathBuf>,
}

impl<Backend> WorkerBlobStore<Backend>
where
    Backend: AgentBlobWorkerBackend + 'static,
{
    pub fn new(
        pool: Arc<AgentWorkerPool<Backend>>,
        agent_id: impl Into<String>,
        blob_db_path: impl Into<PathBuf>,
        legacy_blob_db_path: Option<PathBuf>,
    ) -> Self {
        Self {
            pool,
            agent_id: agent_id.into(),
            blob_db_path: blob_db_path.into(),
            legacy_blob_db_path,
        }
    }

    fn legacy_blob_db_path(&self) -> Option<&Path> {
        self.legacy_blob_db_path.as_deref()
    }

    pub fn get_blob_blocking(
        &self,
        blob_id: &[u8],
    ) -> Result<Option<Vec<u8>>, AgentWorkerPoolError<Backend::Error>> {
        self.pool.get_blob_blocking(
            &self.agent_id,
            &self.blob_db_path,
            blob_id,
            self.legacy_blob_db_path(),
        )
    }

    pub async fn get_blob<Ctx>(
        &self,
        _ctx: &Ctx,
        blob_id: &[u8],
    ) -> Result<Option<Vec<u8>>, AgentWorkerPoolError<Backend::Error>> {
        self.get_blob_blocking(blob_id)
    }

    pub fn set_blob_blocking(
        &self,
        blob_id: &[u8],
        blob_data: &[u8],
    ) -> Result<(), AgentWorkerPoolError<Backend::Error>> {
        self.pool.set_blob_blocking(
            &self.agent_id,
            &self.blob_db_path,
            blob_id,
            blob_data,
            self.legacy_blob_db_path(),
        )
    }

    pub async fn set_blob<Ctx>(
        &self,
        _ctx: &Ctx,
        blob_id: &[u8],
        blob_data: &[u8],
    ) -> Result<(), AgentWorkerPoolError<Backend::Error>> {
        self.set_blob_blocking(blob_id, blob_data)
    }

    pub async fn set_blob_locally_only<Ctx>(
        &self,
        ctx: &Ctx,
        blob_id: &[u8],
        blob_data: &[u8],
    ) -> Result<(), AgentWorkerPoolError<Backend::Error>> {
        self.set_blob(ctx, blob_id, blob_data).await
    }

    pub fn flush_blocking(&self) {}

    pub async fn flush<Ctx>(&self, _ctx: &Ctx) {
        self.flush_blocking();
    }
}
