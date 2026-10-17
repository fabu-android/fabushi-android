use std::path::{Path,PathBuf};

pub const SAND_DATA_DIRNAME:&str="fabushi";
pub const SAND_PRODUCTION_DATA_DIRNAME:&str="production";

pub fn get_sand_root_dir(user_data:&Path)->PathBuf{user_data.join(SAND_DATA_DIRNAME)}
pub fn get_sand_production_root_dir(user_data:&Path)->PathBuf{get_sand_root_dir(user_data).join(SAND_PRODUCTION_DATA_DIRNAME)}
pub fn get_gateway_discovery_path(root:&Path)->PathBuf{root.join("gateway-discovery")}
pub fn get_host_lock_path(root:&Path)->PathBuf{root.join("host.lock")}
pub fn get_host_secrets_path(root:&Path)->PathBuf{root.join("secrets")}
pub fn get_host_crash_marker_path(root:&Path)->PathBuf{root.join("host-crash.marker")}

pub fn get_android_host_runtime_root(app_data_dir:&Path)->PathBuf{
    app_data_dir.join("host-runtime")
}

pub fn get_android_host_lock_path(app_data_dir:&Path)->PathBuf{
    get_android_host_runtime_root(app_data_dir).join("host.lock")
}

pub fn get_android_native_discovery_path(app_data_dir:&Path)->PathBuf{
    get_android_host_runtime_root(app_data_dir).join("native-runtime-discovery.json")
}

pub fn get_android_invariant_report_path(app_data_dir:&Path)->PathBuf{
    get_android_host_runtime_root(app_data_dir).join("invariant-violations.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_host_paths_are_rooted_under_app_private_runtime() {
        let root = Path::new("/data/user/0/com.ombhrum.fabushi/files");
        assert_eq!(get_android_host_runtime_root(root), root.join("host-runtime"));
        assert_eq!(get_android_host_lock_path(root), root.join("host-runtime/host.lock"));
        assert_eq!(get_android_native_discovery_path(root), root.join("host-runtime/native-runtime-discovery.json"));
        assert_eq!(get_android_invariant_report_path(root), root.join("host-runtime/invariant-violations.jsonl"));
    }
}
pub fn reanchor_sand_path(old_root:&Path,new_root:&Path,path:&Path)->Option<PathBuf>{path.strip_prefix(old_root).ok().map(|relative|new_root.join(relative))}
