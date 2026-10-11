pub const ROSTER_REPLICA_KEY: &str = "roster";
pub const ORDERED_REPLICAS_V1: &str = "orderedReplicasV1";

pub fn transcript_replica_key(agent_id: &str) -> String {
    format!("transcript:{agent_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transcript_replica_is_agent_scoped() {
        assert_eq!(transcript_replica_key("a-1"), "transcript:a-1");
    }
}
