//! Account-fenced managed team-rule snapshot owner.
//!
//! Desktop preserves the last known-good managed-rule snapshot when Dashboard refresh fails.
//! Android keeps the same Host-owned semantic, additionally fencing snapshots by canonical
//! account session so a stale rule set can never cross an account switch.

use serde_json::Value;
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Default)]
pub struct ManagedTeamRulesResolver {
    snapshots: Mutex<BTreeMap<String, Vec<Value>>>,
    refresh_lock: Mutex<()>,
}

impl ManagedTeamRulesResolver {
    pub fn resolve_with<F>(&self, account_fence: &str, fetch: F) -> Result<Vec<Value>, String>
    where
        F: FnOnce() -> Result<Vec<Value>, String>,
    {
        if account_fence.trim().is_empty() {
            return Err("managed team rules require a non-empty account fence".into());
        }
        let _refresh = self
            .refresh_lock
            .lock()
            .map_err(|_| "managed team rules refresh lock poisoned".to_string())?;
        match fetch() {
            Ok(rules) => {
                self.snapshots
                    .lock()
                    .map_err(|_| "managed team rules snapshot lock poisoned".to_string())?
                    .insert(account_fence.to_string(), rules.clone());
                Ok(rules)
            }
            Err(error) => self
                .snapshot(account_fence)?
                .ok_or(error),
        }
    }

    pub fn snapshot(&self, account_fence: &str) -> Result<Option<Vec<Value>>, String> {
        Ok(self
            .snapshots
            .lock()
            .map_err(|_| "managed team rules snapshot lock poisoned".to_string())?
            .get(account_fence)
            .cloned())
    }

    pub fn clear_account(&self, account_fence: &str) -> Result<(), String> {
        self.snapshots
            .lock()
            .map_err(|_| "managed team rules snapshot lock poisoned".to_string())?
            .remove(account_fence);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn failed_refresh_preserves_same_account_known_good_snapshot() {
        let resolver = ManagedTeamRulesResolver::default();
        let first = resolver
            .resolve_with("session:a", || Ok(vec![json!({"fullPath":"security"})]))
            .unwrap();
        assert_eq!(first.len(), 1);
        let fallback = resolver
            .resolve_with("session:a", || Err("dashboard unavailable".into()))
            .unwrap();
        assert_eq!(fallback, first);
    }

    #[test]
    fn first_refresh_failure_remains_a_real_error() {
        let resolver = ManagedTeamRulesResolver::default();
        assert_eq!(
            resolver
                .resolve_with("session:a", || Err("auth unavailable".into()))
                .unwrap_err(),
            "auth unavailable"
        );
    }

    #[test]
    fn snapshots_are_account_fenced_and_clearable() {
        let resolver = ManagedTeamRulesResolver::default();
        resolver
            .resolve_with("session:a", || Ok(vec![json!({"fullPath":"a"})]))
            .unwrap();
        assert!(resolver.snapshot("session:b").unwrap().is_none());
        assert!(resolver
            .resolve_with("session:b", || Err("offline".into()))
            .is_err());
        resolver.clear_account("session:a").unwrap();
        assert!(resolver.snapshot("session:a").unwrap().is_none());
    }
}
