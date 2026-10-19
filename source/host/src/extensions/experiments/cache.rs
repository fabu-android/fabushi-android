use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const BOOTSTRAP_CACHE_FILENAME: &str = "sand-statsig-bootstrap.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedStatsigBootstrap {
    pub config: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub fetched_at_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct StatsigBootstrapCache {
    path: PathBuf,
}

impl StatsigBootstrapCache {
    pub fn new(cache_dir: impl AsRef<Path>) -> Self {
        Self {
            path: cache_dir.as_ref().join(BOOTSTRAP_CACHE_FILENAME),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Option<CachedStatsigBootstrap> {
        let bytes = fs::read(&self.path).ok()?;
        let cached: CachedStatsigBootstrap = serde_json::from_slice(&bytes).ok()?;
        if cached.config.trim().is_empty() {
            return None;
        }
        Some(cached)
    }

    pub fn load_fresh(&self, now_ms: u64, max_age_ms: u64) -> Option<CachedStatsigBootstrap> {
        let cached = self.load()?;
        let fetched_at_ms = cached.fetched_at_ms?;
        if fetched_at_ms > now_ms || now_ms.saturating_sub(fetched_at_ms) > max_age_ms {
            return None;
        }
        Some(cached)
    }

    pub fn save(&self, cached: &CachedStatsigBootstrap) -> io::Result<()> {
        if cached.config.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Statsig bootstrap config is empty",
            ));
        }
        let parent = self.path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Statsig bootstrap cache path has no parent",
            )
        })?;
        fs::create_dir_all(parent)?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        let result = (|| -> io::Result<()> {
            let encoded = serde_json::to_vec(cached)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&encoded)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-statsig-cache-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn cached(fetched_at_ms: Option<u64>) -> CachedStatsigBootstrap {
        CachedStatsigBootstrap {
            config: r#"{"user":{"userID":"account-a"}}"#.into(),
            user_id: Some("account-a".into()),
            fetched_at_ms,
        }
    }

    #[test]
    fn cache_round_trip_is_atomic_and_leaves_no_temp_file() {
        let root = root("atomic");
        let cache = StatsigBootstrapCache::new(&root);
        cache.save(&cached(Some(100))).unwrap();
        assert_eq!(cache.load(), Some(cached(Some(100))));
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .all(|entry| !entry.unwrap().file_name().to_string_lossy().contains("tmp-"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fresh_load_rejects_missing_future_stale_and_corrupt_cache() {
        let root = root("freshness");
        let cache = StatsigBootstrapCache::new(&root);

        cache.save(&cached(None)).unwrap();
        assert!(cache.load_fresh(1_000, 500).is_none());

        cache.save(&cached(Some(1_001))).unwrap();
        assert!(cache.load_fresh(1_000, 500).is_none());

        cache.save(&cached(Some(100))).unwrap();
        assert!(cache.load_fresh(1_000, 500).is_none());

        cache.save(&cached(Some(750))).unwrap();
        assert_eq!(cache.load_fresh(1_000, 500), Some(cached(Some(750))));

        fs::write(cache.path(), b"{not-json").unwrap();
        assert!(cache.load().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn empty_config_is_never_persisted() {
        let root = root("empty");
        let cache = StatsigBootstrapCache::new(&root);
        let mut value = cached(Some(10));
        value.config.clear();
        assert!(cache.save(&value).is_err());
        assert!(!cache.path().exists());
        let _ = fs::remove_dir_all(root);
    }
}
