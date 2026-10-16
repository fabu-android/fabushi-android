use std::collections::BTreeMap;
use std::sync::Mutex;

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaStamp {
    pub replica_key: String,
    pub epoch: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaSnapshot<T, C = String> {
    pub replica_key: String,
    pub epoch: String,
    pub through_sequence: u64,
    pub coverage: C,
    pub value: T,
}

#[derive(Debug)]
pub struct HostReplicaWriter {
    epoch: String,
    sequences: Mutex<BTreeMap<String, u64>>,
}

impl Default for HostReplicaWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl HostReplicaWriter {
    pub fn new() -> Self {
        Self::with_epoch(Uuid::new_v4().to_string())
    }

    pub fn with_epoch(epoch: impl Into<String>) -> Self {
        Self {
            epoch: epoch.into(),
            sequences: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn process_epoch(&self) -> &str {
        &self.epoch
    }

    pub fn next_stamp(&self, replica_key: &str) -> ReplicaStamp {
        let mut sequences = self
            .sequences
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let sequence = sequences
            .get(replica_key)
            .copied()
            .unwrap_or_default()
            .saturating_add(1);
        sequences.insert(replica_key.to_string(), sequence);
        ReplicaStamp {
            replica_key: replica_key.to_string(),
            epoch: self.epoch.clone(),
            sequence,
        }
    }

    pub fn last_sequence(&self, replica_key: &str) -> u64 {
        self.sequences
            .lock()
            .map(|sequences| sequences.get(replica_key).copied().unwrap_or_default())
            .unwrap_or_default()
    }

    pub fn capture_snapshot<T, C>(
        &self,
        replica_key: &str,
        coverage: C,
        value: T,
    ) -> ReplicaSnapshot<T, C> {
        ReplicaSnapshot {
            replica_key: replica_key.to_string(),
            epoch: self.epoch.clone(),
            through_sequence: self.last_sequence(replica_key),
            coverage,
            value,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_monotonic_per_replica_and_share_process_epoch() {
        let writer = HostReplicaWriter::with_epoch("epoch-a");
        let a1 = writer.next_stamp("agent:a");
        let b1 = writer.next_stamp("agent:b");
        let a2 = writer.next_stamp("agent:a");
        assert_eq!(a1.sequence, 1);
        assert_eq!(b1.sequence, 1);
        assert_eq!(a2.sequence, 2);
        assert_eq!(a1.epoch, "epoch-a");
        assert_eq!(b1.epoch, "epoch-a");
        assert_eq!(writer.last_sequence("agent:a"), 2);
        assert_eq!(writer.last_sequence("missing"), 0);
    }

    #[test]
    fn snapshot_captures_current_coverage_without_advancing_sequence() {
        let writer = HostReplicaWriter::with_epoch("epoch-b");
        writer.next_stamp("conversation:1");
        writer.next_stamp("conversation:1");
        let snapshot = writer.capture_snapshot(
            "conversation:1",
            "complete",
            vec!["m1".to_string(), "m2".to_string()],
        );
        assert_eq!(snapshot.replica_key, "conversation:1");
        assert_eq!(snapshot.epoch, "epoch-b");
        assert_eq!(snapshot.through_sequence, 2);
        assert_eq!(snapshot.coverage, "complete");
        assert_eq!(snapshot.value, vec!["m1".to_string(), "m2".to_string()]);
        assert_eq!(writer.last_sequence("conversation:1"), 2);
    }
}
