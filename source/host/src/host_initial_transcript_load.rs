use std::thread;
use std::time::Duration;

pub const INITIAL_TRANSCRIPT_BUSY_ATTEMPTS: usize = 5;
pub const INITIAL_TRANSCRIPT_BUSY_BASE_DELAY_MS: u64 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitialTranscriptLoadOptions {
    pub limit: usize,
    pub allow_empty_on_corruption: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitialTranscriptDegradedReason {
    SqliteBusy(String),
    AgentLimit,
    Corruption(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitialTranscriptLoadOutcome {
    pub rows: Vec<String>,
    pub degraded: Option<InitialTranscriptDegradedReason>,
}

impl InitialTranscriptLoadOutcome {
    fn loaded(rows: Vec<String>) -> Self {
        Self { rows, degraded: None }
    }

    fn degraded(reason: InitialTranscriptDegradedReason) -> Self {
        Self { rows: Vec::new(), degraded: Some(reason) }
    }
}

pub trait InitialTranscriptLoader {
    fn load(&mut self, agent_id: &str, limit: usize) -> Result<Vec<String>, String>;
}

pub fn is_sqlite_busy_message(error: &str) -> bool {
    let message = error.to_ascii_lowercase();
    message.contains("database is locked")
        || message.contains("database table is locked")
        || message.contains("database schema is locked")
        || message.contains("sqlite_busy")
        || message.contains("sqlite busy")
}

pub fn is_agent_limit_message(error: &str) -> bool {
    let message = error.trim();
    message.starts_with("Agent limit of ") && message.ends_with(" reached")
}

pub fn is_storage_corruption_message(error: &str) -> bool {
    let message = error.to_ascii_lowercase();
    message.contains("database disk image is malformed")
        || message.contains("file is not a database")
        || message.contains("database malformed")
        || message.contains("sqlite_corrupt")
        || message.contains("sqlite_notadb")
        || message.contains("corrupt transcript")
}

pub fn load_initial_transcript_resiliently<L: InitialTranscriptLoader>(
    loader: &mut L,
    agent_id: &str,
    options: &InitialTranscriptLoadOptions,
) -> Result<Vec<String>, String> {
    load_initial_transcript_outcome(loader, agent_id, options).map(|outcome| outcome.rows)
}

pub fn load_initial_transcript_outcome<L: InitialTranscriptLoader>(
    loader: &mut L,
    agent_id: &str,
    options: &InitialTranscriptLoadOptions,
) -> Result<InitialTranscriptLoadOutcome, String> {
    load_initial_transcript_outcome_with_delay(loader, agent_id, options, |delay_ms| {
        thread::sleep(Duration::from_millis(delay_ms));
    })
}

pub fn load_initial_transcript_outcome_with_delay<L, D>(
    loader: &mut L,
    agent_id: &str,
    options: &InitialTranscriptLoadOptions,
    mut delay: D,
) -> Result<InitialTranscriptLoadOutcome, String>
where
    L: InitialTranscriptLoader,
    D: FnMut(u64),
{
    if agent_id.trim().is_empty() {
        return Err("agent_id is required".into());
    }
    let limit = options.limit.max(1);
    let mut last_busy = None;
    for attempt in 0..INITIAL_TRANSCRIPT_BUSY_ATTEMPTS {
        match loader.load(agent_id, limit) {
            Ok(rows) => return Ok(InitialTranscriptLoadOutcome::loaded(rows)),
            Err(error) if is_sqlite_busy_message(&error) => {
                last_busy = Some(error);
                if attempt + 1 < INITIAL_TRANSCRIPT_BUSY_ATTEMPTS {
                    let factor = 1_u64
                        .checked_shl(attempt.min(63) as u32)
                        .unwrap_or(u64::MAX);
                    delay(INITIAL_TRANSCRIPT_BUSY_BASE_DELAY_MS.saturating_mul(factor));
                }
            }
            Err(error) if is_agent_limit_message(&error) => {
                return Ok(InitialTranscriptLoadOutcome::degraded(
                    InitialTranscriptDegradedReason::AgentLimit,
                ));
            }
            Err(error)
                if options.allow_empty_on_corruption && is_storage_corruption_message(&error) =>
            {
                return Ok(InitialTranscriptLoadOutcome::degraded(
                    InitialTranscriptDegradedReason::Corruption(error),
                ));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(InitialTranscriptLoadOutcome::degraded(
        InitialTranscriptDegradedReason::SqliteBusy(
            last_busy.unwrap_or_else(|| "SQLite busy".to_string()),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Loader {
        results: VecDeque<Result<Vec<String>, String>>,
        calls: usize,
    }

    impl Loader {
        fn new(results: impl IntoIterator<Item = Result<Vec<String>, String>>) -> Self {
            Self { results: results.into_iter().collect(), calls: 0 }
        }
    }

    impl InitialTranscriptLoader for Loader {
        fn load(&mut self, _agent_id: &str, _limit: usize) -> Result<Vec<String>, String> {
            self.calls += 1;
            self.results.pop_front().unwrap_or_else(|| Err("unexpected extra load".into()))
        }
    }

    fn options() -> InitialTranscriptLoadOptions {
        InitialTranscriptLoadOptions { limit: 100, allow_empty_on_corruption: false }
    }

    #[test]
    fn busy_retries_with_desktop_backoff() {
        let mut loader = Loader::new([
            Err("database is locked".into()),
            Err("SQLITE_BUSY".into()),
            Ok(vec!["restored".into()]),
        ]);
        let mut delays = Vec::new();
        let outcome = load_initial_transcript_outcome_with_delay(
            &mut loader,
            "agent",
            &options(),
            |delay| delays.push(delay),
        ).unwrap();
        assert_eq!(outcome.rows, vec!["restored"]);
        assert_eq!(outcome.degraded, None);
        assert_eq!(delays, vec![100, 200]);
        assert_eq!(loader.calls, 3);
    }

    #[test]
    fn exhausted_busy_and_agent_limit_are_explicit_degraded_states() {
        let mut busy = Loader::new((0..INITIAL_TRANSCRIPT_BUSY_ATTEMPTS)
            .map(|_| Err::<Vec<String>, String>("database table is locked".into())));
        let outcome = load_initial_transcript_outcome_with_delay(
            &mut busy, "agent", &options(), |_| {},
        ).unwrap();
        assert!(matches!(
            outcome.degraded,
            Some(InitialTranscriptDegradedReason::SqliteBusy(_))
        ));

        let mut limited = Loader::new([Err("Agent limit of 10 reached".into())]);
        let outcome = load_initial_transcript_outcome_with_delay(
            &mut limited, "agent", &options(), |_| {},
        ).unwrap();
        assert_eq!(outcome.degraded, Some(InitialTranscriptDegradedReason::AgentLimit));
    }

    #[test]
    fn corruption_flag_does_not_swallow_unrelated_failures() {
        let mut permissive = options();
        permissive.allow_empty_on_corruption = true;

        let mut corrupted = Loader::new([Err("database disk image is malformed".into())]);
        let outcome = load_initial_transcript_outcome_with_delay(
            &mut corrupted, "agent", &permissive, |_| {},
        ).unwrap();
        assert!(matches!(
            outcome.degraded,
            Some(InitialTranscriptDegradedReason::Corruption(_))
        ));

        let mut denied = Loader::new([Err("permission denied".into())]);
        assert_eq!(
            load_initial_transcript_outcome_with_delay(
                &mut denied, "agent", &permissive, |_| {},
            ).unwrap_err(),
            "permission denied"
        );
    }

    #[test]
    fn empty_agent_id_fails_before_storage_access() {
        let mut loader = Loader::new([Ok(vec!["unexpected".into()])]);
        assert!(load_initial_transcript_outcome_with_delay(
            &mut loader, " ", &options(), |_| {},
        ).is_err());
        assert_eq!(loader.calls, 0);
    }
}
