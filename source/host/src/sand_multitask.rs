pub const EXECUTOR_SUBAGENT_TYPE:&str="executor";
pub fn resolve_multitask_enabled(configured:Option<bool>,available_processors:usize)->bool{configured.unwrap_or(available_processors>1)}
pub fn sand_multitask_prompt_section(enabled:bool)->&'static str{if enabled{"Parallel task execution is available."}else{""}}
