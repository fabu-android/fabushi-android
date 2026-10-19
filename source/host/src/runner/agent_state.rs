#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateWriteResult<T> {
    Ok { detail: T },
    Failed { reason: String },
}

pub fn state_write_ok<T>(detail: T) -> StateWriteResult<T> {
    StateWriteResult::Ok { detail }
}

pub fn state_write_failed<T>(reason: impl Into<String>) -> StateWriteResult<T> {
    StateWriteResult::Failed {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_write_ok_preserves_detail() {
        assert_eq!(
            state_write_ok("saved"),
            StateWriteResult::Ok { detail: "saved" }
        );
    }

    #[test]
    fn state_write_failed_preserves_reason() {
        assert_eq!(
            state_write_failed::<()>("persist failed"),
            StateWriteResult::Failed {
                reason: "persist failed".into(),
            }
        );
    }
}
