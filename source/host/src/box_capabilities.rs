use std::fmt;

pub trait CapableBox<Ctx> {
    type Description;
    type EnvironmentUpdate;
    type McpLoadResult;
    type McpAccessor;
    type Error;

    fn max_windows(&self) -> Option<u32> { None }
    fn agent_window_index(&self, _agent_id: &str) -> Option<u32> { None }
    fn terminals_folder(&self) -> Option<String> { None }
    fn is_available(&self) -> Option<Result<bool, Self::Error>> { None }
    fn is_preparing(&self, _agent_id: &str) -> Option<bool> { None }
    fn description(&self) -> Option<Self::Description> { None }
    fn apply_environment(&mut self, _ctx: &Ctx, _update: Self::EnvironmentUpdate) -> Option<Result<(), Self::Error>> { None }
    fn load_mcp_servers(&mut self, _ctx: &Ctx, _config_json: &str) -> Option<Result<Self::McpLoadResult, Self::Error>> { None }
    fn mcp_resource_accessor(&mut self, _ctx: &Ctx) -> Option<Result<Self::McpAccessor, Self::Error>> { None }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxCapabilityCallError<E> {
    EnvironmentUnsupported,
    McpUnsupported,
    Inner(E),
}

impl<E: fmt::Display> fmt::Display for BoxCapabilityCallError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvironmentUnsupported => formatter.write_str("Box environment sync is unsupported"),
            Self::McpUnsupported => formatter.write_str("Box MCP capability is unsupported"),
            Self::Inner(error) => error.fmt(formatter),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for BoxCapabilityCallError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self { Self::Inner(error) => Some(error), _ => None }
    }
}

pub fn box_max_windows<Ctx, Box>(box_: &Box) -> u32 where Box: CapableBox<Ctx> { box_.max_windows().unwrap_or(1) }
pub fn box_supports_multi_window<Ctx, Box>(box_: &Box) -> bool where Box: CapableBox<Ctx> { box_max_windows::<Ctx, Box>(box_) > 1 }
pub fn box_agent_window_index<Ctx, Box>(box_: &Box, agent_id: &str) -> Option<u32> where Box: CapableBox<Ctx> { box_.agent_window_index(agent_id) }
pub fn box_terminals_folder<Ctx, Box>(box_: &Box) -> Option<String> where Box: CapableBox<Ctx> { box_.terminals_folder() }
pub fn box_is_available<Ctx, Box>(box_: &Box) -> Result<bool, Box::Error> where Box: CapableBox<Ctx> { match box_.is_available() { Some(result) => result, None => Ok(true) } }
pub fn box_is_preparing<Ctx, Box>(box_: &Box, agent_id: &str) -> bool where Box: CapableBox<Ctx> { box_.is_preparing(agent_id).unwrap_or(false) }
pub fn box_description<Ctx, Box>(box_: &Box) -> Option<Box::Description> where Box: CapableBox<Ctx> { box_.description() }
pub fn box_apply_environment<Ctx, Box>(box_: &mut Box, ctx: &Ctx, update: Box::EnvironmentUpdate) -> Result<(), BoxCapabilityCallError<Box::Error>> where Box: CapableBox<Ctx> { match box_.apply_environment(ctx, update) { Some(result) => result.map_err(BoxCapabilityCallError::Inner), None => Err(BoxCapabilityCallError::EnvironmentUnsupported) } }
pub fn box_load_mcp_servers<Ctx, Box>(box_: &mut Box, ctx: &Ctx, config_json: &str) -> Result<Box::McpLoadResult, BoxCapabilityCallError<Box::Error>> where Box: CapableBox<Ctx> { match box_.load_mcp_servers(ctx, config_json) { Some(result) => result.map_err(BoxCapabilityCallError::Inner), None => Err(BoxCapabilityCallError::McpUnsupported) } }
pub fn box_mcp_resource_accessor<Ctx, Box>(box_: &mut Box, ctx: &Ctx) -> Result<Box::McpAccessor, BoxCapabilityCallError<Box::Error>> where Box: CapableBox<Ctx> { match box_.mcp_resource_accessor(ctx) { Some(result) => result.map_err(BoxCapabilityCallError::Inner), None => Err(BoxCapabilityCallError::McpUnsupported) } }
