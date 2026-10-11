#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteRunnerCli { pub endpoint: String, pub device_id: String }

impl RemoteRunnerCli {
    pub fn parse(args:&[String]) -> Result<Self,&'static str> {
        let mut endpoint=None; let mut device_id=None;
        for arg in args {
            if let Some(v)=arg.strip_prefix("--endpoint="){endpoint=Some(v.to_string());}
            if let Some(v)=arg.strip_prefix("--device-id="){device_id=Some(v.to_string());}
        }
        let endpoint=endpoint.ok_or("missing --endpoint")?;
        let device_id=device_id.ok_or("missing --device-id")?;
        if !(endpoint.starts_with("https://")||endpoint.starts_with("wss://")) { return Err("remote endpoint must use TLS"); }
        if device_id.trim().is_empty() { return Err("device id is required"); }
        Ok(Self { endpoint, device_id })
    }
}
#[cfg(test)]
mod tests { use super::*; #[test] fn rejects_plaintext(){let a=vec!["--endpoint=http://x".into(),"--device-id=d".into()];assert!(RemoteRunnerCli::parse(&a).is_err());} }
