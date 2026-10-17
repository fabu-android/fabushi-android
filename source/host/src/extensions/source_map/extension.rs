use crate::extensions::extension_ids_generated::HostExtensionId;

use super::source_map_service::SandSourceMap;

pub const SOURCE_MAP_DEPENDENCIES: &[HostExtensionId] = &[];

pub fn source_map_extension_id() -> HostExtensionId {
    HostExtensionId::SourceMap
}

pub fn start_source_map_extension(path: std::path::PathBuf) -> SandSourceMap {
    SandSourceMap::new(path)
}
