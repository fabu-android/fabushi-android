#[derive(Clone,Debug,PartialEq,Eq)]
pub enum AgentActivity{Thinking,RunningTool(String),Waiting,Completed,Failed(String)}
pub fn derive_tool_call_activity(tool_name:&str,target:Option<&str>)->AgentActivity{
    let detail=match target{Some(value) if !value.is_empty()=>format!("{} {}",tool_name,value),_=>tool_name.to_string()};
    AgentActivity::RunningTool(detail)
}
pub fn parse_args_json(value:&str)->bool{let t=value.trim();t.starts_with('{')&&t.ends_with('}')}
pub fn file_basename(path:&str)->&str{path.rsplit(['/', '\\']).next().unwrap_or(path)}
