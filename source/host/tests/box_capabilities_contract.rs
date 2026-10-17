use std::io;
use fabushi_mahayana_host::box_capabilities::{BoxCapabilityCallError, CapableBox, box_agent_window_index, box_apply_environment, box_description, box_is_available, box_is_preparing, box_load_mcp_servers, box_max_windows, box_mcp_resource_accessor, box_supports_multi_window, box_terminals_folder};

#[derive(Default)] struct MinimalBox;
impl CapableBox<String> for MinimalBox { type Description=String; type EnvironmentUpdate=String; type McpLoadResult=Vec<String>; type McpAccessor=String; type Error=io::Error; }
struct FullBox { applied: Vec<String>, mcp_configs: Vec<String> }
impl CapableBox<String> for FullBox {
 type Description=String; type EnvironmentUpdate=String; type McpLoadResult=Vec<String>; type McpAccessor=String; type Error=io::Error;
 fn max_windows(&self)->Option<u32>{Some(8)}
 fn agent_window_index(&self, agent_id:&str)->Option<u32>{(agent_id=="agent-7").then_some(3)}
 fn terminals_folder(&self)->Option<String>{Some("/terminals".into())}
 fn is_available(&self)->Option<Result<bool,Self::Error>>{Some(Ok(false))}
 fn is_preparing(&self,agent_id:&str)->Option<bool>{Some(agent_id=="agent-7")}
 fn description(&self)->Option<Self::Description>{Some("loopback".into())}
 fn apply_environment(&mut self,_:&String,update:String)->Option<Result<(),Self::Error>>{self.applied.push(update);Some(Ok(()))}
 fn load_mcp_servers(&mut self,_:&String,config_json:&str)->Option<Result<Self::McpLoadResult,Self::Error>>{self.mcp_configs.push(config_json.into());Some(Ok(vec!["filesystem".into()]))}
 fn mcp_resource_accessor(&mut self,_:&String)->Option<Result<Self::McpAccessor,Self::Error>>{Some(Ok("mcp-accessor".into()))}
}

#[test]
fn box_capabilities_preserve_desktop_defaults_and_fail_closed_for_optional_features(){
 let mut b=MinimalBox;
 assert_eq!(box_max_windows::<String,_>(&b),1); assert!(!box_supports_multi_window::<String,_>(&b)); assert_eq!(box_agent_window_index::<String,_>(&b,"agent"),None); assert_eq!(box_terminals_folder::<String,_>(&b),None); assert!(box_is_available::<String,_>(&b).unwrap()); assert!(!box_is_preparing::<String,_>(&b,"agent")); assert_eq!(box_description::<String,_>(&b),None);
 assert!(matches!(box_apply_environment(&mut b,&"ctx".into(),"A=1".into()),Err(BoxCapabilityCallError::EnvironmentUnsupported)));
 assert!(matches!(box_load_mcp_servers(&mut b,&"ctx".into(),"{}"),Err(BoxCapabilityCallError::McpUnsupported)));
 assert!(matches!(box_mcp_resource_accessor(&mut b,&"ctx".into()),Err(BoxCapabilityCallError::McpUnsupported)));
}

#[test]
fn box_capabilities_forward_present_features_without_fallbacks(){
 let mut b=FullBox{applied:vec![],mcp_configs:vec![]}; assert_eq!(box_max_windows::<String,_>(&b),8); assert!(box_supports_multi_window::<String,_>(&b)); assert_eq!(box_agent_window_index::<String,_>(&b,"agent-7"),Some(3)); assert_eq!(box_terminals_folder::<String,_>(&b).as_deref(),Some("/terminals")); assert!(!box_is_available::<String,_>(&b).unwrap()); assert!(box_is_preparing::<String,_>(&b,"agent-7")); assert_eq!(box_description::<String,_>(&b).as_deref(),Some("loopback"));
 box_apply_environment(&mut b,&"ctx".into(),"A=1".into()).unwrap(); assert_eq!(b.applied,vec!["A=1"]);
 assert_eq!(box_load_mcp_servers(&mut b,&"ctx".into(),r#"{"mcp":true}"#).unwrap(),vec!["filesystem"]); assert_eq!(b.mcp_configs,vec![r#"{"mcp":true}"#]); assert_eq!(box_mcp_resource_accessor(&mut b,&"ctx".into()).unwrap(),"mcp-accessor");
}

#[test]
fn shipping_remote_box_path_has_a_real_capability_availability_callsite(){
 let source=include_str!("../src/host_runner_composition.rs");
 assert!(source.contains("box_is_available::<RemoteExecutionContext, _>(&self.remote)"));
 assert!(source.contains("impl<T: RemoteExecutionTransport> CapableBox<RemoteExecutionContext>"));
}
