use fabushi_android_shared::node::mcp::mcp_auth_watch_lifecycle::{
    McpAuthPollOutcome, McpAuthPollRequest, McpAuthPollSettlement, McpAuthPollTick,
    McpAuthWatchCompletion, McpAuthWatchLifecycle, PendingMcpAuthWatch,
};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

const STORE_VERSION: u64 = 1;
const MAX_STORE_BYTES: u64 = 512 * 1024;

pub struct AndroidMcpAuthWatchManager {
    store_path: PathBuf,
    lifecycle: McpAuthWatchLifecycle,
}

impl AndroidMcpAuthWatchManager {
    pub fn open(store_path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let store_path = store_path.into();
        let lifecycle = match fs::metadata(&store_path) {
            Ok(metadata) if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_STORE_BYTES => {
                let _ = fs::remove_file(&store_path);
                McpAuthWatchLifecycle::new()
            }
            Ok(_) => match fs::read_to_string(&store_path)
                .map_err(|error| format!("failed to read MCP auth watch store: {error}"))
                .and_then(|raw| parse_store(&raw, now_ms))
            {
                Ok(lifecycle) => lifecycle,
                Err(_) => {
                    let _ = fs::remove_file(&store_path);
                    McpAuthWatchLifecycle::new()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => McpAuthWatchLifecycle::new(),
            Err(error) => return Err(format!("failed to inspect MCP auth watch store: {error}")),
        };
        let mut manager = Self {
            store_path,
            lifecycle,
        };
        if manager.lifecycle.prune_expired(now_ms) > 0 {
            manager.persist()?;
        }
        Ok(manager)
    }

    pub fn begin_watch(
        &mut self,
        now_ms: u64,
        server_id: &str,
        server_name: &str,
        server_url: &str,
        account_key: &str,
        requesting_agent_id: Option<&str>,
        force_reauth: bool,
    ) -> Result<(PendingMcpAuthWatch, Option<PendingMcpAuthWatch>), String> {
        let result = self
            .lifecycle
            .begin_watch(
                now_ms,
                server_id,
                server_name,
                server_url,
                account_key,
                requesting_agent_id,
                force_reauth,
            )
            .map_err(str::to_string)?;
        self.persist()?;
        Ok(result)
    }

    pub fn poll_tick(
        &mut self,
        now_ms: u64,
        server_id: &str,
        account_key: &str,
    ) -> Result<McpAuthPollTick, String> {
        let before = self.snapshot();
        let tick = self.lifecycle.poll_tick(now_ms, server_id, account_key);
        if self.snapshot() != before {
            self.persist()?;
        }
        Ok(tick)
    }

    pub fn settle_poll(
        &mut self,
        now_ms: u64,
        request: &McpAuthPollRequest,
        outcome: McpAuthPollOutcome,
    ) -> Result<McpAuthPollSettlement, String> {
        let settlement = self.lifecycle.settle_poll(now_ms, request, outcome);
        if !matches!(settlement, McpAuthPollSettlement::Stale) {
            self.persist()?;
        }
        Ok(settlement)
    }

    pub fn poll_failed(
        &mut self,
        now_ms: u64,
        request: &McpAuthPollRequest,
    ) -> Result<McpAuthPollSettlement, String> {
        self.settle_poll(now_ms, request, McpAuthPollOutcome::Unreachable)
    }

    pub fn note_auth_completed_elsewhere(
        &mut self,
        server_id: &str,
        account_key: &str,
    ) -> Result<Option<PendingMcpAuthWatch>, String> {
        let watch = self
            .lifecycle
            .note_auth_completed_elsewhere(server_id, account_key);
        if watch.is_some() {
            self.persist()?;
        }
        Ok(watch)
    }

    pub fn cancel_watch(
        &mut self,
        server_id: &str,
        account_key: &str,
    ) -> Result<Option<McpAuthWatchCompletion>, String> {
        let completion = self.lifecycle.cancel_watch(server_id, account_key);
        if completion.is_some() {
            self.persist()?;
        }
        Ok(completion)
    }

    pub fn cancel_server(
        &mut self,
        server_id: &str,
    ) -> Result<Vec<McpAuthWatchCompletion>, String> {
        let completions = self.lifecycle.cancel_server(server_id);
        if !completions.is_empty() {
            self.persist()?;
        }
        Ok(completions)
    }

    pub fn cancel_all(&mut self) -> Result<Vec<McpAuthWatchCompletion>, String> {
        let completions = self.lifecycle.cancel_all();
        if !completions.is_empty() {
            self.persist()?;
        }
        Ok(completions)
    }

    pub fn prune_expired(&mut self, now_ms: u64) -> Result<usize, String> {
        let removed = self.lifecycle.prune_expired(now_ms);
        if removed > 0 {
            self.persist()?;
        }
        Ok(removed)
    }

    pub fn watch(&self, server_id: &str, account_key: &str) -> Option<&PendingMcpAuthWatch> {
        self.lifecycle.watch(server_id, account_key)
    }

    pub fn snapshot(&self) -> Value {
        json!({
            "version": STORE_VERSION,
            "nextGeneration": self.lifecycle.next_generation(),
            "watches": self.lifecycle.watches().map(watch_to_value).collect::<Vec<_>>(),
        })
    }

    pub fn len(&self) -> usize {
        self.lifecycle.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lifecycle.is_empty()
    }

    fn persist(&mut self) -> Result<(), String> {
        if self.lifecycle.is_empty() {
            match fs::remove_file(&self.store_path) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(format!("failed to clear MCP auth watch store: {error}")),
            }
        }

        let parent = self
            .store_path
            .parent()
            .ok_or("MCP auth watch store requires a parent directory")?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create MCP auth watch store directory: {error}"))?;
        let payload = serde_json::to_vec(&self.snapshot())
            .map_err(|error| format!("failed to serialize MCP auth watch store: {error}"))?;
        if payload.len() as u64 > MAX_STORE_BYTES {
            return Err("MCP auth watch store exceeds bounded size".into());
        }
        let temporary = self.store_path.with_extension("tmp");
        let write_result = (|| -> Result<(), String> {
            let mut file = File::create(&temporary)
                .map_err(|error| format!("failed to create MCP auth watch store: {error}"))?;
            file.write_all(&payload)
                .map_err(|error| format!("failed to write MCP auth watch store: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("failed to sync MCP auth watch store: {error}"))?;
            fs::rename(&temporary, &self.store_path)
                .map_err(|error| format!("failed to commit MCP auth watch store: {error}"))?;
            Ok(())
        })();
        let _ = fs::remove_file(&temporary);
        write_result
    }
}

fn watch_to_value(watch: &PendingMcpAuthWatch) -> Value {
    json!({
        "generation": watch.generation,
        "serverId": watch.server_id,
        "serverName": watch.server_name,
        "serverUrl": watch.server_url,
        "accountKey": watch.account_key,
        "requestingAgentId": watch.requesting_agent_id,
        "forceReauth": watch.force_reauth,
        "suppressFirstPoll": watch.suppress_first_poll,
        "startedAtMs": watch.started_at_ms,
        "expiresAtMs": watch.expires_at_ms,
        "nextPollAtMs": watch.next_poll_at_ms,
        "isPolling": watch.is_polling,
    })
}

fn parse_store(raw: &str, now_ms: u64) -> Result<McpAuthWatchLifecycle, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|_| "MCP auth watch store is invalid JSON")?;
    if value.get("version").and_then(Value::as_u64) != Some(STORE_VERSION) {
        return Err("MCP auth watch store version is unsupported".into());
    }
    let next_generation = value
        .get("nextGeneration")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1);
    let watches = value
        .get("watches")
        .and_then(Value::as_array)
        .ok_or("MCP auth watch store is missing watches")?
        .iter()
        .filter_map(parse_watch)
        .collect::<Vec<_>>();
    Ok(McpAuthWatchLifecycle::from_restored(
        now_ms,
        next_generation,
        watches,
    ))
}

fn parse_watch(value: &Value) -> Option<PendingMcpAuthWatch> {
    let generation = value.get("generation")?.as_u64()?;
    let server_id = bounded_string(value.get("serverId")?, 32)?;
    let server_name = bounded_string(value.get("serverName")?, 320)?;
    let server_url = bounded_string(value.get("serverUrl")?, 4096)?;
    let account_key = bounded_string(value.get("accountKey")?, 320)?;
    let requesting_agent_id = value
        .get("requestingAgentId")
        .and_then(|value| bounded_string(value, 320));
    Some(PendingMcpAuthWatch {
        generation,
        server_id,
        server_name,
        server_url,
        account_key,
        requesting_agent_id,
        force_reauth: value.get("forceReauth").and_then(Value::as_bool).unwrap_or(false),
        suppress_first_poll: value
            .get("suppressFirstPoll")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        started_at_ms: value.get("startedAtMs")?.as_u64()?,
        expires_at_ms: value.get("expiresAtMs")?.as_u64()?,
        next_poll_at_ms: value.get("nextPollAtMs")?.as_u64()?,
        is_polling: value.get("isPolling").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn bounded_string(value: &Value, max: usize) -> Option<String> {
    let raw = value.as_str()?.trim();
    (!raw.is_empty() && raw.len() <= max && !raw.chars().any(char::is_control))
        .then(|| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fabushi_android_shared::node::mcp::mcp_auth_watch::{
        AUTH_WATCH_POLL_INTERVAL_MS, AUTH_WATCH_TIMEOUT_MS,
    };

    fn temp_store(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-mcp-auth-watch-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn replacement_and_process_death_preserve_latest_watch_and_requester() {
        let path = temp_store("restart");
        let mut manager = AndroidMcpAuthWatchManager::open(&path, 0).unwrap();
        let first = manager
            .begin_watch(
                0,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "default",
                Some("agent-a"),
                false,
            )
            .unwrap()
            .0;
        let second = manager
            .begin_watch(
                1,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "default",
                None,
                false,
            )
            .unwrap()
            .0;
        assert!(second.generation > first.generation);
        assert_eq!(second.requesting_agent_id.as_deref(), Some("agent-a"));
        drop(manager);

        let reopened = AndroidMcpAuthWatchManager::open(&path, 2).unwrap();
        let restored = reopened.watch("17", "default").unwrap();
        assert_eq!(restored.generation, second.generation);
        assert_eq!(restored.requesting_agent_id.as_deref(), Some("agent-a"));
        assert!(!restored.is_polling);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn duplicate_completion_and_stale_poll_are_fenced() {
        let path = temp_store("stale");
        let mut manager = AndroidMcpAuthWatchManager::open(&path, 0).unwrap();
        manager
            .begin_watch(
                0,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "default",
                Some("agent-a"),
                false,
            )
            .unwrap();
        let request = match manager
            .poll_tick(AUTH_WATCH_POLL_INTERVAL_MS, "17", "default")
            .unwrap()
        {
            McpAuthPollTick::Request(request) => request,
            other => panic!("expected poll request, got {other:?}"),
        };
        assert!(manager
            .note_auth_completed_elsewhere("17", "default")
            .unwrap()
            .is_some());
        assert!(manager
            .note_auth_completed_elsewhere("17", "default")
            .unwrap()
            .is_none());
        assert_eq!(
            manager
                .settle_poll(
                    AUTH_WATCH_POLL_INTERVAL_MS + 1,
                    &request,
                    McpAuthPollOutcome::TokenValid,
                )
                .unwrap(),
            McpAuthPollSettlement::Stale
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn force_reauth_timeout_cancel_and_account_switch_are_durable() {
        let path = temp_store("lifecycle");
        let mut manager = AndroidMcpAuthWatchManager::open(&path, 0).unwrap();
        manager
            .begin_watch(
                0,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "account-a",
                None,
                true,
            )
            .unwrap();
        assert!(matches!(
            manager
                .poll_tick(AUTH_WATCH_POLL_INTERVAL_MS, "17", "account-a")
                .unwrap(),
            McpAuthPollTick::Suppressed
        ));
        assert!(manager
            .note_auth_completed_elsewhere("17", "account-b")
            .unwrap()
            .is_none());
        assert_eq!(manager.len(), 1);
        assert!(matches!(
            manager
                .poll_tick(AUTH_WATCH_TIMEOUT_MS, "17", "account-a")
                .unwrap(),
            McpAuthPollTick::Expired(completion)
                if completion.server_id == "17"
                    && completion.account_key == "account-a"
                    && completion.outcome == "timeout"
        ));
        assert!(manager.is_empty());

        manager
            .begin_watch(
                AUTH_WATCH_TIMEOUT_MS + 1,
                "18",
                "Drive",
                "https://mcp.example.test",
                "default",
                None,
                false,
            )
            .unwrap();
        assert!(manager.cancel_watch("18", "default").unwrap().is_some());
        assert!(manager.is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn corrupted_store_fails_closed_and_is_not_restored() {
        let path = temp_store("corrupt");
        fs::write(&path, b"{not-json").unwrap();
        let manager = AndroidMcpAuthWatchManager::open(&path, 0).unwrap();
        assert!(manager.is_empty());
        assert!(!Path::new(&path).exists());
    }
}
