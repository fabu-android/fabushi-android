use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
    thread,
    time::{Duration, SystemTime},
};

pub type ChangeListener = Arc<dyn Fn() + Send + Sync + 'static>;

struct WatchedDirectoryInner {
    root: PathBuf,
    debounce_ms: u64,
    listener: Mutex<Option<ChangeListener>>,
    generation: AtomicU64,
    snapshot: Mutex<BTreeMap<PathBuf, SystemTime>>,
}

/// Android app-private directory owner.
///
/// Desktop watches arbitrary filesystem mutations with a platform watcher. Android production
/// keeps canonical Host state in app-private storage, so callers mutate it through this owner.
/// We preserve the same root-bound/listener/atomic-write contract without adding a second Android
/// filesystem watcher, while retaining the legacy explicit scan API for existing adapters.
#[derive(Clone)]
pub struct WatchedDirectory {
    inner: Arc<WatchedDirectoryInner>,
}

impl Default for WatchedDirectory {
    fn default() -> Self {
        Self::new(PathBuf::new(), 0)
    }
}

impl std::fmt::Debug for WatchedDirectory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WatchedDirectory")
            .field("root", &self.inner.root)
            .field("debounce_ms", &self.inner.debounce_ms)
            .finish_non_exhaustive()
    }
}

impl WatchedDirectory {
    pub fn new(root: impl Into<PathBuf>, debounce_ms: u64) -> Self {
        Self {
            inner: Arc::new(WatchedDirectoryInner {
                root: root.into(),
                debounce_ms,
                listener: Mutex::new(None),
                generation: AtomicU64::new(0),
                snapshot: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    pub fn get_location(&self) -> &Path {
        &self.inner.root
    }

    pub fn list_subdirectory_names(&self) -> Vec<String> {
        let mut names = fs::read_dir(&self.inner.root)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    pub fn set_on_change(&self, listener: Option<ChangeListener>) -> Result<(), String> {
        *self
            .inner
            .listener
            .lock()
            .map_err(|_| "watched-directory listener lock poisoned".to_string())? = listener;
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    pub fn schedule_notify(&self) {
        schedule_notify(&Arc::downgrade(&self.inner));
    }

    pub fn write_file_atomic(&self, path: impl AsRef<Path>, contents: &[u8]) -> io::Result<()> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "watched file has no parent"))?;
        fs::create_dir_all(parent)?;
        let filename = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "watched file has no name"))?
            .to_string_lossy();
        let temporary = parent.join(format!(".{filename}.{}.tmp", std::process::id()));
        let result = (|| -> io::Result<()> {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(contents)?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            if let Ok(directory) = fs::File::open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return result;
        }
        self.schedule_notify();
        Ok(())
    }

    pub fn scan(&mut self, root: &Path) -> io::Result<Vec<PathBuf>> {
        let mut next = BTreeMap::new();
        if root.exists() {
            for entry in fs::read_dir(root)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let modified = entry.metadata()?.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    next.insert(path, modified);
                }
            }
        }
        let mut snapshot = self
            .inner
            .snapshot
            .lock()
            .map_err(|_| io::Error::other("watched-directory snapshot lock poisoned"))?;
        let changed = next
            .iter()
            .filter_map(|(path, modified)| {
                if snapshot.get(path) != Some(modified) {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .chain(snapshot.keys().filter(|path| !next.contains_key(*path)).cloned())
            .collect();
        *snapshot = next;
        Ok(changed)
    }
}

fn schedule_notify(weak: &Weak<WatchedDirectoryInner>) {
    let Some(inner) = weak.upgrade() else {
        return;
    };
    let generation = inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
    let debounce_ms = inner.debounce_ms;
    let weak = Arc::downgrade(&inner);
    thread::spawn(move || {
        if debounce_ms > 0 {
            thread::sleep(Duration::from_millis(debounce_ms));
        }
        let Some(inner) = weak.upgrade() else {
            return;
        };
        if inner.generation.load(Ordering::Acquire) != generation {
            return;
        }
        let listener = inner
            .listener
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(listener) = listener {
            listener();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn atomic_write_notifies_and_leaves_no_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let watched = WatchedDirectory::new(root.path(), 0);
        let (tx, rx) = mpsc::channel();
        watched
            .set_on_change(Some(Arc::new(move || {
                let _ = tx.send(());
            })))
            .unwrap();

        let path = root.path().join("memory").join("profile.md");
        watched.write_file_atomic(&path, b"first").unwrap();

        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
        assert!(
            fs::read_dir(path.parent().unwrap())
                .unwrap()
                .all(|entry| !entry.unwrap().file_name().to_string_lossy().ends_with(".tmp"))
        );
    }

    #[test]
    fn legacy_scan_detects_added_and_removed_files() {
        let root = tempfile::tempdir().unwrap();
        let mut watched = WatchedDirectory::default();
        assert!(watched.scan(root.path()).unwrap().is_empty());

        let path = root.path().join("a.txt");
        fs::write(&path, "a").unwrap();
        assert_eq!(watched.scan(root.path()).unwrap(), vec![path.clone()]);
        assert!(watched.scan(root.path()).unwrap().is_empty());

        fs::remove_file(&path).unwrap();
        assert_eq!(watched.scan(root.path()).unwrap(), vec![path]);
    }
}
