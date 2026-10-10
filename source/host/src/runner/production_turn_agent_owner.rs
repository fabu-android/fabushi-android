use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use super::{
    stream_attempt::{ProviderFailure, StreamAttemptHost, StreamAttemptInput, TurnStreamProvider},
    turn_run_shell::{shared_turn_run_shell, SharedTurnRunShell, TurnCancellation, TurnRunShellError},
    turn_settle::{prepare_checkpoint, persist_checkpoint, settle_completed_turn, TurnSettlement},
};

pub const SAND_AGENT_MAX_STEPS: usize = 5_000;
pub const SAND_AGENT_TOKEN_LIMIT: usize = 200_000;
pub const DISK_PRESSURE_REMINDER_MESSAGE: &str = "<system_reminder>\nThe box is near disk capacity. Avoid disk-heavy work and do not fill the remaining capacity.\n</system_reminder>";

pub type ProductionTurnSummarizationPrompt = Arc<
    dyn Fn(&str, &str, &dyn Fn() -> bool) -> Result<String, ProviderFailure> + Send + Sync,
>;
pub type ProductionTurnPrivacyModeResolver =
    Arc<dyn Fn() -> Option<ProductionTurnPrivacyMode> + Send + Sync>;
pub type ProductionTurnDiskPressureClaim =
    Arc<dyn Fn() -> Result<Option<String>, String> + Send + Sync>;
pub type ProductionTurnDiskPressureCommit =
    Arc<dyn Fn() -> Result<bool, String> + Send + Sync>;
pub type ProductionTurnDiskPressureRelease =
    Arc<dyn Fn() -> Result<bool, String> + Send + Sync>;
pub type ProductionTurnProfileAnnouncementCommit = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ProductionTurnPrivacyMode {
    Unspecified = 0,
    NoStorage = 1,
    NoTraining = 2,
    UsageDataTrainingAllowed = 3,
    UsageCodebaseTrainingAllowed = 4,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionTurnAgentStaticConfig {
    pub model_id: String,
    pub agent_token_limit: usize,
    pub conversation_id: String,
    pub is_box_scoped_subagent: bool,
    pub is_subagent_runner: bool,
    pub is_shared_room_runner: bool,
    pub sand_send_message_delivery_owed: bool,
    pub transcripts_folder_available: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProductionTurnAgentStaticProjection {
    pub max_steps: usize,
    pub agent_token_limit: usize,
    pub background_summarization_start_unused_tokens: usize,
    pub background_summarization_start_unused_percent: f64,
    pub background_summarization_persist_unused_tokens: usize,
    pub background_summarization_persist_unused_percent: f64,
    pub background_summarization_discard_on_error: bool,
    pub background_summarization_require_trigger_for_mid_loop_persist: bool,
    pub enable_watch_video_in_ide_subagent: bool,
    pub sand_send_message_delivery_owed: bool,
    pub user_message_timestamps: bool,
    pub rerender_user_info_on_request_context_recovery: bool,
    pub rerender_user_info_on_summarization: bool,
    pub skip_pre_turn_state_snapshot: bool,
    pub agent_type: &'static str,
    pub conversation_group_id: String,
    pub disable_user_info: bool,
    pub display_cursor_rules: bool,
    pub display_skills: bool,
    pub exclude_agent_transcripts: bool,
    pub enable_terminal_files: bool,
    pub enable_transcript_in_summary: bool,
}

impl ProductionTurnAgentStaticConfig {
    pub fn frozen_projection(&self) -> ProductionTurnAgentStaticProjection {
        ProductionTurnAgentStaticProjection {
            max_steps: SAND_AGENT_MAX_STEPS,
            agent_token_limit: self.agent_token_limit,
            background_summarization_start_unused_tokens: 10_000,
            background_summarization_start_unused_percent: 0.1,
            background_summarization_persist_unused_tokens: 5_000,
            background_summarization_persist_unused_percent: 0.05,
            background_summarization_discard_on_error: true,
            background_summarization_require_trigger_for_mid_loop_persist: true,
            enable_watch_video_in_ide_subagent: true,
            sand_send_message_delivery_owed: self.sand_send_message_delivery_owed,
            user_message_timestamps: true,
            rerender_user_info_on_request_context_recovery: true,
            rerender_user_info_on_summarization: true,
            skip_pre_turn_state_snapshot: true,
            agent_type: "IDE",
            conversation_group_id: self.conversation_id.clone(),
            disable_user_info: self.is_box_scoped_subagent,
            display_cursor_rules: true,
            display_skills: !self.is_subagent_runner && !self.is_shared_room_runner,
            exclude_agent_transcripts: self.is_subagent_runner
                || !self.transcripts_folder_available,
            enable_terminal_files: false,
            enable_transcript_in_summary: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProductionTurnAgentBuildInput {
    pub static_config: ProductionTurnAgentStaticConfig,
    pub static_projection: ProductionTurnAgentStaticProjection,
    pub privacy_mode: Option<ProductionTurnPrivacyMode>,
}

pub struct ProductionTurnAgentBuildBindings {
    static_config: ProductionTurnAgentStaticConfig,
    privacy_mode_resolver: ProductionTurnPrivacyModeResolver,
    summarization_prompt: ProductionTurnSummarizationPrompt,
}

impl ProductionTurnAgentBuildBindings {
    pub fn new(
        static_config: ProductionTurnAgentStaticConfig,
        privacy_mode_resolver: ProductionTurnPrivacyModeResolver,
        summarization_prompt: ProductionTurnSummarizationPrompt,
    ) -> Self {
        Self {
            static_config,
            privacy_mode_resolver,
            summarization_prompt,
        }
    }
}

pub struct ProductionTurnAgentLifecycleBindings {
    pub account_fence: String,
    pub conversation_id: String,
    pub claim_id: String,
    disk_pressure_claim: Option<ProductionTurnDiskPressureClaim>,
    disk_pressure_commit: Option<ProductionTurnDiskPressureCommit>,
    disk_pressure_release: Option<ProductionTurnDiskPressureRelease>,
    profile_announcement_prompt: Option<String>,
    profile_announcement_commit: Option<ProductionTurnProfileAnnouncementCommit>,
}

impl ProductionTurnAgentLifecycleBindings {
    pub fn new(
        account_fence: impl Into<String>,
        conversation_id: impl Into<String>,
        claim_id: impl Into<String>,
    ) -> Self {
        Self {
            account_fence: account_fence.into(),
            conversation_id: conversation_id.into(),
            claim_id: claim_id.into(),
            disk_pressure_claim: None,
            disk_pressure_commit: None,
            disk_pressure_release: None,
            profile_announcement_prompt: None,
            profile_announcement_commit: None,
        }
    }

    pub fn with_disk_pressure_callbacks(
        mut self,
        claim: ProductionTurnDiskPressureClaim,
        commit: ProductionTurnDiskPressureCommit,
        release: ProductionTurnDiskPressureRelease,
    ) -> Self {
        self.disk_pressure_claim = Some(claim);
        self.disk_pressure_commit = Some(commit);
        self.disk_pressure_release = Some(release);
        self
    }

    pub fn with_profile_announcement(
        mut self,
        prompt: Option<String>,
        commit: Option<ProductionTurnProfileAnnouncementCommit>,
    ) -> Self {
        self.profile_announcement_prompt = prompt;
        self.profile_announcement_commit = commit;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionTurnInput {
    pub operation_id: String,
    pub request_id: String,
    pub agent_id: String,
    pub model: String,
    pub prompt: String,
    pub resume_checkpoint_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionTurnEvent {
    Retrying {
        attempt: usize,
        delay_ms: u64,
        reason: String,
    },
    Delta(String),
    Completed {
        finish_reason: String,
        attempts: usize,
    },
    Failed {
        message: String,
    },
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionTurnResult {
    pub events: Vec<ProductionTurnEvent>,
    pub settlement: Option<TurnSettlement>,
}

pub struct ProductionTurnAgentOwner<P: TurnStreamProvider> {
    stream: StreamAttemptHost<P>,
    shell: SharedTurnRunShell,
    upgrade_quiescing: Arc<AtomicBool>,
    build_input: Option<ProductionTurnAgentBuildInput>,
    summarization_prompt: Option<ProductionTurnSummarizationPrompt>,
    lifecycle_bindings: Option<ProductionTurnAgentLifecycleBindings>,
    disk_pressure_episode_id: Option<String>,
    disk_pressure_committed: bool,
    profile_announcement_committed: bool,
    disposed: bool,
}

impl<P: TurnStreamProvider> ProductionTurnAgentOwner<P> {
    pub fn new(provider: P) -> Self {
        Self {
            stream: StreamAttemptHost::new(provider),
            shell: shared_turn_run_shell(),
            upgrade_quiescing: Arc::new(AtomicBool::new(false)),
            build_input: None,
            summarization_prompt: None,
            lifecycle_bindings: None,
            disk_pressure_episode_id: None,
            disk_pressure_committed: false,
            profile_announcement_committed: false,
            disposed: false,
        }
    }

    pub fn with_upgrade_quiesce_signal(mut self, signal: Arc<AtomicBool>) -> Self {
        self.upgrade_quiescing = signal;
        self
    }

    pub fn with_turn_run_shell(mut self, shell: SharedTurnRunShell) -> Self {
        self.shell = shell;
        self
    }

    pub fn with_build_bindings(
        mut self,
        bindings: ProductionTurnAgentBuildBindings,
    ) -> Self {
        let privacy_mode = (bindings.privacy_mode_resolver)();
        let static_projection = bindings.static_config.frozen_projection();
        self.build_input = Some(ProductionTurnAgentBuildInput {
            static_config: bindings.static_config,
            static_projection,
            privacy_mode,
        });
        self.summarization_prompt = Some(bindings.summarization_prompt);
        self
    }

    pub fn with_lifecycle_bindings(
        mut self,
        bindings: ProductionTurnAgentLifecycleBindings,
    ) -> Result<Self, ProviderFailure> {
        self.release_uncommitted_disk_pressure();
        self.disk_pressure_episode_id = match bindings.disk_pressure_claim.as_ref() {
            Some(claim) => claim().map_err(ProviderFailure::new)?,
            None => None,
        };
        self.lifecycle_bindings = Some(bindings);
        self.disk_pressure_committed = false;
        self.profile_announcement_committed = false;
        self.disposed = false;
        Ok(self)
    }

    pub fn build_input(&self) -> Option<&ProductionTurnAgentBuildInput> {
        self.build_input.as_ref()
    }

    pub fn disk_pressure_episode_id(&self) -> Option<&str> {
        self.disk_pressure_episode_id.as_deref()
    }

    pub fn run_summarization_prompt(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        should_cancel: &dyn Fn() -> bool,
    ) -> Result<String, ProviderFailure> {
        let summarization_prompt = self.summarization_prompt.as_ref().ok_or_else(|| {
            ProviderFailure::new("production turn summarization surface is not bound")
        })?;
        summarization_prompt(system_prompt, user_prompt, should_cancel)
    }

    pub fn run(
        &mut self,
        input: ProductionTurnInput,
    ) -> Result<ProductionTurnResult, ProviderFailure> {
        self.run_with_event_sink(input, &mut |_| Ok(()))
    }

    pub fn run_with_event_sink(
        &mut self,
        input: ProductionTurnInput,
        sink: &mut dyn FnMut(ProductionTurnEvent) -> Result<(), String>,
    ) -> Result<ProductionTurnResult, ProviderFailure> {
        self.validate_build_input(&input)?;
        if self.upgrade_quiescing.load(Ordering::Acquire) {
            return Err(ProviderFailure::new(format!(
                "turn run rejected: {:?}",
                TurnRunShellError::QuiescingForUpgrade
            )));
        }

        let projected_prompt = self.project_prompt_for_turn(&input.prompt);
        self.shell
            .lock()
            .map_err(|_| ProviderFailure::new("turn run shell lock poisoned"))?
            .begin(&input.operation_id, &input.request_id, &projected_prompt)
            .map_err(|error| ProviderFailure::new(format!("turn run rejected: {error:?}")))?;

        let frozen_model = self
            .build_input
            .as_ref()
            .expect("validated production turn build input")
            .static_config
            .model_id
            .clone();
        let stream_input = StreamAttemptInput {
            operation_id: input.operation_id.clone(),
            agent_id: input.agent_id,
            model: frozen_model,
            prompt: projected_prompt,
            resume_checkpoint_available: input.resume_checkpoint_available,
        };

        let mut emitted_retries = Vec::new();
        let mut emitted_chunks = Vec::new();
        let quiesce = Arc::clone(&self.upgrade_quiescing);
        let result = self.stream.run_with_observers(
            &stream_input,
            &mut |retry| {
                emitted_retries.push(retry.clone());
            },
            &mut |chunk| {
                if quiesce.load(Ordering::Acquire) {
                    return Err("upgrade quiesce requested during Agent turn".into());
                }
                emitted_chunks.push(chunk.to_string());
                sink(ProductionTurnEvent::Delta(chunk.to_string()))
            },
        );
        let _ = self
            .shell
            .lock()
            .map_err(|_| ProviderFailure::new("turn run shell lock poisoned"))?
            .finish(&input.operation_id);

        for retry in &emitted_retries {
            sink(ProductionTurnEvent::Retrying {
                attempt: retry.attempt,
                delay_ms: retry.delay_ms,
                reason: retry.reason.clone(),
            })
            .map_err(ProviderFailure::new)?;
        }

        match result {
            Ok(result) => {
                let completed = ProductionTurnEvent::Completed {
                    finish_reason: result.finish_reason.clone(),
                    attempts: result.attempts,
                };
                sink(completed.clone()).map_err(ProviderFailure::new)?;

                let checkpoint = prepare_checkpoint(
                    &input.operation_id,
                    result.attempts,
                    result.chunks.len(),
                )
                .map_err(ProviderFailure::new)?;
                let checkpoint = persist_checkpoint(checkpoint);
                let settlement =
                    settle_completed_turn(checkpoint, result.finish_reason)
                        .map_err(ProviderFailure::new)?;
                self.commit_successful_lifecycle();

                let mut events = emitted_retries
                    .iter()
                    .map(|retry| ProductionTurnEvent::Retrying {
                        attempt: retry.attempt,
                        delay_ms: retry.delay_ms,
                        reason: retry.reason.clone(),
                    })
                    .collect::<Vec<_>>();
                events.extend(
                    emitted_chunks
                        .iter()
                        .cloned()
                        .map(ProductionTurnEvent::Delta),
                );
                events.push(completed);

                Ok(ProductionTurnResult {
                    events,
                    settlement: Some(settlement),
                })
            }
            Err(error) => {
                let failed = ProductionTurnEvent::Failed {
                    message: error.message.clone(),
                };
                sink(failed.clone()).map_err(ProviderFailure::new)?;
                Ok(ProductionTurnResult {
                    events: vec![failed],
                    settlement: None,
                })
            }
        }
    }

    pub fn cancel(&mut self, operation_id: &str) -> Result<ProductionTurnEvent, String> {
        self.shell
            .lock()
            .map_err(|_| "turn run shell lock poisoned".to_string())?
            .cancel(
                operation_id,
                TurnCancellation {
                    intentional: true,
                    reason: "user".into(),
                },
            )
            .map_err(|error| format!("turn cancel rejected: {error:?}"))?;
        self.stream.cancel(operation_id)?;
        self.release_uncommitted_disk_pressure();
        Ok(ProductionTurnEvent::Cancelled)
    }

    pub fn is_active(&self, operation_id: &str) -> bool {
        self.shell
            .lock()
            .is_ok_and(|shell| shell.is_active(operation_id))
    }

    pub fn request_quiesce_for_upgrade(&mut self) {
        self.upgrade_quiescing.store(true, Ordering::Release);
        if let Ok(mut shell) = self.shell.lock() {
            shell.request_quiesce_for_upgrade();
        }
    }

    pub fn cancel_quiesce_for_upgrade(&mut self) {
        self.upgrade_quiescing.store(false, Ordering::Release);
        if let Ok(mut shell) = self.shell.lock() {
            shell.cancel_quiesce_for_upgrade();
        }
    }

    pub fn is_quiescing_for_upgrade(&self) -> bool {
        self.upgrade_quiescing.load(Ordering::Acquire)
            || self
                .shell
                .lock()
                .is_ok_and(|shell| shell.is_quiescing_for_upgrade())
    }

    pub fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.release_uncommitted_disk_pressure();
    }

    fn validate_build_input(&self, input: &ProductionTurnInput) -> Result<(), ProviderFailure> {
        let build = self
            .build_input
            .as_ref()
            .ok_or_else(|| ProviderFailure::new("production turn build input is not bound"))?;
        if build.static_config.model_id.trim().is_empty()
            || build.static_config.conversation_id.trim().is_empty()
        {
            return Err(ProviderFailure::new(
                "production turn identity/configuration is empty",
            ));
        }
        if build.static_config.model_id != input.model {
            return Err(ProviderFailure::new(
                "production turn model drifted after frozen build",
            ));
        }
        if build.static_projection.max_steps != SAND_AGENT_MAX_STEPS
            || build.static_projection.agent_token_limit != SAND_AGENT_TOKEN_LIMIT
            || build.static_config.agent_token_limit != SAND_AGENT_TOKEN_LIMIT
        {
            return Err(ProviderFailure::new(
                "production turn token/step limits drifted from frozen config",
            ));
        }
        if build.static_projection.conversation_group_id != build.static_config.conversation_id {
            return Err(ProviderFailure::new(
                "production turn conversation identity drifted after frozen build",
            ));
        }
        if let Some(lifecycle) = self.lifecycle_bindings.as_ref() {
            if lifecycle.conversation_id != build.static_config.conversation_id {
                return Err(ProviderFailure::new(
                    "production turn build/lifecycle conversation identities differ",
                ));
            }
        }
        Ok(())
    }

    fn project_prompt_for_turn(&self, prompt: &str) -> String {
        let mut projected = prompt.to_string();
        if !self.disk_pressure_committed
            && self.disk_pressure_episode_id.is_some()
            && !projected.contains(DISK_PRESSURE_REMINDER_MESSAGE)
        {
            projected.push_str("\n\n");
            projected.push_str(DISK_PRESSURE_REMINDER_MESSAGE);
        }
        if let Some(profile_prompt) = self
            .lifecycle_bindings
            .as_ref()
            .and_then(|bindings| bindings.profile_announcement_prompt.as_deref())
            .filter(|value| !value.trim().is_empty())
        {
            projected.push_str("\n\n");
            projected.push_str(profile_prompt);
        }
        projected
    }

    fn commit_successful_lifecycle(&mut self) {
        let Some(bindings) = self.lifecycle_bindings.as_ref() else {
            return;
        };
        if !self.disk_pressure_committed && self.disk_pressure_episode_id.is_some() {
            if let Some(commit) = bindings.disk_pressure_commit.as_ref() {
                self.disk_pressure_committed = commit().unwrap_or(false);
            }
        }
        if !self.profile_announcement_committed {
            if let Some(commit) = bindings.profile_announcement_commit.as_ref() {
                commit();
                self.profile_announcement_committed = true;
            }
        }
    }

    fn release_uncommitted_disk_pressure(&mut self) {
        if self.disk_pressure_committed || self.disk_pressure_episode_id.is_none() {
            return;
        }
        if let Some(release) = self
            .lifecycle_bindings
            .as_ref()
            .and_then(|bindings| bindings.disk_pressure_release.as_ref())
        {
            let _ = release();
        }
        self.disk_pressure_episode_id = None;
    }
}

impl<P: TurnStreamProvider> Drop for ProductionTurnAgentOwner<P> {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::stream_attempt::StreamGeneration;
    use std::sync::{
        atomic::{AtomicUsize, Ordering as AtomicOrdering},
        Mutex,
    };

    #[derive(Clone)]
    struct Provider {
        prompts: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }

    impl TurnStreamProvider for Provider {
        fn start_stream(
            &mut self,
            input: &StreamAttemptInput,
            _attempt: usize,
        ) -> Result<StreamGeneration, ProviderFailure> {
            self.prompts.lock().unwrap().push(input.prompt.clone());
            if self.fail {
                return Err(ProviderFailure::new("provider failed"));
            }
            Ok(StreamGeneration {
                first_token_delay_ms: 1,
                chunks: vec!["a".into(), "b".into()],
                finish_reason: "stop".into(),
            })
        }

        fn cancel(&mut self, _operation_id: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn input() -> ProductionTurnInput {
        ProductionTurnInput {
            operation_id: "op".into(),
            request_id: "req".into(),
            agent_id: "agent".into(),
            model: "default".into(),
            prompt: "hello".into(),
            resume_checkpoint_available: false,
        }
    }

    fn config() -> ProductionTurnAgentStaticConfig {
        ProductionTurnAgentStaticConfig {
            model_id: "default".into(),
            agent_token_limit: SAND_AGENT_TOKEN_LIMIT,
            conversation_id: "agent".into(),
            is_box_scoped_subagent: false,
            is_subagent_runner: false,
            is_shared_room_runner: false,
            sand_send_message_delivery_owed: false,
            transcripts_folder_available: true,
        }
    }

    fn bindings(
        privacy_calls: Arc<AtomicUsize>,
    ) -> ProductionTurnAgentBuildBindings {
        ProductionTurnAgentBuildBindings::new(
            config(),
            Arc::new(move || {
                privacy_calls.fetch_add(1, AtomicOrdering::SeqCst);
                Some(ProductionTurnPrivacyMode::NoStorage)
            }),
            Arc::new(|system, user, should_cancel| {
                if should_cancel() {
                    return Err(ProviderFailure::new("cancelled"));
                }
                Ok(format!("{system}|{user}"))
            }),
        )
    }

    #[test]
    fn one_owner_freezes_static_config_and_privacy_once() {
        let privacy_calls = Arc::new(AtomicUsize::new(0));
        let prompts = Arc::new(Mutex::new(Vec::new()));
        let mut owner = ProductionTurnAgentOwner::new(Provider {
            prompts,
            fail: false,
        })
        .with_build_bindings(bindings(Arc::clone(&privacy_calls)));
        assert_eq!(privacy_calls.load(AtomicOrdering::SeqCst), 1);
        let build = owner.build_input().unwrap();
        assert_eq!(build.static_projection.max_steps, SAND_AGENT_MAX_STEPS);
        assert_eq!(build.static_projection.agent_token_limit, SAND_AGENT_TOKEN_LIMIT);
        assert_eq!(build.privacy_mode, Some(ProductionTurnPrivacyMode::NoStorage));
        assert_eq!(
            owner
                .run_summarization_prompt("system", "user", &|| false)
                .unwrap(),
            "system|user"
        );
        let result = owner.run(input()).unwrap();
        assert!(result.settlement.unwrap().checkpoint.durable);
        assert_eq!(privacy_calls.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn success_projects_disk_pressure_and_profile_then_commits_once() {
        let prompts = Arc::new(Mutex::new(Vec::new()));
        let commits = Arc::new(AtomicUsize::new(0));
        let releases = Arc::new(AtomicUsize::new(0));
        let profiles = Arc::new(AtomicUsize::new(0));

        let commit_count = Arc::clone(&commits);
        let release_count = Arc::clone(&releases);
        let profile_count = Arc::clone(&profiles);
        let lifecycle = ProductionTurnAgentLifecycleBindings::new("acct", "agent", "op")
            .with_disk_pressure_callbacks(
                Arc::new(|| Ok(Some("episode-1".into()))),
                Arc::new(move || {
                    commit_count.fetch_add(1, AtomicOrdering::SeqCst);
                    Ok(true)
                }),
                Arc::new(move || {
                    release_count.fetch_add(1, AtomicOrdering::SeqCst);
                    Ok(true)
                }),
            )
            .with_profile_announcement(
                Some("<agent_profile>Renamed</agent_profile>".into()),
                Some(Arc::new(move || {
                    profile_count.fetch_add(1, AtomicOrdering::SeqCst);
                })),
            );

        let mut owner = ProductionTurnAgentOwner::new(Provider {
            prompts: Arc::clone(&prompts),
            fail: false,
        })
        .with_build_bindings(bindings(Arc::new(AtomicUsize::new(0))))
        .with_lifecycle_bindings(lifecycle)
        .unwrap();
        owner.run(input()).unwrap();
        owner.dispose();
        assert_eq!(commits.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(releases.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(profiles.load(AtomicOrdering::SeqCst), 1);
        let prompt = prompts.lock().unwrap().join("");
        assert!(prompt.contains(DISK_PRESSURE_REMINDER_MESSAGE));
        assert!(prompt.contains("<agent_profile>Renamed</agent_profile>"));
    }

    #[test]
    fn failure_and_dispose_release_uncommitted_claim_without_profile_commit() {
        let releases = Arc::new(AtomicUsize::new(0));
        let profiles = Arc::new(AtomicUsize::new(0));
        let release_count = Arc::clone(&releases);
        let profile_count = Arc::clone(&profiles);
        let lifecycle = ProductionTurnAgentLifecycleBindings::new("acct", "agent", "op")
            .with_disk_pressure_callbacks(
                Arc::new(|| Ok(Some("episode-1".into()))),
                Arc::new(|| Ok(true)),
                Arc::new(move || {
                    release_count.fetch_add(1, AtomicOrdering::SeqCst);
                    Ok(true)
                }),
            )
            .with_profile_announcement(
                Some("profile".into()),
                Some(Arc::new(move || {
                    profile_count.fetch_add(1, AtomicOrdering::SeqCst);
                })),
            );
        let mut owner = ProductionTurnAgentOwner::new(Provider {
            prompts: Arc::new(Mutex::new(Vec::new())),
            fail: true,
        })
        .with_build_bindings(bindings(Arc::new(AtomicUsize::new(0))))
        .with_lifecycle_bindings(lifecycle)
        .unwrap();

        let result = owner.run(input()).unwrap();
        assert!(matches!(
            result.events.as_slice(),
            [ProductionTurnEvent::Failed { .. }]
        ));
        owner.dispose();
        owner.dispose();
        assert_eq!(releases.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(profiles.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn shared_upgrade_quiesce_blocks_new_turn_and_can_be_cancelled() {
        let signal = Arc::new(AtomicBool::new(true));
        let mut owner = ProductionTurnAgentOwner::new(Provider {
            prompts: Arc::new(Mutex::new(Vec::new())),
            fail: false,
        })
        .with_upgrade_quiesce_signal(Arc::clone(&signal))
        .with_build_bindings(bindings(Arc::new(AtomicUsize::new(0))));
        assert!(owner.run(input()).unwrap_err().message.contains("QuiescingForUpgrade"));
        owner.cancel_quiesce_for_upgrade();
        assert!(!signal.load(Ordering::Acquire));
        assert!(owner.run(input()).is_ok());
    }

    #[test]
    fn frozen_model_drift_fails_before_provider_dispatch() {
        let prompts = Arc::new(Mutex::new(Vec::new()));
        let mut owner = ProductionTurnAgentOwner::new(Provider {
            prompts: Arc::clone(&prompts),
            fail: false,
        })
        .with_build_bindings(bindings(Arc::new(AtomicUsize::new(0))));
        let mut drifted = input();
        drifted.model = "changed-after-freeze".into();
        assert!(owner.run(drifted).unwrap_err().message.contains("model drifted"));
        assert!(prompts.lock().unwrap().is_empty());
    }
}
