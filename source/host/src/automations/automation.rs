use std::path::PathBuf;
use serde_json::Value;
use uuid::Uuid;
use super::automation_trigger::{trigger_schedule,trigger_members};

pub const AUTOMATION_MAX_NAME_LENGTH:usize=80;
pub const AUTOMATION_MAX_PER_AGENT:usize=50;
pub const AUTOMATION_UI_LIMIT:usize=100;
pub const AUTOMATION_MAX_RUN_HISTORY:usize=20;
pub const AUTOMATION_MAX_RUN_DETAIL_LENGTH:usize=300;
pub const MAX_EVENTS_IN_AUTOMATION_WAKE:usize=25;
pub const AUTOMATION_STATUS_PROMPT_MARKER:&str="<automation_status>";
pub const AUTOMATION_PROMPT_GUIDANCE_VERSION:&str="backend_triggers_v4";

#[derive(Debug,Clone,PartialEq)]
pub struct AutomationRun{pub id:String,pub trigger:String,pub started_at:f64,pub finished_at:Option<f64>,pub status:String,pub detail:Option<String>,pub event:Option<String>,pub coalesced_run_ids:Option<Vec<String>>}
#[derive(Debug,Clone,PartialEq)]
pub struct AutomationConfig{pub name:String,pub prompt:String,pub trigger:Value,pub is_enabled:bool,pub created_at:f64,pub last_run_at:Option<f64>,pub raised_notices:Vec<String>}
#[derive(Debug,Clone,PartialEq)]
pub struct AutomationRecord{pub id:String,pub name:String,pub prompt:String,pub trigger:Value,pub is_enabled:bool,pub created_at:f64,pub last_run_at:Option<f64>,pub raised_notices:Vec<String>,pub schedule:String,pub trigger_description:String,pub next_run_at:Option<f64>,pub runs:Vec<AutomationRun>,pub file_path:PathBuf}
#[derive(Debug,Clone,PartialEq)]
pub struct AutomationSpec{pub name:String,pub prompt:String,pub trigger:Value,pub is_enabled:Option<bool>}

pub fn clamp_automation_name(name:&str)->String{name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(AUTOMATION_MAX_NAME_LENGTH).collect()}
pub fn normalize_automation_prompt(prompt:&str)->String{prompt.trim().to_string()}
pub fn slugify_automation_name(name:&str)->String{let mut out=String::new();let mut dash=false;for c in name.trim().to_ascii_lowercase().chars(){if c.is_ascii_alphanumeric(){out.push(c);dash=false;}else if !dash&&!out.is_empty(){out.push('-');dash=true;}}while out.ends_with('-'){out.pop();}if out.is_empty(){"automation".into()}else{out}}
pub fn create_automation_run_id()->String{Uuid::new_v4().to_string()}
pub fn clamp_run_detail(detail:Option<&str>)->Option<String>{let value=detail?.trim();(!value.is_empty()).then(||value.chars().take(AUTOMATION_MAX_RUN_DETAIL_LENGTH).collect())}
pub fn clamp_coalesced_run_ids(values:Option<&[String]>)->Option<Vec<String>>{let out=values.unwrap_or_default().iter().filter(|id|!id.is_empty()).take(MAX_EVENTS_IN_AUTOMATION_WAKE).cloned().collect::<Vec<_>>();(!out.is_empty()).then_some(out)}
pub fn describe_trigger(trigger:&Value)->String{
    if let Some(schedule)=trigger_schedule(trigger){return format!("Scheduled: {schedule}");}
    let members=trigger_members(trigger); if members.len()>1{return format!("{} event listeners",members.len());}
    let Some(member)=members.first() else{return "Unknown trigger".into();};
    match member.get("type").and_then(Value::as_str).unwrap_or_default(){
        "slack"=>format!("Slack {}",member.get("channel").and_then(Value::as_str).unwrap_or_default()),
        "github"=>format!("GitHub {}",member.get("repo").and_then(Value::as_str).unwrap_or_default()),
        "microsoftTeams"=>"Microsoft Teams event".into(),"linear"=>"Linear event".into(),"sentry"=>"Sentry event".into(),"pagerduty"=>"PagerDuty event".into(),other=>other.to_string(),
    }
}

/// Grok-equivalent routines capability guidance, expressed independently in Rust.
///
/// The Host owns the durable routine definitions and injects this section into
/// the provider system prompt. The renderer never owns or reconstructs this
/// policy.
pub fn render_automations_system_prompt(
    automations:&[AutomationRecord],
    location:Option<&str>,
    time_zone:Option<&str>,
)->String{
    let Some(location)=location.map(str::trim).filter(|value|!value.is_empty()) else{return String::new();};
    let zone_note=time_zone.map(str::trim).filter(|value|!value.is_empty())
        .map(|zone|format!("the user's local time ({zone})"))
        .unwrap_or_else(||"the user's local time".to_string());
    let mut lines=vec![
        "## Routines".to_string(),
        "Routines are durable standing orders: a saved prompt plus either a schedule or an outside-event listener. They can run while the user is away.".to_string(),
        format!("Definitions live under {location}; each routine has its own folder. Read that Host-owned folder when inspection is needed, and use update_state with target \"routine\" for create/update/pause/resume/delete rather than editing files directly."),
        "Treat recurring, time-based, monitoring, reminder, digest, and \"tell me when\" requests as routine candidates. Create one when intent is unambiguous; otherwise propose it briefly instead of pretending to remain awake.".to_string(),
        "Write saved prompts as durable intent, not frozen tool-call arguments. Connector/MCP schemas may change between runs, so discover the current tool contract when a run fires.".to_string(),
        format!("Cron schedules use five fields (minute hour day-of-month month day-of-week) interpreted in {zone_note}. @hourly/@daily/@weekly/@monthly and @every intervals are supported; CRON_TZ=<IANA zone> pins a schedule to a fixed zone."),
        "When the user gives an hour but no minute, preserve the current local minute; use minute 0 only when they explicitly ask for the top of the hour. Prefer the least-frequent cadence that still makes the result useful.".to_string(),
        "For vague recurring work, default to weekday daytime hours in the user's zone rather than nights and weekends. Leave that window only for an explicit seven-day/overnight request, a genuinely time-critical event, an event that only happens then, or a personal-life routine whose correctness requires every day.".to_string(),
        "Use event listeners instead of polling when the platform event exists. Supported listener families are Slack, GitHub, Microsoft Teams, Linear, Sentry, PagerDuty, and groups that OR several listeners together.".to_string(),
        "Slack listeners may target a channel, DM, or wildcard and can match messages, mentions, keywords, or reactions. Channel listeners only receive channels the connected app can access, so surface the channel-invite requirement when relevant.".to_string(),
        "GitHub listeners target one repository and explicit event kinds. CI passed/failed listeners require a concrete branch; user allowlists filter human-driven events but never CI. Confirm actual GitHub logins instead of guessing display names.".to_string(),
        "Microsoft Teams, Linear, Sentry, and PagerDuty listeners may use their platform identifiers to narrow scope. Omitted optional filters mean any matching entity inside the required connection scope.".to_string(),
        "A listener and a schedule are alternatives, not two triggers on one routine. Use a schedule only for genuinely time-based work, unsupported events, or a finite watch that needs a deadline even if no event ever arrives.".to_string(),
        "A hidden [routine] wake is the routine firing, not a fresh user message. Carry out the saved prompt and surface useful results naturally; if the saved instruction explicitly says to stay quiet when nothing changed, silence is a valid outcome.".to_string(),
        "Make finite watches self-expiring: delete an event watch after its matching event, or have a scheduled watch delete itself after success/deadline. Keep a routine indefinitely only for an explicitly ongoing subscription, reminder, or digest.".to_string(),
        "If the same authentication failure keeps blocking a routine, pause it rather than repeatedly waking and failing. For an MCP server use AuthenticateMcpServer so the user can reconnect the affected account, then resume only after auth is restored.".to_string(),
        "Routine writes can require a confirmation card because they act while the user is away. If confirmation is requested, rely on the tool's approval flow instead of asking separately or retrying a denied write with different wording.".to_string(),
    ];
    if automations.is_empty(){
        lines.push("No routines yet.".into());
    }else{
        lines.push("Current routines:".into());
        for automation in automations{
            let state=if automation.is_enabled{"enabled"}else{"paused"};
            let raw=if automation.trigger.get("type").and_then(Value::as_str)==Some("cron")&&!automation.schedule.is_empty(){
                format!(" ({})",automation.schedule)
            }else{String::new()};
            lines.push(format!("- {} [{}] — {}{}; folder {}",automation.name,state,describe_trigger(&automation.trigger),raw,automation.id));
        }
    }
    lines.join("\n")
}
