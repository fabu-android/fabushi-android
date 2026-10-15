use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

use fabushi_android_shared::model_catalog::SandModelCatalogEntry;
use sha2::{Digest, Sha256};

pub const CLOUD_AGENT_MAX_WAIT_MS: u64 = 5 * 60 * 60_000;
pub const CLOUD_AGENT_POLL_INTERVAL_MS: u64 = 10_000;
pub const CLOUD_AGENT_POLL_RPC_TIMEOUT_MS: u64 = 30_000;
pub const CLOUD_AGENT_RATE_LIMIT_FALLBACK_MS: u64 = 60_000;
pub const CLOUD_AGENT_RATE_LIMIT_JITTER_RATIO: f64 = 0.25;
pub const CLOUD_AGENT_RUN_RESTART_GRACE_MS: u64 = 3 * 60_000;
pub const SAVED_ENVIRONMENT_LIST_LIMIT: usize = 500;
pub const MAX_LISTED_SAVED_ENVIRONMENTS: usize = 25;
pub const MODEL_CATALOG_TTL_MS: u64 = 5 * 60_000;
pub const TEAM_ADMIN_POLICY_TTL_MS: u64 = 5 * 60_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedEnvironment { pub public_id: String, pub name: String }

pub trait SavedEnvironmentClient: Send + Sync {
    fn get_environment(&self, public_id: &str) -> Result<Option<SavedEnvironment>, String>;
    fn list_environments(&self, limit: usize) -> Result<Vec<SavedEnvironment>, String>;
}

pub fn available_environments_note(values: &[SavedEnvironment]) -> String {
    if values.is_empty() { return " This account has no saved environments; create one from the Cloud Agents dashboard.".into(); }
    let listed = values.iter().take(MAX_LISTED_SAVED_ENVIRONMENTS).map(|v| {
        let name=v.name.trim(); if name.is_empty(){v.public_id.clone()}else{format!("{name} ({})",v.public_id)}
    }).collect::<Vec<_>>().join(", ");
    let hidden=values.len().saturating_sub(MAX_LISTED_SAVED_ENVIRONMENTS);
    if hidden>0 { format!(" Available: {listed} (+{hidden} more; set environment.id to select one that is not listed).") } else { format!(" Available: {listed}.") }
}

pub fn resolve_saved_environment(client:&dyn SavedEnvironmentClient, public_id:Option<&str>, name:Option<&str>) -> Result<SavedEnvironment,String> {
    let id=public_id.unwrap_or_default().trim(); let name=name.unwrap_or_default().trim();
    if !id.is_empty() {
        if let Some(v)=client.get_environment(id)? { return Ok(v); }
        let values=client.list_environments(SAVED_ENVIRONMENT_LIST_LIMIT)?;
        return Err(format!("No saved environment with id '{id}'.{}",available_environments_note(&values)));
    }
    if name.is_empty(){return Err("A saved-environment launch requires the environment public id or name.".into());}
    let values=client.list_environments(SAVED_ENVIRONMENT_LIST_LIMIT)?;
    let matches=values.iter().filter(|v|v.name.trim().eq_ignore_ascii_case(name)).cloned().collect::<Vec<_>>();
    match matches.as_slice(){[v]=>Ok(v.clone()),[]=>Err(format!("No saved environment named '{name}'.{}",available_environments_note(&values))),many=>Err(format!("Multiple saved environments are named '{name}'. Set environment.id to one of: {}.",many.iter().map(|v|v.public_id.as_str()).collect::<Vec<_>>().join(", ")))}
}

pub fn deterministic_jitter_fraction(identity: &str, attempt: u64) -> f64 {
    let mut h=Sha256::new(); h.update(identity.as_bytes()); h.update(attempt.to_le_bytes()); let d=h.finalize();
    let n=u64::from_le_bytes(d[0..8].try_into().expect("sha256 prefix")); (n as f64)/(u64::MAX as f64)
}

pub fn cloud_agent_rate_limit_pause_ms(retry_after_ms:Option<u64>, identity:&str, attempt:u64)->u64{
    let base=retry_after_ms.unwrap_or(CLOUD_AGENT_RATE_LIMIT_FALLBACK_MS);
    let jitter=(base as f64*deterministic_jitter_fraction(identity,attempt)*CLOUD_AGENT_RATE_LIMIT_JITTER_RATIO).round() as u64;
    base.saturating_add(jitter)
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)] pub enum CloudAgentRunStatus{Unspecified,Creating,Running,Finished,Error,Expired}
impl CloudAgentRunStatus{pub fn from_number(v:i64)->Self{match v{1=>Self::Running,2=>Self::Finished,3=>Self::Error,4=>Self::Creating,5=>Self::Expired,_=>Self::Unspecified}} pub fn is_active(self)->bool{matches!(self,Self::Creating|Self::Running)}}

#[derive(Clone,Debug,Default,PartialEq,Eq)] pub struct CloudAgentTerminalDetail{pub branch:Option<String>,pub pr_url:Option<String>,pub commit_count:u64,pub files_changed:u64,pub lines_added:u64,pub lines_removed:u64,pub summary:Option<String>,pub error_title:Option<String>,pub error_detail:Option<String>,pub rate_limit_reason:Option<String>}
#[derive(Clone,Debug,PartialEq,Eq)] pub struct CloudAgentWatchResult{pub status:&'static str,pub text:String}

pub fn build_watch_result(id:&str,status:CloudAgentRunStatus,d:&CloudAgentTerminalDetail)->CloudAgentWatchResult{
    if matches!(status,CloudAgentRunStatus::Error|CloudAgentRunStatus::Expired){
        let mut l=vec![format!("The cloud agent ({id}) {} before finishing.",if status==CloudAgentRunStatus::Expired{"expired"}else{"errored"})];
        if d.rate_limit_reason.as_deref()==Some("sand_included_limit") { let e=[d.error_title.as_deref().unwrap_or("").trim(),d.error_detail.as_deref().unwrap_or("").trim()].into_iter().filter(|v|!v.is_empty()).collect::<Vec<_>>().join("\n"); if !e.is_empty(){l.push(String::new());l.push(e);} }
        if let Some(s)=d.summary.as_deref().map(str::trim).filter(|s|!s.is_empty()){l.push(String::new());l.push(s.into());}
        return CloudAgentWatchResult{status:"error",text:l.join("\n")};
    }
    let mut l=vec!["The cloud agent finished.".to_string()];
    if let Some(b)=d.branch.as_deref().filter(|v|!v.is_empty()){l.push(format!("Branch: {b}"));}
    if let Some(p)=d.pr_url.as_deref().filter(|v|!v.is_empty()){l.push(format!("Pull request: {p}"));}else{l.push("No pull request link is available yet.".into());}
    let mut parts=Vec::new(); if d.commit_count>0{parts.push(format!("{} commit{}",d.commit_count,if d.commit_count==1{""}else{"s"}));} if d.files_changed>0{parts.push(format!("+{}/-{} across {} file{}",d.lines_added,d.lines_removed,d.files_changed,if d.files_changed==1{""}else{"s"}));} if !parts.is_empty(){l.push(format!("Changes: {}.",parts.join(", ")));}
    if let Some(s)=d.summary.as_deref().map(str::trim).filter(|s|!s.is_empty()){l.push(String::new());l.push("Summary from the cloud agent:".into());l.push(s.into());}
    CloudAgentWatchResult{status:"completed",text:l.join("\n")}
}

pub type ModelCatalogLoader=Arc<dyn Fn()->Result<Vec<SandModelCatalogEntry>,String>+Send+Sync>;
pub struct CloudAgentModelCatalogCache{clock:Arc<dyn Fn()->u64+Send+Sync>,load:ModelCatalogLoader,cache:Mutex<Option<(Vec<SandModelCatalogEntry>,u64)>>}
impl CloudAgentModelCatalogCache{pub fn new(clock:Arc<dyn Fn()->u64+Send+Sync>,load:ModelCatalogLoader)->Self{Self{clock,load,cache:Mutex::new(None)}} pub fn list_models(&self)->Result<Vec<SandModelCatalogEntry>,String>{let now=(self.clock)();if let Some((v,t))=self.cache.lock().unwrap_or_else(|p|p.into_inner()).as_ref(){if now.saturating_sub(*t)<MODEL_CATALOG_TTL_MS{return Ok(v.clone());}}let v=(self.load)()?;*self.cache.lock().unwrap_or_else(|p|p.into_inner())=Some((v.clone(),now));Ok(v)}}

pub type TeamAdminPolicyLoader=Arc<dyn Fn()->Result<bool,String>+Send+Sync>;
pub struct CloudAgentTeamAdminPolicyCache{clock:Arc<dyn Fn()->u64+Send+Sync>,load:TeamAdminPolicyLoader,state:Arc<Mutex<Option<(bool,u64)>>>,refreshing:Arc<AtomicBool>}
impl CloudAgentTeamAdminPolicyCache{pub fn new(clock:Arc<dyn Fn()->u64+Send+Sync>,load:TeamAdminPolicyLoader)->Self{Self{clock,load,state:Arc::new(Mutex::new(None)),refreshing:Arc::new(AtomicBool::new(false))}} pub fn is_disabled_by_team_admin(&self)->bool{let v=self.state.lock().unwrap_or_else(|p|p.into_inner()).map(|x|x.0).unwrap_or(false);self.refresh();v} pub fn prefetch(&self){self.refresh()} fn refresh(&self){let now=(self.clock)();if self.state.lock().unwrap_or_else(|p|p.into_inner()).is_some_and(|(_,t)|now.saturating_sub(t)<TEAM_ADMIN_POLICY_TTL_MS){return;}if self.refreshing.swap(true,Ordering::AcqRel){return;}let s=Arc::clone(&self.state);let r=Arc::clone(&self.refreshing);let l=Arc::clone(&self.load);let c=Arc::clone(&self.clock);let _=std::thread::Builder::new().name("cloud-agent-admin-cache".into()).spawn(move||{let previous=s.lock().unwrap_or_else(|p|p.into_inner()).map(|x|x.0).unwrap_or(false);let value=l().unwrap_or(previous);*s.lock().unwrap_or_else(|p|p.into_inner())=Some((value,c()));r.store(false,Ordering::Release);});} pub fn wait_for_refresh_for_testing(&self){while self.refreshing.load(Ordering::Acquire){std::thread::yield_now();}}}

#[derive(Clone,Debug,PartialEq,Eq)] pub struct CloudAgentPollError{pub is_rate_limit:bool,pub retry_after_ms:Option<u64>}
pub type CloudAgentInfoFetcher=Arc<dyn Fn(&str,u64)->Result<(CloudAgentRunStatus,CloudAgentTerminalDetail),CloudAgentPollError>+Send+Sync>;
pub struct CloudAgentCompletionPoller{clock:Arc<dyn Fn()->u64+Send+Sync>,sleep:Arc<dyn Fn(Duration)+Send+Sync>,fetch:CloudAgentInfoFetcher,identity:String,max_wait_ms:u64,poll_ms:u64,restart_grace_ms:u64,disposed:AtomicBool}
impl CloudAgentCompletionPoller{pub fn new(identity:String,clock:Arc<dyn Fn()->u64+Send+Sync>,sleep:Arc<dyn Fn(Duration)+Send+Sync>,fetch:CloudAgentInfoFetcher)->Self{Self{identity,clock,sleep,fetch,max_wait_ms:CLOUD_AGENT_MAX_WAIT_MS,poll_ms:CLOUD_AGENT_POLL_INTERVAL_MS,restart_grace_ms:CLOUD_AGENT_RUN_RESTART_GRACE_MS,disposed:AtomicBool::new(false)}} pub fn with_limits(mut self,max:u64,poll:u64,restart:u64)->Self{self.max_wait_ms=max;self.poll_ms=poll;self.restart_grace_ms=restart;self} pub fn dispose(&self){self.disposed.store(true,Ordering::Release)} pub fn await_completion(&self,id:&str,wait_for_restart:bool)->CloudAgentWatchResult{let started=(self.clock)();let deadline=started.saturating_add(self.max_wait_ms);let restart_deadline=started.saturating_add(self.restart_grace_ms);let mut restart=wait_for_restart;let mut paused_until=0;let mut rate_attempt=0;loop{if self.disposed.load(Ordering::Acquire){return CloudAgentWatchResult{status:"error",text:format!("The cloud agent ({id}) status check stopped before completion.")};}(self.sleep)(Duration::from_millis(self.poll_ms));let now=(self.clock)();if now>=deadline{return CloudAgentWatchResult{status:"error",text:format!("The cloud agent ({id}) is still running after the bounded wait; it was not replayed.")};}if now<paused_until{continue;}match (self.fetch)(id,CLOUD_AGENT_POLL_RPC_TIMEOUT_MS){Err(e)=>{if e.is_rate_limit{paused_until=now.saturating_add(cloud_agent_rate_limit_pause_ms(e.retry_after_ms,&self.identity,rate_attempt));rate_attempt=rate_attempt.saturating_add(1);}continue;}Ok((status,detail))=>{if restart{if status.is_active()||now>=restart_deadline{restart=false}else{continue;}}if status.is_active()||status==CloudAgentRunStatus::Unspecified{continue;}return build_watch_result(id,status,&detail);}}}}}
