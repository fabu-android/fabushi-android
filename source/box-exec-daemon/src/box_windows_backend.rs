use fabushi_android_shared::ExecutionError;

pub const SAND_BOX_PRIMARY_WINDOW_INDEX: u32 = 1;
pub const SAND_BOX_FORK_ROUTER_PORT: u16 = 1339;
pub const SAND_BOX_DISPLAY_HEADER: &str = "x-sand-display";
pub const SAND_BOX_WINDOW_OWNER_HEADER: &str = "x-sand-window-owner";
pub const SAND_BOX_WINDOW_UNAVAILABLE_EXIT_CODE: i32 = 75;

#[derive(Clone,Debug,PartialEq,Eq)] pub struct RemoteWindowCommand{pub command:String,pub working_directory:String,pub tool_call_id:String,pub skip_approval:bool}
#[derive(Clone,Debug,PartialEq,Eq)] pub struct RemoteWindowCommandOutcome{pub exit_code:i32,pub stderr:String}
pub trait RemoteWindowCommandExecutor{fn execute(&mut self,command:&RemoteWindowCommand)->Result<RemoteWindowCommandOutcome,ExecutionError>;}
#[derive(Clone,Debug,PartialEq,Eq)] pub enum RemoteWindowError{GuardedPrimary,InvalidOwnerToken,NoMonitor(String),Execution(String)}

pub fn is_valid_sand_window_owner_token(value:&str)->bool{let v=value.trim();!v.is_empty()&&v.len()<=128&&v.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'_'|b'-'))}
pub fn sand_box_display_token(index:u32)->String{index.to_string()}
pub fn sand_box_window_key(agent_id:&str,index:u32)->String{format!("{agent_id}#{index}")}

pub fn start_window_command(index:u32,owner_token:Option<&str>)->Result<RemoteWindowCommand,RemoteWindowError>{
 if index==SAND_BOX_PRIMARY_WINDOW_INDEX{return Err(RemoteWindowError::GuardedPrimary);}
 if let Some(token)=owner_token{if !is_valid_sand_window_owner_token(token){return Err(RemoteWindowError::InvalidOwnerToken);}}
 let command=match owner_token{Some(token)=>format!("/usr/local/bin/start-window {index} {token}"),None=>format!("/usr/local/bin/start-window {index}")};
 Ok(RemoteWindowCommand{command,working_directory:"/workspace".into(),tool_call_id:"sand-start-window".into(),skip_approval:true})
}

pub fn run_start_window<E:RemoteWindowCommandExecutor>(executor:&mut E,index:u32,owner_token:Option<&str>)->Result<(),RemoteWindowError>{
 let command=start_window_command(index,owner_token)?;
 let outcome=executor.execute(&command).map_err(|e|RemoteWindowError::Execution(format!("{e:?}")))?;
 if outcome.exit_code==0{return Ok(());} if outcome.exit_code==SAND_BOX_WINDOW_UNAVAILABLE_EXIT_CODE{return Err(RemoteWindowError::NoMonitor(format!("could not claim display :{index}: {}",outcome.stderr.trim())));} Err(RemoteWindowError::Execution(format!("start-window exited {}: {}",outcome.exit_code,outcome.stderr.trim())))
}

pub fn monitor_busy_lease_command(index:u32)->RemoteWindowCommand{RemoteWindowCommand{command:format!("touch /tmp/sand-monitor-busy-{index}"),working_directory:"/workspace".into(),tool_call_id:"sand-monitor-busy-lease".into(),skip_approval:true}}

pub fn parse_positive_env_int(raw:Option<&str>,fallback:u32)->u32{let Some(raw)=raw else{return fallback};let digits=raw.trim_start().chars().take_while(|c|c.is_ascii_digit()).collect::<String>();digits.parse::<u32>().ok().filter(|v|*v>0).unwrap_or(fallback)}

#[cfg(test)] mod tests{use super::*;
struct Exec{calls:Vec<RemoteWindowCommand>,outcome:RemoteWindowCommandOutcome}impl RemoteWindowCommandExecutor for Exec{fn execute(&mut self,c:&RemoteWindowCommand)->Result<RemoteWindowCommandOutcome,ExecutionError>{self.calls.push(c.clone());Ok(self.outcome.clone())}}
#[test]fn builder_preserves_command_owner_and_no_monitor_contract(){let mut e=Exec{calls:vec![],outcome:RemoteWindowCommandOutcome{exit_code:0,stderr:String::new()}};run_start_window(&mut e,2,Some("agent_owner-7")).unwrap();assert_eq!(e.calls[0].command,"/usr/local/bin/start-window 2 agent_owner-7");assert_eq!(e.calls[0].working_directory,"/workspace");assert_eq!(e.calls[0].tool_call_id,"sand-start-window");assert!(e.calls[0].skip_approval);assert!(matches!(start_window_command(1,None),Err(RemoteWindowError::GuardedPrimary)));assert!(matches!(start_window_command(2,Some("bad token")),Err(RemoteWindowError::InvalidOwnerToken)));e.outcome=RemoteWindowCommandOutcome{exit_code:SAND_BOX_WINDOW_UNAVAILABLE_EXIT_CODE,stderr:"busy".into()};assert!(matches!(run_start_window(&mut e,3,None),Err(RemoteWindowError::NoMonitor(s)) if s.contains("display :3")));}
#[test]fn lease_display_and_env_helpers_are_frozen(){assert_eq!(SAND_BOX_FORK_ROUTER_PORT,1339);assert_eq!(SAND_BOX_DISPLAY_HEADER,"x-sand-display");assert_eq!(SAND_BOX_WINDOW_OWNER_HEADER,"x-sand-window-owner");assert_eq!(monitor_busy_lease_command(3).command,"touch /tmp/sand-monitor-busy-3");assert_eq!(sand_box_window_key("agent-a",2),"agent-a#2");assert_eq!(parse_positive_env_int(Some(" 12workers"),100),12);assert_eq!(parse_positive_env_int(Some("0"),100),100);}
}
