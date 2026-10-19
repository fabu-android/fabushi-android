use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Debug, PartialEq, Eq)]
pub enum Attempt<T> {
    Ok(T),
    Panic,
}

pub fn attempt_sync<T>(operation: impl FnOnce() -> T) -> Attempt<T> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(value) => Attempt::Ok(value),
        Err(_) => Attempt::Panic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_success_and_failure_without_unwinding_caller() {
        assert_eq!(attempt_sync(|| 42), Attempt::Ok(42));
        assert_eq!(attempt_sync(|| panic!("boom")), Attempt::<()>::Panic);
    }
}
