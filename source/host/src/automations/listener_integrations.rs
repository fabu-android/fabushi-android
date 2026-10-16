use std::collections::BTreeMap;
use serde_json::Value;
use super::automation_trigger::trigger_listener_platforms;
pub fn is_listener_platform(value:&str)->bool{matches!(value,"github"|"slack")}
pub fn count_listener_platforms<'a>(automations:impl IntoIterator<Item=(bool,&'a Value)>)->BTreeMap<String,usize>{
    let mut counts=BTreeMap::from([("github".to_string(),0usize),("slack".to_string(),0usize)]);
    for(enabled,trigger)in automations{if !enabled{continue;}for platform in trigger_listener_platforms(trigger){if let Some(count)=counts.get_mut(&platform){*count+=1;}}}counts
}
pub fn describe_scope_issues(issues:&[(String,String)])->String{
    let missing=issues.iter().filter(|(kind,_)|kind=="bot-not-in-channel").map(|(_,scope)|scope.clone()).collect::<Vec<_>>();
    let not_found=issues.iter().filter(|(kind,_)|kind=="not-found").map(|(_,scope)|scope.clone()).collect::<Vec<_>>();
    let mut parts=Vec::new();
    if !missing.is_empty(){parts.push(format!("Invite @Cursor to {} in Slack — messages there can't reach this listener until the bot joins.",missing.join(", ")));}
    if !not_found.is_empty(){parts.push(format!("Couldn't find {} — check the name, or invite @Cursor to it first if it's a private channel.",not_found.join(", ")));}
    parts.join(" ")
}
