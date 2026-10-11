#[derive(Clone,Debug,PartialEq,Eq)]
pub struct HostRequestContext { pub request_id:String,pub account_id:Option<String>,pub time_zone:String }

pub fn resolve_time_zone(candidate:Option<&str>)->String{
    candidate.filter(|value|!value.trim().is_empty()).unwrap_or("UTC").to_string()
}
pub fn create_host_request_context(request_id:&str,account_id:Option<&str>,time_zone:Option<&str>)->Result<HostRequestContext,&'static str>{
    if request_id.trim().is_empty(){return Err("request_id is required");}
    Ok(HostRequestContext{request_id:request_id.into(),account_id:account_id.map(str::to_string),time_zone:resolve_time_zone(time_zone)})
}
