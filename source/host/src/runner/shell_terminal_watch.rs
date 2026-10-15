use thiserror::Error;

use super::background_work::{
    SHELL_REWATCH_MAX_WAIT_MS, SHELL_REWATCH_MISSING_FILE_GIVE_UP,
    parse_shell_terminal_footer,
};
use super::conversation_state::{
    RecentUserMessage, select_unconfirmed_user_messages,
};
use super::sand_prompt_markers::SAND_HIDDEN_PROMPT_MARKER;
use super::system_prompt::build_user_message_address_note;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellTerminalPollRead {
    Snapshot {
        output_path: String,
        result: TerminalReadResult,
    },
    PermissionDenied {
        output_path: Option<String>,
    },
    TransientFailure(String),
}

/// Blocking production poll loop for the frozen Grok shell rewatch state
/// machine. Host supplies box readiness/read I/O and clock/delay; Runner owns
/// terminal settlement semantics. Non-permission read failures are retried
/// until the five-hour deadline.
pub fn poll_shell_terminal_file<Read, Now, Delay, Cancelled>(
    started_at_ms: u64,
    poll_ms: u64,
    mut read: Read,
    mut now_ms: Now,
    mut delay: Delay,
    mut is_cancelled: Cancelled,
) -> Option<ShellWatchSettlement>
where
    Read: FnMut() -> ShellTerminalPollRead,
    Now: FnMut() -> u64,
    Delay: FnMut(u64),
    Cancelled: FnMut() -> bool,
{
    let mut state = ShellTerminalPollState::new(started_at_ms);
    loop {
        if is_cancelled() {
            return None;
        }

        match read() {
            ShellTerminalPollRead::Snapshot {
                output_path,
                result,
            } => {
                if let Ok(snapshot) = read_shell_terminal_snapshot(result) {
                    if let Some(settlement) =
                        state.observe_snapshot(now_ms(), &output_path, &snapshot)
                    {
                        return Some(settlement);
                    }
                }
            }
            ShellTerminalPollRead::PermissionDenied { output_path } => {
                if output_path.is_some() {
                    state.output_path = output_path;
                }
                return Some(state.permission_denied());
            }
            ShellTerminalPollRead::TransientFailure(_) => {}
        }

        if let Some(settlement) = state.timeout(now_ms()) {
            return Some(settlement);
        }
        if is_cancelled() {
            return None;
        }
        delay(poll_ms.max(1));
    }
}

pub const GROUP_CHAT_TAG_PREFIX: &str = "[Group chat: ";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct SandTerminalReadError(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalReadResult {
    SuccessText(String),
    SuccessData(Vec<u8>),
    FileNotFound,
    Failure(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFileSnapshot {
    pub exists: bool,
    pub content: String,
}

pub fn read_shell_terminal_snapshot(
    result: TerminalReadResult,
) -> Result<TerminalFileSnapshot, SandTerminalReadError> {
    match result {
        TerminalReadResult::SuccessText(content) => Ok(TerminalFileSnapshot {
            exists: true,
            content,
        }),
        TerminalReadResult::SuccessData(data) => Ok(TerminalFileSnapshot {
            exists: true,
            content: String::from_utf8_lossy(&data).into_owned(),
        }),
        TerminalReadResult::FileNotFound => Ok(TerminalFileSnapshot {
            exists: false,
            content: String::new(),
        }),
        TerminalReadResult::Failure(case) => Err(SandTerminalReadError(format!(
            "terminal file read failed ({})",
            if case.is_empty() { "unknown" } else { &case }
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellWatchStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellWatchSettlement {
    pub status: ShellWatchStatus,
    pub detail: Option<String>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellTerminalPollState {
    started_at_ms: u64,
    missing_reads: usize,
    output_path: Option<String>,
}

impl ShellTerminalPollState {
    pub fn new(started_at_ms: u64) -> Self {
        Self {
            started_at_ms,
            missing_reads: 0,
            output_path: None,
        }
    }

    pub fn missing_reads(&self) -> usize {
        self.missing_reads
    }

    pub fn observe_snapshot(
        &mut self,
        now_ms: u64,
        output_path: impl Into<String>,
        snapshot: &TerminalFileSnapshot,
    ) -> Option<ShellWatchSettlement> {
        let output_path = output_path.into();
        self.output_path = Some(output_path.clone());
        if snapshot.exists {
            self.missing_reads = 0;
            let footer = parse_shell_terminal_footer(&snapshot.content);
            if footer.is_complete {
                let status = if footer.exit_code == Some(0) {
                    ShellWatchStatus::Success
                } else {
                    ShellWatchStatus::Error
                };
                let detail = if status == ShellWatchStatus::Success {
                    None
                } else if footer.is_stream_failure {
                    Some(
                        "the command's output stream failed; read its output file for the error"
                            .to_string(),
                    )
                } else {
                    Some(format!(
                        "exit_code={}",
                        footer
                            .exit_code
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "unknown".to_string()),
                    ))
                };
                return Some(ShellWatchSettlement {
                    status,
                    detail,
                    output_path: Some(output_path),
                });
            }
        } else {
            self.missing_reads = self.missing_reads.saturating_add(1);
            if self.missing_reads >= SHELL_REWATCH_MISSING_FILE_GIVE_UP {
                return Some(ShellWatchSettlement {
                    status: ShellWatchStatus::Error,
                    detail: Some(
                        "The command's terminal output file no longer exists, so its completion can no longer be observed (it may have been cleaned up)."
                            .to_string(),
                    ),
                    output_path: Some(output_path),
                });
            }
        }
        self.timeout(now_ms)
    }

    pub fn permission_denied(&self) -> ShellWatchSettlement {
        ShellWatchSettlement {
            status: ShellWatchStatus::Error,
            detail: Some(
                "Fabushi is no longer allowed to read this command's output on the user's computer, so its completion cannot be observed. The command keeps running; ask the user to approve reading its output file for the result."
                    .to_string(),
            ),
            output_path: self.output_path.clone(),
        }
    }

    pub fn timeout(&self, now_ms: u64) -> Option<ShellWatchSettlement> {
        if now_ms.saturating_sub(self.started_at_ms) < SHELL_REWATCH_MAX_WAIT_MS {
            return None;
        }
        Some(ShellWatchSettlement {
            status: ShellWatchStatus::Error,
            detail: Some(format!(
                "The command is still running after {} minutes. It keeps running; check its output file for the result.",
                SHELL_REWATCH_MAX_WAIT_MS / 60_000
            )),
            output_path: self.output_path.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentTerminalUserMessage {
    pub id: String,
    pub text: String,
    pub rich_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedUserMessage {
    pub text: String,
    pub message_id: String,
    pub rich_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedUserTurnWatermark {
    pub turn_count: usize,
    pub boundary_ref: Vec<u8>,
    pub last_user_message_id: Option<String>,
    pub has_user_turn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializedTurnKind {
    NonAgent,
    Agent {
        user_message: Option<MaterializedUserMessage>,
    },
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedTurn {
    pub turn_ref: Vec<u8>,
    pub kind: MaterializedTurnKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatermarkResult {
    pub last_user_message_id: Option<String>,
    pub has_user_turn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatermarkResolution {
    pub result: WatermarkResult,
    pub cache: Option<ConfirmedUserTurnWatermark>,
}

pub fn is_group_turn_prompt_text(text: &str) -> bool {
    text.strip_prefix(SAND_HIDDEN_PROMPT_MARKER)
        .unwrap_or(text)
        .starts_with(GROUP_CHAT_TAG_PREFIX)
}

pub fn turn_refs_equal(a: &[u8], b: &[u8]) -> bool {
    a == b
}

pub fn find_confirmed_user_turn_watermark(
    turns: &[MaterializedTurn],
    cached: Option<&ConfirmedUserTurnWatermark>,
) -> WatermarkResolution {
    let cache_valid = cached.is_some_and(|cached| {
        cached.turn_count == 0
            || (cached.turn_count <= turns.len()
                && turns
                    .get(cached.turn_count - 1)
                    .is_some_and(|turn| turn_refs_equal(&turn.turn_ref, &cached.boundary_ref)))
    });
    let stop_index = cached
        .filter(|_| cache_valid)
        .map_or(0, |cached| cached.turn_count);

    let cache_result = |result: WatermarkResult| {
        let turn_count = turns.len();
        let boundary_ref = turns
            .last()
            .map(|turn| turn.turn_ref.clone())
            .unwrap_or_default();
        WatermarkResolution {
            cache: Some(ConfirmedUserTurnWatermark {
                turn_count,
                boundary_ref,
                last_user_message_id: result.last_user_message_id.clone(),
                has_user_turn: result.has_user_turn,
            }),
            result,
        }
    };

    for index in (stop_index..turns.len()).rev() {
        let turn = &turns[index];
        if turn.turn_ref.is_empty() {
            continue;
        }
        match &turn.kind {
            MaterializedTurnKind::Unreadable => {
                return WatermarkResolution {
                    result: WatermarkResult {
                        last_user_message_id: None,
                        has_user_turn: true,
                    },
                    cache: None,
                };
            }
            MaterializedTurnKind::NonAgent => continue,
            MaterializedTurnKind::Agent { user_message: None } => {
                return WatermarkResolution {
                    result: WatermarkResult {
                        last_user_message_id: None,
                        has_user_turn: true,
                    },
                    cache: None,
                };
            }
            MaterializedTurnKind::Agent {
                user_message: Some(user_message),
            } => {
                if user_message.text.starts_with(SAND_HIDDEN_PROMPT_MARKER) {
                    continue;
                }
                if user_message.message_id.is_empty()
                    && is_group_turn_prompt_text(&user_message.text)
                {
                    continue;
                }
                return cache_result(WatermarkResult {
                    last_user_message_id: Some(user_message.message_id.clone()),
                    has_user_turn: true,
                });
            }
        }
    }

    if let Some(cached) = cached.filter(|_| cache_valid && stop_index > 0) {
        return cache_result(WatermarkResult {
            last_user_message_id: cached.last_user_message_id.clone(),
            has_user_turn: cached.has_user_turn,
        });
    }

    cache_result(WatermarkResult {
        last_user_message_id: None,
        has_user_turn: false,
    })
}

pub fn collect_prepend_user_messages(
    recent_user_messages: &[RecentTerminalUserMessage],
    current_message_id: Option<&str>,
    watermark: &WatermarkResult,
) -> Vec<MaterializedUserMessage> {
    if recent_user_messages.is_empty() {
        return Vec::new();
    }
    let selectable = recent_user_messages
        .iter()
        .map(|message| RecentUserMessage {
            id: message.id.clone(),
            text: message.text.clone(),
        })
        .collect::<Vec<_>>();
    let selected = select_unconfirmed_user_messages(
        &selectable,
        current_message_id,
        watermark.last_user_message_id.as_deref(),
        watermark.has_user_turn,
    );
    selected
        .into_iter()
        .map(|message| {
            let source = recent_user_messages
                .iter()
                .find(|candidate| candidate.id == message.id);
            let address_note = build_user_message_address_note(Some(&message.id));
            MaterializedUserMessage {
                text: if address_note.is_empty() {
                    message.text
                } else {
                    format!("{address_note}\n{}", message.text)
                },
                message_id: message.id,
                rich_text: source.and_then(|source| source.rich_text.clone()),
            }
        })
        .collect()
}
