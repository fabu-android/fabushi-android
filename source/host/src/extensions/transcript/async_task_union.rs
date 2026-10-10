use std::collections::HashSet;

use serde::Serialize;

use super::sand_pending_wake_store::{DurablePendingWakeMarker, PendingWakeKind};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsyncTask {
    pub kind: String,
    pub id: String,
    pub label: String,
    pub status: String,
    pub started_at_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_type: Option<String>,
}

fn kind_name(kind: PendingWakeKind) -> &'static str {
    match kind {
        PendingWakeKind::CloudAgent => "cloud-agent",
        PendingWakeKind::Shell => "shell",
        PendingWakeKind::Subagent => "subagent",
    }
}

pub fn marker_label(marker: &DurablePendingWakeMarker) -> String {
    if let Some(title) = marker.title.as_deref().filter(|title| !title.is_empty()) {
        return title.to_string();
    }
    match marker.kind {
        PendingWakeKind::CloudAgent => format!("Cloud agent {}", marker.work_id),
        PendingWakeKind::Shell => format!("Background command {}", marker.work_id),
        PendingWakeKind::Subagent => format!("Background task {}", marker.work_id),
    }
}

pub fn pending_wake_marker_to_async_task(marker: &DurablePendingWakeMarker) -> AsyncTask {
    let detail = match marker.kind {
        PendingWakeKind::Shell if marker.interrupted_by_recreate => {
            Some("reattached after a host restart".to_string())
        }
        PendingWakeKind::Subagent => marker
            .subagent_type
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        PendingWakeKind::CloudAgent | PendingWakeKind::Shell => None,
    };
    AsyncTask {
        kind: kind_name(marker.kind).to_string(),
        id: marker.work_id.clone(),
        label: marker_label(marker),
        status: "running".to_string(),
        started_at_ms: marker.marked_at_ms,
        detail,
        subagent_type: (marker.kind == PendingWakeKind::Subagent)
            .then(|| {
                marker
                    .subagent_type
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            })
            .flatten(),
    }
}

pub fn merge_async_tasks(
    live_tasks: &[AsyncTask],
    markers: &[DurablePendingWakeMarker],
) -> Vec<AsyncTask> {
    let mut seen = live_tasks
        .iter()
        .map(|task| (task.kind.clone(), task.id.clone()))
        .collect::<HashSet<_>>();
    let mut merged = live_tasks.to_vec();
    for marker in markers {
        let key = (kind_name(marker.kind).to_string(), marker.work_id.clone());
        if !seen.insert(key.clone()) {
            if marker.kind == PendingWakeKind::Shell && marker.interrupted_by_recreate {
                if let Some(task) = merged
                    .iter_mut()
                    .find(|task| task.kind == key.0 && task.id == key.1)
                {
                    task.detail = Some("reattached after a host restart".to_string());
                }
            }
            continue;
        }
        merged.push(pending_wake_marker_to_async_task(marker));
    }
    merged.sort_by(|a, b| {
        a.started_at_ms
            .partial_cmp(&b.started_at_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(kind: PendingWakeKind, work_id: &str, started: f64) -> DurablePendingWakeMarker {
        DurablePendingWakeMarker {
            account_fence: "acct-a".into(),
            agent_id: "agent-a".into(),
            kind,
            work_id: work_id.into(),
            marked_at_ms: started,
            quiet_origin: None,
            title: None,
            subagent_type: None,
            interrupted_by_recreate: false,
        }
    }

    #[test]
    fn union_projects_all_background_kinds_and_stable_defaults() {
        let markers = vec![
            marker(PendingWakeKind::CloudAgent, "cloud-1", 30.0),
            marker(PendingWakeKind::Shell, "shell-1", 20.0),
            marker(PendingWakeKind::Subagent, "sub-1", 10.0),
        ];
        let tasks = merge_async_tasks(&[], &markers);
        assert_eq!(
            tasks.iter().map(|task| task.kind.as_str()).collect::<Vec<_>>(),
            vec!["subagent", "shell", "cloud-agent"]
        );
        assert_eq!(tasks[0].label, "Background task sub-1");
        assert_eq!(tasks[1].label, "Background command shell-1");
        assert_eq!(tasks[2].label, "Cloud agent cloud-1");
    }

    #[test]
    fn live_task_wins_identity_but_restart_detail_is_enriched() {
        let live = vec![AsyncTask {
            kind: "shell".into(),
            id: "shell-1".into(),
            label: "Live shell".into(),
            status: "running".into(),
            started_at_ms: 1.0,
            detail: None,
            subagent_type: None,
        }];
        let mut durable = marker(PendingWakeKind::Shell, "shell-1", 2.0);
        durable.interrupted_by_recreate = true;
        let tasks = merge_async_tasks(&live, &[durable]);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].label, "Live shell");
        assert_eq!(
            tasks[0].detail.as_deref(),
            Some("reattached after a host restart")
        );
    }
}
