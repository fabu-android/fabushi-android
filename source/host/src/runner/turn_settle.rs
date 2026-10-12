#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnCheckpoint {
    pub operation_id: String,
    pub attempt: usize,
    pub emitted_chunks: usize,
    pub durable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnSettlement {
    pub operation_id: String,
    pub completed: bool,
    pub cancelled: bool,
    pub finish_reason: String,
    pub checkpoint: TurnCheckpoint,
}

pub fn prepare_checkpoint(
    operation_id: &str,
    attempt: usize,
    emitted_chunks: usize,
) -> Result<TurnCheckpoint, &'static str> {
    if operation_id.trim().is_empty() {
        return Err("operation_id is required");
    }
    Ok(TurnCheckpoint {
        operation_id: operation_id.to_string(),
        attempt,
        emitted_chunks,
        durable: false,
    })
}

pub fn persist_checkpoint(mut checkpoint: TurnCheckpoint) -> TurnCheckpoint {
    checkpoint.durable = true;
    checkpoint
}

pub fn settle_completed_turn(
    checkpoint: TurnCheckpoint,
    finish_reason: impl Into<String>,
) -> Result<TurnSettlement, &'static str> {
    if !checkpoint.durable {
        return Err("checkpoint must be durable before terminal settlement");
    }
    Ok(TurnSettlement {
        operation_id: checkpoint.operation_id.clone(),
        completed: true,
        cancelled: false,
        finish_reason: finish_reason.into(),
        checkpoint,
    })
}

pub fn settle_cancelled_turn(
    checkpoint: TurnCheckpoint,
) -> Result<TurnSettlement, &'static str> {
    if !checkpoint.durable {
        return Err("checkpoint must be durable before terminal settlement");
    }
    Ok(TurnSettlement {
        operation_id: checkpoint.operation_id.clone(),
        completed: false,
        cancelled: true,
        finish_reason: "cancelled".into(),
        checkpoint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_settlement_requires_durable_checkpoint() {
        let checkpoint = prepare_checkpoint("op", 1, 2).unwrap();
        assert!(settle_completed_turn(checkpoint.clone(), "stop").is_err());
        let checkpoint = persist_checkpoint(checkpoint);
        let settlement = settle_completed_turn(checkpoint, "stop").unwrap();
        assert!(settlement.completed);
        assert!(settlement.checkpoint.durable);
    }
}
