use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::automations::automation_store::{FileAutomationStore, UserTimeZoneResolver, get_agent_automations_dir};
use crate::workflows::workflow_store::FileWorkflowStore;
use super::channel_store::{FileChannelStore, get_agent_channels_dir};

pub const AUTOMATIONS_DIRNAME: &str = "automations";
pub const WORKFLOWS_DIRNAME: &str = "workflows";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UnavailableMemoryRecall {
    pub profile: Vec<Value>,
    pub recent: Vec<Value>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableMemoryStore;

impl UnavailableMemoryStore {
    pub fn recall(&self) -> UnavailableMemoryRecall {
        UnavailableMemoryRecall::default()
    }

    pub fn list_memories(&self) -> Vec<Value> {
        Vec::new()
    }

    pub fn add_memory(&self) -> Option<Value> {
        None
    }

    pub fn remove_memory_by_content(&self) -> bool {
        false
    }

    pub fn get_location(&self) -> Option<PathBuf> {
        None
    }

    pub fn set_on_change(&self) {}

    pub fn remove_memory(&self) -> bool {
        false
    }

    pub fn clear_memories(&self) {}
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoSessionMemory;

impl NoSessionMemory {
    pub fn create_agent_store(&self) -> UnavailableMemoryStore {
        UnavailableMemoryStore
    }

    pub fn agent_has_content(&self, _agent_dir: &Path) -> bool {
        false
    }
}

pub const NO_SESSION_MEMORY: NoSessionMemory = NoSessionMemory;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStoreLocations {
    pub agent_dir: PathBuf,
    pub global_workflows_dir: PathBuf,
}

fn agent_dir_for_db_path(db_path: &Path) -> &Path {
    db_path.parent().unwrap_or_else(|| Path::new(""))
}

pub fn automation_store_location_for_db_path(db_path: &Path) -> PathBuf {
    agent_dir_for_db_path(db_path).join(AUTOMATIONS_DIRNAME)
}

pub fn automation_store_for_db_path(db_path: &Path) -> FileAutomationStore {
    automation_store_for_db_path_with_time_zone_resolver(db_path, std::sync::Arc::new(|| None))
}

pub fn automation_store_for_db_path_with_time_zone_resolver(
    db_path: &Path,
    resolve_user_time_zone: UserTimeZoneResolver,
) -> FileAutomationStore {
    FileAutomationStore::with_user_time_zone_resolver(
        get_agent_automations_dir(agent_dir_for_db_path(db_path)),
        resolve_user_time_zone,
    )
}

pub fn workflow_store_locations_for_db_path(db_path: &Path) -> WorkflowStoreLocations {
    let agent_dir = agent_dir_for_db_path(db_path).to_path_buf();
    let global_workflows_dir = agent_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(""))
        .join(WORKFLOWS_DIRNAME);
    WorkflowStoreLocations {
        agent_dir,
        global_workflows_dir,
    }
}

pub fn workflow_store_for_db_path(db_path: &Path) -> FileWorkflowStore {
    workflow_store_for_db_path_with_time_zone_resolver(db_path, std::sync::Arc::new(|| None))
}

pub fn workflow_store_for_db_path_with_time_zone_resolver(
    db_path: &Path,
    resolve_user_time_zone: UserTimeZoneResolver,
) -> FileWorkflowStore {
    let locations = workflow_store_locations_for_db_path(db_path);
    FileWorkflowStore::with_user_time_zone_resolver(
        locations.agent_dir,
        locations.global_workflows_dir,
        resolve_user_time_zone,
    )
}

pub fn channel_store_for_db_path(db_path: &Path) -> FileChannelStore {
    FileChannelStore::new(get_agent_channels_dir(agent_dir_for_db_path(db_path)))
}
