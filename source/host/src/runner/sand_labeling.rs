use std::collections::BTreeMap;

#[derive(Default, Debug)]
pub struct FollowupLabelingTracker {
    previous_request_by_scope: BTreeMap<String, String>,
}

impl FollowupLabelingTracker {
    fn scope_key(account_fence: &str, conversation_id: &str) -> String {
        format!("{account_fence}\n{conversation_id}")
    }

    pub fn remember_turn(
        &mut self,
        account_fence: &str,
        conversation_id: &str,
        request_id: &str,
    ) -> Option<String> {
        if account_fence.trim().is_empty()
            || conversation_id.trim().is_empty()
            || request_id.trim().is_empty()
        {
            return None;
        }
        self.previous_request_by_scope.insert(
            Self::scope_key(account_fence, conversation_id),
            request_id.to_string(),
        )
    }

    pub fn clear_account(&mut self, account_fence: &str) {
        let prefix = format!("{account_fence}\n");
        self.previous_request_by_scope
            .retain(|scope, _| !scope.starts_with(&prefix));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn followup_is_scoped_to_account_and_conversation() {
        let mut tracker = FollowupLabelingTracker::default();
        assert_eq!(tracker.remember_turn("acct-a", "agent-1", "req-1"), None);
        assert_eq!(
            tracker.remember_turn("acct-a", "agent-1", "req-2").as_deref(),
            Some("req-1")
        );
        assert_eq!(tracker.remember_turn("acct-a", "agent-2", "req-x"), None);
        assert_eq!(tracker.remember_turn("acct-b", "agent-1", "req-y"), None);
        assert_eq!(
            tracker.remember_turn("acct-a", "agent-1", "req-3").as_deref(),
            Some("req-2")
        );
    }

    #[test]
    fn account_clear_drops_only_that_accounts_lineage() {
        let mut tracker = FollowupLabelingTracker::default();
        tracker.remember_turn("acct-a", "agent", "a-1");
        tracker.remember_turn("acct-b", "agent", "b-1");
        tracker.clear_account("acct-a");
        assert_eq!(tracker.remember_turn("acct-a", "agent", "a-2"), None);
        assert_eq!(
            tracker.remember_turn("acct-b", "agent", "b-2").as_deref(),
            Some("b-1")
        );
    }
}
