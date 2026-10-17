#[derive(Clone,Debug,PartialEq,Eq)]
pub struct HostRequestContext { pub request_id:String,pub account_id:Option<String>,pub time_zone:String }

pub fn resolve_time_zone(candidate:Option<&str>)->String{
    candidate.filter(|value|!value.trim().is_empty()).unwrap_or("UTC").to_string()
}
pub fn create_host_request_context(request_id:&str,account_id:Option<&str>,time_zone:Option<&str>)->Result<HostRequestContext,&'static str>{
    if request_id.trim().is_empty(){return Err("request_id is required");}
    Ok(HostRequestContext{request_id:request_id.into(),account_id:account_id.map(str::to_string),time_zone:resolve_time_zone(time_zone)})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_context_preserves_scope_and_defaults_only_empty_timezone() {
        let context = create_host_request_context(
            "request-1",
            Some("account-1"),
            Some(" America/Los_Angeles "),
        )
        .unwrap();
        assert_eq!(context.request_id, "request-1");
        assert_eq!(context.account_id.as_deref(), Some("account-1"));
        assert_eq!(context.time_zone, " America/Los_Angeles ");
    }

    #[test]
    fn request_context_rejects_empty_request() {
        assert_eq!(create_host_request_context("   ", None, None).unwrap_err(), "request_id is required");
        assert_eq!(
            create_host_request_context("request-2", None, Some("   ")).unwrap().time_zone,
            "UTC",
        );
    }
}
