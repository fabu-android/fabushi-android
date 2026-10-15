use std::sync::{Arc, Mutex, atomic::{AtomicU64, AtomicUsize, Ordering}};
use std::time::Duration;

use fabushi_android_shared::model_catalog::SandModelCatalogEntry;
use fabushi_mahayana_host::extensions::cloud_agents::cloud_agent_poll_loop::*;

fn environment(id:&str,name:&str)->SavedEnvironment{SavedEnvironment{public_id:id.into(),name:name.into()}}
struct EnvClient{values:Vec<SavedEnvironment>}
impl SavedEnvironmentClient for EnvClient{
    fn get_environment(&self,id:&str)->Result<Option<SavedEnvironment>,String>{Ok(self.values.iter().find(|v|v.public_id==id).cloned())}
    fn list_environments(&self,_:usize)->Result<Vec<SavedEnvironment>,String>{Ok(self.values.clone())}
}

#[test]
fn saved_environment_selection_is_id_first_case_insensitive_and_unambiguous(){
    let c=EnvClient{values:vec![environment("one","Prod"),environment("two","Dev")]};
    assert_eq!(resolve_saved_environment(&c,Some("two"),None).unwrap().name,"Dev");
    assert_eq!(resolve_saved_environment(&c,None,Some("prod")).unwrap().public_id,"one");
    let e=resolve_saved_environment(&c,Some("missing"),None).unwrap_err(); assert!(e.contains("Prod (one)"));
    let dup=EnvClient{values:vec![environment("a","prod"),environment("b","PROD")]};
    assert!(resolve_saved_environment(&dup,None,Some("prod")).unwrap_err().contains("Multiple saved environments"));
}

#[test]
fn terminal_error_rate_limit_and_diff_projection_match_desktop_contract(){
    let complete=build_watch_result("bc-1",CloudAgentRunStatus::Finished,&CloudAgentTerminalDetail{branch:Some("agent/work".into()),pr_url:Some("https://example/pr".into()),commit_count:2,files_changed:3,lines_added:10,lines_removed:4,summary:Some("Done.".into()),..Default::default()});
    assert_eq!(complete.status,"completed"); assert!(complete.text.contains("2 commits, +10/-4 across 3 files")); assert!(complete.text.contains("Done."));
    let failed=build_watch_result("bc-2",CloudAgentRunStatus::Expired,&CloudAgentTerminalDetail{error_title:Some("Limit reached".into()),error_detail:Some("Upgrade or wait.".into()),rate_limit_reason:Some("sand_included_limit".into()),..Default::default()});
    assert_eq!(failed.status,"error"); assert!(failed.text.contains("expired before finishing")); assert!(failed.text.contains("Limit reached\nUpgrade or wait."));
}

#[test]
fn deterministic_jitter_is_stable_and_bounded(){
    let a=cloud_agent_rate_limit_pause_ms(Some(1_000),"account-a/device-1/op-1",0);
    let b=cloud_agent_rate_limit_pause_ms(Some(1_000),"account-a/device-1/op-1",0);
    assert_eq!(a,b); assert!((1_000..=1_250).contains(&a));
    assert!((CLOUD_AGENT_RATE_LIMIT_FALLBACK_MS..=CLOUD_AGENT_RATE_LIMIT_FALLBACK_MS+CLOUD_AGENT_RATE_LIMIT_FALLBACK_MS/4).contains(&cloud_agent_rate_limit_pause_ms(None,"x",1)));
}

#[test]
fn model_catalog_ttl_reuses_then_refreshes(){
    let now=Arc::new(AtomicU64::new(100)); let calls=Arc::new(AtomicUsize::new(0));
    let cache=CloudAgentModelCatalogCache::new({let n=Arc::clone(&now);Arc::new(move||n.load(Ordering::SeqCst))},{let c=Arc::clone(&calls);Arc::new(move||{let i=c.fetch_add(1,Ordering::SeqCst)+1;Ok(vec![SandModelCatalogEntry{id:format!("model-{i}"),display_name:None,aliases:vec![],params:vec![],variants:vec![]}])})});
    assert_eq!(cache.list_models().unwrap()[0].id,"model-1"); assert_eq!(cache.list_models().unwrap()[0].id,"model-1"); assert_eq!(calls.load(Ordering::SeqCst),1);
    now.store(100+MODEL_CATALOG_TTL_MS,Ordering::SeqCst); assert_eq!(cache.list_models().unwrap()[0].id,"model-2");
}

#[test]
fn admin_cache_is_fail_open_then_refreshes_without_blocking_callers(){
    let now=Arc::new(AtomicU64::new(0)); let calls=Arc::new(AtomicUsize::new(0));
    let cache=CloudAgentTeamAdminPolicyCache::new({let n=Arc::clone(&now);Arc::new(move||n.load(Ordering::SeqCst))},{let c=Arc::clone(&calls);Arc::new(move||{c.fetch_add(1,Ordering::SeqCst);Ok(true)})});
    assert!(!cache.is_disabled_by_team_admin()); cache.wait_for_refresh_for_testing(); assert!(cache.is_disabled_by_team_admin()); assert_eq!(calls.load(Ordering::SeqCst),1);
    now.store(TEAM_ADMIN_POLICY_TTL_MS,Ordering::SeqCst); cache.prefetch(); cache.wait_for_refresh_for_testing(); assert_eq!(calls.load(Ordering::SeqCst),2);
}

#[test]
fn completion_poller_honors_restart_grace_rate_limit_and_terminal_state(){
    let now=Arc::new(AtomicU64::new(0)); let calls=Arc::new(AtomicUsize::new(0));
    let responses=Arc::new(Mutex::new(vec![Err(CloudAgentPollError{is_rate_limit:true,retry_after_ms:Some(20)}),Ok((CloudAgentRunStatus::Running,CloudAgentTerminalDetail::default())),Ok((CloudAgentRunStatus::Finished,CloudAgentTerminalDetail{pr_url:Some("https://example/pr".into()),..Default::default()}))]));
    let poller=CloudAgentCompletionPoller::new("account-a/device-1/op-1".into(),{let n=Arc::clone(&now);Arc::new(move||n.load(Ordering::SeqCst))},{let n=Arc::clone(&now);Arc::new(move|d:Duration|{n.fetch_add(d.as_millis() as u64,Ordering::SeqCst);})},{let r=Arc::clone(&responses);let c=Arc::clone(&calls);Arc::new(move|_,timeout|{assert_eq!(timeout,CLOUD_AGENT_POLL_RPC_TIMEOUT_MS);c.fetch_add(1,Ordering::SeqCst);r.lock().unwrap().remove(0)})}).with_limits(1_000,10,100);
    let result=poller.await_completion("bc-1",false); assert_eq!(result.status,"completed"); assert!(result.text.contains("https://example/pr")); assert_eq!(calls.load(Ordering::SeqCst),3);
}

#[test]
fn production_reconcile_uses_bounded_poll_policy_without_replaying_side_effects() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let runtime = std::fs::read_to_string(root.join("host/src/android_json_runtime.rs")).unwrap();
    let production = std::fs::read_to_string(root.join("box-exec-daemon/src/cloud_agent_production.rs")).unwrap();
    for needle in [
        "CLOUD_AGENT_MAX_WAIT_MS",
        "CLOUD_AGENT_POLL_INTERVAL_MS",
        "parse_rate_limit_retry_after_ms",
        "cloud_agent_rate_limit_pause_ms",
        "cloud_agent_rate_limited",
        "runner.reconcile_authorized(&context, now_ms())",
        "durable operation was not replayed",
    ] {
        assert!(runtime.contains(needle), "shipping Host CloudAgent reconcile must contain {needle}");
    }
    assert!(production.contains("ureq::Error::Status(429, response)"));
    assert!(production.contains("retry_after_ms="));
    assert!(production.contains("Never replay launch/reply/rename/cancel/archive/delete"));
}
