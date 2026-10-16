use crate::{gateway_config::GatewayServerConfig,host_lock::HostLock,sand_host::SandHost};
use std::{io,path::Path};

pub const SHUTDOWN_WATCHDOG_MS:u64=10_000;

pub struct HostMain {
    pub lock:HostLock,
    pub gateway:GatewayServerConfig,
    pub host:SandHost,
}
impl HostMain {
    pub fn start(lock_path:&Path,gateway:GatewayServerConfig)->io::Result<Self>{
        let lock=HostLock::acquire(lock_path)?;
        Ok(Self{lock,gateway,host:SandHost::new()})
    }
}
