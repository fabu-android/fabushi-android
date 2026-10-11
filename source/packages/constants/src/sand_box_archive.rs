pub const SAND_BOX_PERSIST_ARCHIVE_EXCLUDES: &[&str] = &[
    "home/box/chrome-profile/*/Cache",
    "home/box/chrome-profile/*/Code Cache",
    "home/box/chrome-profile/*/GPUCache",
    "home/box/chrome-profile/*/Service Worker/CacheStorage",
];

pub const SAND_WORKSPACE_IGNORE_FILE_NAME: &str = ".sandignore";

pub const SAND_BOX_WORKSPACE_DEFAULT_IGNORE_PATTERNS: &[&str] = &[
    "node_modules/", ".next/", ".nuxt/", ".svelte-kit/", ".turbo/", ".parcel-cache/",
    ".cache/", "dist/", "build/", "out/", "coverage/", "__pycache__/", "*.pyc", "*.pyo",
    ".venv/", "venv/", ".pytest_cache/", ".mypy_cache/", ".ruff_cache/", ".tox/",
    ".ipynb_checkpoints/", "*.egg-info/", ".eggs/", "target/", ".gradle/", "core.[0-9]*", "*.core",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expensive_cache_and_build_directories_are_ignored() {
        assert!(SAND_BOX_PERSIST_ARCHIVE_EXCLUDES.iter().any(|value| value.contains("Cache")));
        assert!(SAND_BOX_WORKSPACE_DEFAULT_IGNORE_PATTERNS.contains(&"node_modules/"));
        assert!(SAND_BOX_WORKSPACE_DEFAULT_IGNORE_PATTERNS.contains(&"target/"));
    }
}
