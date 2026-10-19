use chrono::{Local, TimeZone, Utc};
use chrono_tz::Tz;

pub use super::automation::AUTOMATION_STATUS_PROMPT_MARKER;
use super::automation::{AUTOMATION_UI_LIMIT, AutomationRecord, AutomationRun};
use super::automation_store::FileAutomationStore;


pub fn render_automation_cleared_status_reminder() -> String {
    [
        "<system_reminder>",
        AUTOMATION_STATUS_PROMPT_MARKER,
        "Current routine runtime status. This snapshot is authoritative for this turn and supersedes earlier routine status reminders.",
        "No current routines.",
        "</automation_status>",
        "</system_reminder>",
    ]
    .join("\n")
}

pub fn render_automation_runtime_status_reminder(
    automations: &[AutomationRecord],
    time_zone: Option<&str>,
    firing_automation_id: Option<&str>,
) -> Option<String> {
    if automations.is_empty() {
        return None;
    }

    let mut lines = vec![
        "<system_reminder>".to_string(),
        AUTOMATION_STATUS_PROMPT_MARKER.to_string(),
        "Current routine runtime status. This snapshot is authoritative for this turn and supersedes earlier routine status reminders.".to_string(),
    ];

    for automation in automations {
        let next = if automation.is_enabled {
            automation
                .next_run_at
                .map(|value| format!("next run {}; ", format_timestamp(value, time_zone)))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let visible_runs = if firing_automation_id == Some(automation.id.as_str()) {
            automation
                .runs
                .iter()
                .filter(|run| run.status != "running")
                .collect::<Vec<_>>()
        } else {
            automation.runs.iter().collect::<Vec<_>>()
        };
        lines.push(format!(
            "- {} (folder {}): {}{}",
            automation.name,
            automation.id,
            next,
            summarize_last_run(&visible_runs, time_zone),
        ));
    }

    lines.push("</automation_status>".to_string());
    lines.push("</system_reminder>".to_string());
    Some(lines.join("\n"))
}

pub fn create_automation_status_reminder(
    store: &FileAutomationStore,
    firing_automation_id: Option<&str>,
) -> Option<String> {
    if store.get_location().as_os_str().is_empty() {
        return None;
    }
    let definitions = store.list_definitions();
    let capped = &definitions[..definitions.len().min(AUTOMATION_UI_LIMIT)];
    let time_zone = store.resolved_user_time_zone();
    render_automation_runtime_status_reminder(
        capped,
        time_zone.as_deref(),
        firing_automation_id,
    )
}

fn summarize_last_run(runs: &[&AutomationRun], time_zone: Option<&str>) -> String {
    let Some(last) = runs.first() else {
        return "never run".to_string();
    };
    if last.status == "running" {
        return format!(
            "running now (started {})",
            format_timestamp(last.started_at, time_zone)
        );
    }
    format!(
        "last run {} ({})",
        format_timestamp(last.started_at, time_zone),
        if last.status == "ok" {
            "succeeded"
        } else {
            "failed"
        },
    )
}

fn format_timestamp(ms: f64, time_zone: Option<&str>) -> String {
    if !ms.is_finite() || ms < i64::MIN as f64 || ms > i64::MAX as f64 {
        return "never".to_string();
    }
    let Some(utc) = Utc.timestamp_millis_opt(ms.trunc() as i64).single() else {
        return "never".to_string();
    };

    if let Some(zone) = time_zone
        .and_then(|value| value.trim().parse::<Tz>().ok())
    {
        return utc
            .with_timezone(&zone)
            .format("%m/%d/%Y, %I:%M:%S %p")
            .to_string();
    }

    utc.with_timezone(&Local)
        .format("%m/%d/%Y, %I:%M:%S %p")
        .to_string()
}
