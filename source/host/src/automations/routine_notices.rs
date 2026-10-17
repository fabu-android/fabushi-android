use serde_json::Value;
use super::automation_trigger::trigger_members;
pub const GITHUB_LISTENER_SCOPE:&str="github-listener-scope";
pub const GITHUB_LISTENER_SCOPE_CREATED_BEFORE_MS:f64=1_785_369_600_000.0;
pub fn is_routine_notice_id(value:&str)->bool{value==GITHUB_LISTENER_SCOPE}
pub fn routine_notice_ids_to_raise(created_at:f64,trigger:&Value,raised:&[String])->Vec<String>{
    if created_at>=GITHUB_LISTENER_SCOPE_CREATED_BEFORE_MS||raised.iter().any(|id|id==GITHUB_LISTENER_SCOPE){return Vec::new();}
    let applies=trigger_members(trigger).iter().any(|listener|listener.get("type").and_then(Value::as_str)==Some("github")&&listener.get("userAllowlist").and_then(Value::as_array).is_none_or(|v|v.is_empty()));
    applies.then(||vec![GITHUB_LISTENER_SCOPE.to_string()]).unwrap_or_default()
}

pub fn routine_notice_lines_for_ids(ids:&[String])->Vec<String>{
    let mut lines=Vec::new();
    if ids.iter().any(|id|id==GITHUB_LISTENER_SCOPE){
        lines.push(format!(
            "NOTICE {GITHUB_LISTENER_SCOPE} (raised once for this routine, and only here — act on it now or not at all): this routine's github listener filters nobody, so it fires for everyone in the repo, the shape of a listener written before userAllowlist existed. Decide whether this event was genuinely in scope for the saved prompt; silence by design is not a wasted fire."
        ));
        lines.push("If this fire is clearly wasted, update the listener now: narrow userAllowlist to a confirmed GitHub login or remove event kinds the saved prompt never covered. CI events are never user-gated. Tell the user what changed, and leave it alone when the mismatch or login is uncertain.".into());
    }
    lines
}
pub fn routine_notice_wake_lines(created_at:f64,trigger:&Value,raised:&[String])->Vec<String>{
    routine_notice_lines_for_ids(&routine_notice_ids_to_raise(created_at,trigger,raised))
}
