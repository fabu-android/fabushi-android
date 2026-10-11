pub fn sand_quiet_work_origin_key(agent_id:&str,operation_id:&str)->Result<String,&'static str>{
    if agent_id.is_empty()||operation_id.is_empty(){return Err("agent and operation ids are required");}
    Ok(format!("{}:{}",agent_id,operation_id))
}
