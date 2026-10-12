use crate::interaction_updates::{InteractionKind, InteractionUpdate};

pub fn updates_after(updates: &[InteractionUpdate], sequence: u64) -> Vec<InteractionUpdate> {
    updates.iter().filter(|update| update.sequence > sequence).cloned().collect()
}

pub fn latest_of_kind(updates: &[InteractionUpdate], kind: InteractionKind) -> Option<&InteractionUpdate> {
    updates.iter().rev().find(|update| update.kind == kind)
}
