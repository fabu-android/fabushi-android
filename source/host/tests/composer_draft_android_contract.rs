use fabushi_mahayana_host::{
    android_json_runtime::{AndroidHostMode, AndroidJsonHost},
    composer_draft::DurableComposerDraftOwner,
};
use serde_json::json;

#[test]
fn composer_draft_survives_process_reopen_and_is_account_agent_fenced() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("composer-drafts-unit.json");
    let first = {
        let mut owner = DurableComposerDraftOwner::open(&path, 10).unwrap();
        owner
            .write(Some("slot-a"), "fence-a", "agent-a", "unfinished", 11)
            .unwrap()
    };
    let reopened = DurableComposerDraftOwner::open(&path, 20).unwrap();
    assert_eq!(
        reopened
            .read(Some("slot-a"), "fence-a", "agent-a")
            .unwrap()
            .unwrap()
            .draft_id,
        first.draft_id
    );
    assert!(reopened.read(Some("slot-b"), "fence-a", "agent-a").unwrap().is_none());
    assert!(reopened.read(Some("slot-a"), "fence-b", "agent-a").unwrap().is_none());
    assert!(reopened.read(Some("slot-a"), "fence-a", "agent-b").unwrap().is_none());
}

#[test]
fn clear_if_current_preserves_newer_user_edit() {
    let root = tempfile::tempdir().unwrap();
    let mut owner =
        DurableComposerDraftOwner::open(root.path().join("composer-drafts-unit.json"), 10).unwrap();
    let sent = owner
        .write(Some("slot-a"), "fence-a", "agent-a", "sent text", 11)
        .unwrap();
    let newer = owner
        .write(Some("slot-a"), "fence-a", "agent-a", "newer edit", 12)
        .unwrap();
    assert_ne!(sent.draft_id, newer.draft_id);
    assert!(!owner
        .clear_if_current(Some("slot-a"), "fence-a", "agent-a", &sent.draft_id)
        .unwrap());
    assert_eq!(
        owner
            .read(Some("slot-a"), "fence-a", "agent-a")
            .unwrap()
            .unwrap()
            .prompt,
        "newer edit"
    );
    assert!(owner
        .clear_if_current(Some("slot-a"), "fence-a", "agent-a", &newer.draft_id)
        .unwrap());
}

#[test]
fn host_contract_restores_draft_and_rejects_stale_clear_after_reopen() {
    let root = tempfile::tempdir().unwrap();
    let first_id = {
        let mut host = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
        host.dispatch(
            "feature.composerDraft.write",
            &json!({"accountSlot":"slot-a","agentId":"agent-a","prompt":"first"}),
        )
        .unwrap()["draftId"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let mut reopened = AndroidJsonHost::new(root.path(), AndroidHostMode::Test);
    assert_eq!(
        reopened
            .dispatch(
                "feature.composerDraft.read",
                &json!({"accountSlot":"slot-a","agentId":"agent-a"}),
            )
            .unwrap()["draft"]["prompt"],
        "first"
    );
    let second_id = reopened
        .dispatch(
            "feature.composerDraft.write",
            &json!({"accountSlot":"slot-a","agentId":"agent-a","prompt":"second"}),
        )
        .unwrap()["draftId"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!reopened
        .dispatch(
            "feature.composerDraft.clearIfCurrent",
            &json!({"accountSlot":"slot-a","agentId":"agent-a","draftId":first_id}),
        )
        .unwrap()["cleared"]
        .as_bool()
        .unwrap());
    assert!(reopened
        .dispatch(
            "feature.composerDraft.clearIfCurrent",
            &json!({"accountSlot":"slot-a","agentId":"agent-a","draftId":second_id}),
        )
        .unwrap()["cleared"]
        .as_bool()
        .unwrap());
}

#[test]
fn corrupt_draft_store_is_quarantined_instead_of_becoming_parallel_truth() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("composer-drafts-unit.json");
    std::fs::write(&path, b"{not-json").unwrap();
    let owner = DurableComposerDraftOwner::open(&path, 99).unwrap();
    assert!(owner.read(None, "fence-a", "agent-a").unwrap().is_none());
    assert!(root.path().join("composer-drafts-unit.corrupt-99.json").exists());
}
