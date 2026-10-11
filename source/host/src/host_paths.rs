use std::path::{Path,PathBuf};

pub const SAND_DATA_DIRNAME:&str="fabushi";
pub const SAND_PRODUCTION_DATA_DIRNAME:&str="production";

pub fn get_sand_root_dir(user_data:&Path)->PathBuf{user_data.join(SAND_DATA_DIRNAME)}
pub fn get_sand_production_root_dir(user_data:&Path)->PathBuf{get_sand_root_dir(user_data).join(SAND_PRODUCTION_DATA_DIRNAME)}
pub fn get_gateway_discovery_path(root:&Path)->PathBuf{root.join("gateway-discovery")}
pub fn get_host_lock_path(root:&Path)->PathBuf{root.join("host.lock")}
pub fn get_host_secrets_path(root:&Path)->PathBuf{root.join("secrets")}
pub fn get_host_crash_marker_path(root:&Path)->PathBuf{root.join("host-crash.marker")}
pub fn reanchor_sand_path(old_root:&Path,new_root:&Path,path:&Path)->Option<PathBuf>{path.strip_prefix(old_root).ok().map(|relative|new_root.join(relative))}
