use fabushi_android_shared::node::mcp::mcp_server_id::validate_mcp_server_id;
use std::collections::BTreeMap;

pub const DEFAULT_MCP_AUTH_WAIT_TTL_MS: u64 = 60 * 60 * 1_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpAuthCompletionIdentity {
    pub server_id: String,
    pub server_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpAuthWaitRegistration {
    pub agent_id: String,
    pub connector: String,
    pub server_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WaitEntry {
    agent_id: String,
    server_id: Option<String>,
    expires_at_ms: u64,
}

#[derive(Clone, Debug)]
pub struct McpAuthWaitRegistry {
    waits: BTreeMap<String, WaitEntry>,
    ttl_ms: u64,
}

pub fn normalize_connector_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

impl McpAuthWaitRegistry {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            waits: BTreeMap::new(),
            ttl_ms: ttl_ms.max(1),
        }
    }

    pub fn register(&mut self, now_ms: u64, registration: McpAuthWaitRegistration) {
        self.prune(now_ms);
        if registration.agent_id.trim().is_empty() {
            return;
        }

        let server_id = registration
            .server_id
            .and_then(|value| validate_mcp_server_id(&value).ok());
        let name_key = normalize_connector_name(&registration.connector);
        let key = if !name_key.is_empty() {
            Some(name_key)
        } else {
            server_id.as_ref().map(|id| format!("id:{id}"))
        };

        if let Some(key) = key {
            self.waits.insert(
                key,
                WaitEntry {
                    agent_id: registration.agent_id,
                    server_id,
                    expires_at_ms: now_ms.saturating_add(self.ttl_ms),
                },
            );
        }
    }

    pub fn take(
        &mut self,
        now_ms: u64,
        completion: &McpAuthCompletionIdentity,
    ) -> Option<String> {
        self.prune(now_ms);
        let name_key = normalize_connector_name(&completion.server_name);
        let mut id_match: Option<String> = None;
        let mut name_match: Option<String> = None;
        let mut matched_keys = Vec::new();

        for (key, entry) in &self.waits {
            let matches_id = entry
                .server_id
                .as_deref()
                .is_some_and(|id| id == completion.server_id);
            let matches_name =
                entry.server_id.is_none() && !name_key.is_empty() && key == &name_key;
            if matches_id || matches_name {
                matched_keys.push(key.clone());
                if matches_id {
                    id_match = Some(entry.agent_id.clone());
                } else if name_match.is_none() {
                    name_match = Some(entry.agent_id.clone());
                }
            }
        }

        for key in matched_keys {
            self.waits.remove(&key);
        }
        id_match.or(name_match)
    }

    pub fn prune(&mut self, now_ms: u64) {
        self.waits
            .retain(|_, entry| entry.expires_at_ms > now_ms);
    }

    pub fn len(&self) -> usize {
        self.waits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waits.is_empty()
    }
}

impl Default for McpAuthWaitRegistry {
    fn default() -> Self {
        Self::new(DEFAULT_MCP_AUTH_WAIT_TTL_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_id_match_wins_and_consumes_matching_waits() {
        let mut waits = McpAuthWaitRegistry::new(1_000);
        waits.register(
            10,
            McpAuthWaitRegistration {
                agent_id: "by-name".into(),
                connector: "Git Hub".into(),
                server_id: None,
            },
        );
        waits.register(
            10,
            McpAuthWaitRegistration {
                agent_id: "by-id".into(),
                connector: "Different".into(),
                server_id: Some("srv-1".into()),
            },
        );

        let agent = waits.take(
            20,
            &McpAuthCompletionIdentity {
                server_id: "srv-1".into(),
                server_name: "GitHub".into(),
            },
        );
        assert_eq!(agent.as_deref(), Some("by-id"));
        assert!(waits.is_empty());
    }

    #[test]
    #[test]
    fn invalid_server_id_registration_does_not_create_id_match() {
        let mut waits = McpAuthWaitRegistry::new(1_000);
        waits.register(
            10,
            McpAuthWaitRegistration {
                agent_id: "agent-a".into(),
                connector: "".into(),
                server_id: Some("not-a-server".into()),
            },
        );
        assert!(waits.is_empty());

        waits.register(
            10,
            McpAuthWaitRegistration {
                agent_id: "agent-b".into(),
                connector: "GitHub".into(),
                server_id: Some("01".into()),
            },
        );
        assert_eq!(
            waits.take(
                20,
                &McpAuthCompletionIdentity {
                    server_id: "1".into(),
                    server_name: "github".into(),
                },
            )
            .as_deref(),
            Some("agent-b")
        );
    }

    fn name_fallback_is_normalized_and_expired_waits_are_ignored() {
        let mut waits = McpAuthWaitRegistry::new(100);
        waits.register(
            10,
            McpAuthWaitRegistration {
                agent_id: "agent-a".into(),
                connector: "Google Drive".into(),
                server_id: None,
            },
        );
        assert_eq!(
            waits.take(
                50,
                &McpAuthCompletionIdentity {
                    server_id: "other".into(),
                    server_name: "google-drive".into(),
                },
            )
            .as_deref(),
            Some("agent-a")
        );

        waits.register(
            100,
            McpAuthWaitRegistration {
                agent_id: "expired".into(),
                connector: "Calendar".into(),
                server_id: None,
            },
        );
        assert_eq!(
            waits.take(
                200,
                &McpAuthCompletionIdentity {
                    server_id: "none".into(),
                    server_name: "calendar".into(),
                },
            ),
            None
        );
    }
}
