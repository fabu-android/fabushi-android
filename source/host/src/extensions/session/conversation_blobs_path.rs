use std::path::{Path, PathBuf};

use super::session_path_contracts::CONVERSATION_BLOBS_FILENAME;

pub fn conversation_blobs_path(db_path: impl AsRef<Path>) -> PathBuf {
    db_path
        .as_ref()
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(CONVERSATION_BLOBS_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_database_stays_next_to_the_canonical_session_store() {
        assert_eq!(
            conversation_blobs_path(Path::new("/data/agents/a/store.db")),
            PathBuf::from("/data/agents/a/conversation-blobs.db")
        );
        assert_eq!(
            conversation_blobs_path(Path::new("store.db")),
            PathBuf::from("conversation-blobs.db")
        );
    }
}
