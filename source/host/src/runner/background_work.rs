use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn background_task_started_at_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundAsyncTaskSnapshot {
    pub id: String,
    pub label: String,
    pub started_at_ms: u64,
}

pub const SHELL_REWATCH_POLL_DEFAULT_MS: u64 = 10_000;
pub const SHELL_REWATCH_MAX_WAIT_MS: u64 = 5 * 60 * 60 * 1_000;
pub const SHELL_REWATCH_MISSING_FILE_GIVE_UP: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellTerminalFooter {
    pub is_complete: bool,
    pub exit_code: Option<i64>,
    pub is_stream_failure: bool,
}

impl Default for ShellTerminalFooter {
    fn default() -> Self {
        Self {
            is_complete: false,
            exit_code: None,
            is_stream_failure: false,
        }
    }
}

#[derive(Clone)]
pub struct BackgroundWorkRecord {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub owner_id: Option<String>,
    pub metadata: Option<Value>,
    pub abort: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl std::fmt::Debug for BackgroundWorkRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BackgroundWorkRecord")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("state", &self.state)
            .field("owner_id", &self.owner_id)
            .field("metadata", &self.metadata)
            .field("has_abort", &self.abort.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundWakeupPayload {
    pub kind: String,
    pub reason: Option<String>,
    pub task_id: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub detail: Option<String>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundWakeup {
    pub id: Option<String>,
    pub conversation_id: Option<String>,
    pub payload: BackgroundWakeupPayload,
}

pub type ShellCompletionCallback =
    Arc<dyn Fn(&BackgroundWakeupPayload, Option<&str>) + Send + Sync>;
pub type ShellWorkRegisteredCallback =
    Arc<dyn Fn(&BackgroundWorkRecord, Option<&str>) + Send + Sync>;
pub type WorkSetChangedCallback = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub struct RevivingBackgroundWorkRegistry {
    work: HashMap<String, BackgroundWorkRecord>,
    wakeups: Vec<BackgroundWakeup>,
    completions: Vec<BackgroundWakeupPayload>,
    quiet_origins: HashMap<String, String>,
    on_shell_completion: Option<ShellCompletionCallback>,
    on_shell_work_registered: Option<ShellWorkRegisteredCallback>,
    on_work_set_changed: Option<WorkSetChangedCallback>,
}

impl RevivingBackgroundWorkRegistry {
    pub fn with_callbacks(
        on_shell_completion: Option<ShellCompletionCallback>,
        on_shell_work_registered: Option<ShellWorkRegisteredCallback>,
        on_work_set_changed: Option<WorkSetChangedCallback>,
    ) -> Self {
        Self {
            on_shell_completion,
            on_shell_work_registered,
            on_work_set_changed,
            ..Self::default()
        }
    }

    pub fn upsert_work(&mut self, record: BackgroundWorkRecord) {
        self.work.insert(record.id.clone(), record);
        self.changed();
    }

    pub fn upsert_work_for_turn(
        &mut self,
        record: BackgroundWorkRecord,
        quiet_origin: Option<&str>,
    ) {
        self.record_work_origin(&record.id, quiet_origin);
        let is_running_shell = record.kind == "shell" && record.state == "running";
        self.upsert_work(record.clone());
        if is_running_shell {
            if let Some(callback) = self.on_shell_work_registered.as_ref() {
                callback(&record, quiet_origin);
            }
        }
    }

    pub fn clear_work(&mut self, id: &str) -> Option<BackgroundWorkRecord> {
        let value = self.work.remove(id);
        self.changed();
        value
    }

    pub fn abort_work(&mut self, id: &str) -> bool {
        let Some(record) = self.work.remove(id) else {
            return false;
        };
        if let Some(abort) = record.abort {
            abort();
        }
        self.changed();
        true
    }

    pub fn abort_all_work(&mut self, kind: Option<&str>) -> usize {
        let ids = self
            .list_work(kind)
            .into_iter()
            .map(|record| record.id.clone())
            .collect::<Vec<_>>();
        for id in &ids {
            if let Some(record) = self.work.remove(id) {
                if let Some(abort) = record.abort {
                    abort();
                }
            }
        }
        self.changed();
        ids.len()
    }

    pub fn has_running_work(&self, kind: Option<&str>) -> bool {
        self.list_work(kind)
            .into_iter()
            .any(|record| record.state == "running")
    }

    pub fn list_work(&self, kind: Option<&str>) -> Vec<&BackgroundWorkRecord> {
        self.work
            .values()
            .filter(|record| kind.is_none_or(|kind| record.kind == kind))
            .collect()
    }

    pub fn enqueue(&mut self, wakeup: BackgroundWakeup) {
        let payload = &wakeup.payload;
        if payload.kind == "shell" && payload.reason.as_deref() != Some("task_progress") {
            let origin = self.quiet_origins.remove(&payload.task_id);
            if let Some(callback) = self.on_shell_completion.as_ref() {
                callback(payload, origin.as_deref());
            }
            return;
        }
        self.wakeups.push(wakeup);
    }

    pub fn pull(&self, conversation_id: &str) -> Vec<BackgroundWakeup> {
        self.wakeups
            .iter()
            .filter(|item| {
                item.conversation_id
                    .as_deref()
                    .is_none_or(|candidate| candidate == conversation_id)
            })
            .cloned()
            .collect()
    }

    pub fn ack(&mut self, ids: &[String]) {
        let selected = ids.iter().map(String::as_str).collect::<HashSet<_>>();
        self.wakeups
            .retain(|wakeup| !wakeup.id.as_deref().is_some_and(|id| selected.contains(id)));
    }

    pub fn nack(&mut self, _ids: &[String]) {}

    pub fn suppress(&mut self, conversation_id: &str, id: &str) {
        self.wakeups.retain(|wakeup| {
            !(wakeup.conversation_id.as_deref() == Some(conversation_id)
                && wakeup.id.as_deref() == Some(id))
        });
    }

    pub fn enqueue_completion(&mut self, item: BackgroundWakeupPayload) {
        self.completions.push(item);
    }

    pub fn drain_completions(&mut self) -> Vec<BackgroundWakeupPayload> {
        std::mem::take(&mut self.completions)
    }

    pub fn has_pending_completions(&self, _conversation_id: &str) -> bool {
        !self.completions.is_empty()
    }

    pub fn mark_awaited_completion(&mut self, _task_id: &str) {}

    pub fn record_work_origin(&mut self, id: &str, origin: Option<&str>) {
        match origin {
            Some(origin) => {
                self.quiet_origins.insert(id.to_string(), origin.to_string());
            }
            None => {
                self.quiet_origins.remove(id);
            }
        }
    }

    fn changed(&self) {
        if let Some(callback) = self.on_work_set_changed.as_ref() {
            callback();
        }
    }
}

pub fn shell_rewatch_poll_ms(raw: Option<&str>) -> u64 {
    raw.and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(SHELL_REWATCH_POLL_DEFAULT_MS)
}

pub fn parse_shell_terminal_footer(content: &str) -> ShellTerminalFooter {
    let Some(marker_start) = content.rfind("\n---\n") else {
        return ShellTerminalFooter::default();
    };
    let footer_and_end = &content[marker_start + 5..];
    let Some(footer) = footer_and_end.strip_suffix("\n---")
        .or_else(|| footer_and_end.strip_suffix("\n---\n"))
        .or_else(|| footer_and_end.strip_suffix("\n---\r\n"))
    else {
        return ShellTerminalFooter::default();
    };

    for line in footer.lines() {
        if let Some(raw) = line.strip_prefix("exit_code:") {
            let raw = raw.trim();
            return ShellTerminalFooter {
                is_complete: true,
                exit_code: (!raw.is_empty())
                    .then(|| raw.parse::<i64>().ok())
                    .flatten(),
                is_stream_failure: false,
            };
        }
    }

    let has_error = footer.lines().any(|line| line.starts_with("error:"));
    let has_ended_at = footer
        .lines()
        .any(|line| line.strip_prefix("ended_at:").is_some_and(|value| !value.trim().is_empty()));
    if has_error && has_ended_at {
        ShellTerminalFooter {
            is_complete: true,
            exit_code: None,
            is_stream_failure: true,
        }
    } else {
        ShellTerminalFooter::default()
    }
}

pub fn derive_background_subagent_title(prompt: &str) -> String {
    let one_line = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.is_empty() {
        return "Background task".to_string();
    }
    if one_line.chars().count() <= 80 {
        return one_line;
    }
    let prefix = one_line.chars().take(79).collect::<String>();
    format!("{prefix}…")
}

pub fn format_steer_prompt(message: &str) -> String {
    [
        "[Steering message from the parent agent that dispatched you]",
        message.trim(),
        "Take this into account and continue your task from where you are — do not start over.",
    ]
    .join("\n\n")
}


#[derive(Debug, Clone, PartialEq)]
pub struct CloudAgentWatchOptions {
    pub quiet_origin: Option<Value>,
    pub after_followup: bool,
}

impl CloudAgentWatchOptions {
    pub fn new(quiet_origin: Option<Value>, after_followup: bool) -> Self {
        Self {
            quiet_origin,
            after_followup,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloudAgentPendingWatch {
    pub parent_agent_id: String,
    pub work_id: String,
    pub title: String,
    pub quiet_origin: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloudAgentWatchOutcome {
    pub status: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloudAgentBackgroundCompletion {
    pub parent_agent_id: String,
    pub work_id: String,
    pub title: String,
    pub status: String,
    pub result: String,
    pub quiet_origin: Option<Value>,
}

pub type CloudAgentAwaitCallback =
    Arc<dyn Fn(&str, bool) -> CloudAgentWatchOutcome + Send + Sync>;
pub type CloudAgentPendingCallback =
    Arc<dyn Fn(&CloudAgentPendingWatch) + Send + Sync>;
pub type CloudAgentSettledCallback =
    Arc<dyn Fn(CloudAgentBackgroundCompletion) + Send + Sync>;
pub type CloudAgentAsyncTasksChangedCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Debug, Clone)]
struct ArmedCloudAgentWatch {
    generation: u64,
    parent_agent_id: String,
    work_id: String,
    title: String,
    started_at_ms: u64,
    quiet_origin: Option<Value>,
}

#[derive(Default)]
struct CloudAgentWatchState {
    next_generation: u64,
    armed: HashMap<String, ArmedCloudAgentWatch>,
}

#[derive(Clone)]
pub struct RunnerCloudAgentWatches {
    state: Arc<Mutex<CloudAgentWatchState>>,
    await_completion: CloudAgentAwaitCallback,
    on_pending: Option<CloudAgentPendingCallback>,
    on_settled: Option<CloudAgentSettledCallback>,
    on_async_tasks_changed: Option<CloudAgentAsyncTasksChangedCallback>,
}

impl RunnerCloudAgentWatches {
    pub fn new(
        await_completion: CloudAgentAwaitCallback,
        on_pending: Option<CloudAgentPendingCallback>,
        on_settled: Option<CloudAgentSettledCallback>,
        on_async_tasks_changed: Option<CloudAgentAsyncTasksChangedCallback>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(CloudAgentWatchState::default())),
            await_completion,
            on_pending,
            on_settled,
            on_async_tasks_changed,
        }
    }

    pub fn watch_cloud_agent(
        &self,
        parent_agent_id: &str,
        bc_id: &str,
        options: CloudAgentWatchOptions,
    ) -> bool {
        let parent_agent_id = parent_agent_id.trim();
        let bc_id = bc_id.trim();
        if parent_agent_id.is_empty() || bc_id.is_empty() {
            return false;
        }

        let key = cloud_agent_watch_key(parent_agent_id, bc_id);
        let armed = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.armed.contains_key(&key) {
                return false;
            }
            state.next_generation = state.next_generation.saturating_add(1);
            let armed = ArmedCloudAgentWatch {
                generation: state.next_generation,
                parent_agent_id: parent_agent_id.to_string(),
                work_id: bc_id.to_string(),
                title: format!("Cloud agent {bc_id}"),
                started_at_ms: background_task_started_at_ms(),
                quiet_origin: options.quiet_origin.clone(),
            };
            state.armed.insert(key.clone(), armed.clone());
            armed
        };

        if let Some(callback) = self.on_pending.as_ref() {
            callback(&CloudAgentPendingWatch {
                parent_agent_id: armed.parent_agent_id.clone(),
                work_id: armed.work_id.clone(),
                title: armed.title.clone(),
                quiet_origin: armed.quiet_origin.clone(),
            });
        }
        self.emit_async_tasks_changed(parent_agent_id);

        let state = Arc::clone(&self.state);
        let await_completion = Arc::clone(&self.await_completion);
        let on_settled = self.on_settled.clone();
        let on_async_tasks_changed = self.on_async_tasks_changed.clone();
        let after_followup = options.after_followup;
        let generation = armed.generation;
        let parent_agent_id = armed.parent_agent_id.clone();
        let work_id = armed.work_id.clone();
        let title = armed.title.clone();
        let quiet_origin = armed.quiet_origin.clone();

        let _ = std::thread::Builder::new()
            .name(format!("mahayana-cloud-agent-watch-{work_id}"))
            .spawn(move || {
                let outcome = await_completion(&work_id, after_followup);
                let still_owned = {
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let owned = state
                        .armed
                        .get(&key)
                        .is_some_and(|current| current.generation == generation);
                    if owned {
                        state.armed.remove(&key);
                    }
                    owned
                };
                if !still_owned {
                    return;
                }
                // Settle the live owner before publishing the task-set change.
                // The shipping settlement callback clears the durable pending-wake
                // projection, so observers cannot briefly resurrect a completed
                // cloud task from the recovery ledger.
                if let Some(callback) = on_settled.as_ref() {
                    callback(CloudAgentBackgroundCompletion {
                        parent_agent_id: parent_agent_id.clone(),
                        work_id,
                        title,
                        status: if outcome.status == "error" {
                            "error".into()
                        } else {
                            "completed".into()
                        },
                        result: if outcome.text.trim().is_empty() {
                            "(the cloud agent finished without producing any output)".into()
                        } else {
                            outcome.text.trim().to_string()
                        },
                        quiet_origin,
                    });
                }
                if let Some(callback) = on_async_tasks_changed.as_ref() {
                    callback(&parent_agent_id);
                }
            });
        true
    }

    pub fn is_cloud_watch_armed(&self, parent_agent_id: &str, bc_id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .armed
            .contains_key(&cloud_agent_watch_key(parent_agent_id.trim(), bc_id.trim()))
    }

    pub fn pending_cloud_agent_watch_ids(&self, parent_agent_id: &str) -> Vec<String> {
        let mut ids = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .armed
            .values()
            .filter(|watch| watch.parent_agent_id == parent_agent_id)
            .map(|watch| watch.work_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    pub fn async_task_snapshots(&self, parent_agent_id: &str) -> Vec<BackgroundAsyncTaskSnapshot> {
        let mut tasks = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .armed
            .values()
            .filter(|watch| watch.parent_agent_id == parent_agent_id)
            .map(|watch| BackgroundAsyncTaskSnapshot {
                id: watch.work_id.clone(),
                label: watch.title.clone(),
                started_at_ms: watch.started_at_ms,
            })
            .collect::<Vec<_>>();
        tasks.sort_by(|a, b| {
            a.started_at_ms
                .cmp(&b.started_at_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        tasks
    }

    pub fn cancel_cloud_watch(&self, parent_agent_id: &str, bc_id: &str) -> bool {
        let removed = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .armed
            .remove(&cloud_agent_watch_key(parent_agent_id.trim(), bc_id.trim()))
            .is_some();
        if removed {
            self.emit_async_tasks_changed(parent_agent_id);
        }
        removed
    }

    pub fn dispose_parent(&self, parent_agent_id: &str) -> usize {
        let removed = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let before = state.armed.len();
            state
                .armed
                .retain(|_, watch| watch.parent_agent_id != parent_agent_id);
            before.saturating_sub(state.armed.len())
        };
        if removed > 0 {
            self.emit_async_tasks_changed(parent_agent_id);
        }
        removed
    }

    fn emit_async_tasks_changed(&self, parent_agent_id: &str) {
        if let Some(callback) = self.on_async_tasks_changed.as_ref() {
            callback(parent_agent_id);
        }
    }
}

fn cloud_agent_watch_key(parent_agent_id: &str, bc_id: &str) -> String {
    format!("{parent_agent_id}\0{bc_id}")
}


#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundShellWatchOptions {
    pub title: Option<String>,
    pub quiet_origin: Option<Value>,
}

impl BackgroundShellWatchOptions {
    pub fn new(title: Option<String>, quiet_origin: Option<Value>) -> Self {
        Self { title, quiet_origin }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundShellPendingWatch {
    pub parent_agent_id: String,
    pub work_id: String,
    pub title: String,
    pub quiet_origin: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundShellWatchOutcome {
    pub status: String,
    pub detail: Option<String>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundShellBackgroundCompletion {
    pub parent_agent_id: String,
    pub work_id: String,
    pub title: String,
    pub status: String,
    pub detail: Option<String>,
    pub output_path: Option<String>,
    pub quiet_origin: Option<Value>,
}

pub type BackgroundShellAwaitCallback = Arc<
    dyn Fn(&str, &str, Arc<AtomicBool>) -> Option<BackgroundShellWatchOutcome>
        + Send
        + Sync,
>;
pub type BackgroundShellPendingCallback =
    Arc<dyn Fn(&BackgroundShellPendingWatch) + Send + Sync>;
pub type BackgroundShellSettledCallback =
    Arc<dyn Fn(BackgroundShellBackgroundCompletion) + Send + Sync>;
pub type BackgroundShellAsyncTasksChangedCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
struct ArmedBackgroundShellWatch {
    generation: u64,
    parent_agent_id: String,
    work_id: String,
    title: String,
    started_at_ms: u64,
    quiet_origin: Option<Value>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct BackgroundShellWatchState {
    next_generation: u64,
    armed: HashMap<String, ArmedBackgroundShellWatch>,
}

/// Single shipping owner for Grok-style background-shell terminal rewatches.
///
/// The durable pending marker is armed before duplicate detection, matching
/// frozen Grok 0.18. This is important during Host recreate because
/// PendingWakeRearm clears the old marker before invoking this owner.
#[derive(Clone)]
pub struct RunnerBackgroundShellWatches {
    state: Arc<Mutex<BackgroundShellWatchState>>,
    await_terminal: BackgroundShellAwaitCallback,
    on_pending: Option<BackgroundShellPendingCallback>,
    on_settled: Option<BackgroundShellSettledCallback>,
    on_async_tasks_changed: Option<BackgroundShellAsyncTasksChangedCallback>,
}

impl RunnerBackgroundShellWatches {
    pub fn new(
        await_terminal: BackgroundShellAwaitCallback,
        on_pending: Option<BackgroundShellPendingCallback>,
        on_settled: Option<BackgroundShellSettledCallback>,
        on_async_tasks_changed: Option<BackgroundShellAsyncTasksChangedCallback>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(BackgroundShellWatchState::default())),
            await_terminal,
            on_pending,
            on_settled,
            on_async_tasks_changed,
        }
    }

    pub fn watch_background_shell(
        &self,
        parent_agent_id: &str,
        shell_id: &str,
        options: BackgroundShellWatchOptions,
    ) -> bool {
        let parent_agent_id = parent_agent_id.trim();
        let shell_id = shell_id.trim();
        if parent_agent_id.is_empty() || shell_id.is_empty() {
            return false;
        }

        let title = options
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Background command {shell_id}"));

        let key = background_shell_watch_key(parent_agent_id, shell_id);
        let armed = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.armed.contains_key(&key) {
                return false;
            }
            state.next_generation = state.next_generation.saturating_add(1);
            let armed = ArmedBackgroundShellWatch {
                generation: state.next_generation,
                parent_agent_id: parent_agent_id.to_string(),
                work_id: shell_id.to_string(),
                title,
                started_at_ms: background_task_started_at_ms(),
                quiet_origin: options.quiet_origin,
                cancelled: Arc::new(AtomicBool::new(false)),
            };
            state.armed.insert(key.clone(), armed.clone());
            armed
        };

        // Persist/project only after this owner successfully claims the watch.
        // Duplicate restart rearm attempts therefore cannot emit a second
        // durable pending marker or duplicate async-task projection.
        if let Some(callback) = self.on_pending.as_ref() {
            callback(&BackgroundShellPendingWatch {
                parent_agent_id: armed.parent_agent_id.clone(),
                work_id: armed.work_id.clone(),
                title: armed.title.clone(),
                quiet_origin: armed.quiet_origin.clone(),
            });
        }
        self.emit_async_tasks_changed(parent_agent_id);

        let state = Arc::clone(&self.state);
        let await_terminal = Arc::clone(&self.await_terminal);
        let on_settled = self.on_settled.clone();
        let on_async_tasks_changed = self.on_async_tasks_changed.clone();
        let generation = armed.generation;
        let parent_agent_id = armed.parent_agent_id.clone();
        let work_id = armed.work_id.clone();
        let title = armed.title.clone();
        let quiet_origin = armed.quiet_origin.clone();
        let cancelled = Arc::clone(&armed.cancelled);

        let _ = std::thread::Builder::new()
            .name(format!("mahayana-background-shell-watch-{work_id}"))
            .spawn(move || {
                let outcome =
                    await_terminal(&parent_agent_id, &work_id, Arc::clone(&cancelled));
                if cancelled.load(Ordering::Acquire) {
                    return;
                }
                let still_owned = {
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let owned = state
                        .armed
                        .get(&key)
                        .is_some_and(|current| current.generation == generation);
                    if owned {
                        state.armed.remove(&key);
                    }
                    owned
                };
                if !still_owned {
                    return;
                }
                // Settle the live owner before publishing the task-set change.
                // The settlement callback clears the durable recovery projection
                // first, preventing a finished shell from being re-emitted as
                // running by the async-task observer.
                if let (Some(callback), Some(outcome)) = (on_settled.as_ref(), outcome) {
                    callback(BackgroundShellBackgroundCompletion {
                        parent_agent_id: parent_agent_id.clone(),
                        work_id,
                        title,
                        status: outcome.status,
                        detail: outcome.detail,
                        output_path: outcome.output_path,
                        quiet_origin,
                    });
                }
                if let Some(callback) = on_async_tasks_changed.as_ref() {
                    callback(&parent_agent_id);
                }
            });
        true
    }

    pub fn is_shell_watch_armed(&self, parent_agent_id: &str, shell_id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .armed
            .contains_key(&background_shell_watch_key(
                parent_agent_id.trim(),
                shell_id.trim(),
            ))
    }

    pub fn pending_shell_rewatch_ids(&self, parent_agent_id: &str) -> Vec<String> {
        let mut ids = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .armed
            .values()
            .filter(|watch| watch.parent_agent_id == parent_agent_id)
            .map(|watch| watch.work_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }

    pub fn async_task_snapshots(&self, parent_agent_id: &str) -> Vec<BackgroundAsyncTaskSnapshot> {
        let mut tasks = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .armed
            .values()
            .filter(|watch| watch.parent_agent_id == parent_agent_id)
            .map(|watch| BackgroundAsyncTaskSnapshot {
                id: watch.work_id.clone(),
                label: watch.title.clone(),
                started_at_ms: watch.started_at_ms,
            })
            .collect::<Vec<_>>();
        tasks.sort_by(|a, b| {
            a.started_at_ms
                .cmp(&b.started_at_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        tasks
    }

    pub fn has_running_background_shell_work(&self) -> bool {
        !self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .armed
            .is_empty()
    }

    pub fn cancel_shell_watch(&self, parent_agent_id: &str, shell_id: &str) -> bool {
        let removed = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .armed
            .remove(&background_shell_watch_key(
                parent_agent_id.trim(),
                shell_id.trim(),
            ));
        let Some(removed) = removed else {
            return false;
        };
        removed.cancelled.store(true, Ordering::Release);
        self.emit_async_tasks_changed(parent_agent_id);
        true
    }

    pub fn dispose_parent(&self, parent_agent_id: &str) -> usize {
        let removed = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let keys = state
                .armed
                .iter()
                .filter_map(|(key, watch)| {
                    (watch.parent_agent_id == parent_agent_id).then_some(key.clone())
                })
                .collect::<Vec<_>>();
            for key in &keys {
                if let Some(watch) = state.armed.remove(key) {
                    watch.cancelled.store(true, Ordering::Release);
                }
            }
            keys.len()
        };
        if removed > 0 {
            self.emit_async_tasks_changed(parent_agent_id);
        }
        removed
    }

    fn emit_async_tasks_changed(&self, parent_agent_id: &str) {
        if let Some(callback) = self.on_async_tasks_changed.as_ref() {
            callback(parent_agent_id);
        }
    }
}

fn background_shell_watch_key(parent_agent_id: &str, shell_id: &str) -> String {
    format!("{parent_agent_id}\0{shell_id}")
}
