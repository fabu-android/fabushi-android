pub mod host_extensions;
pub mod scheduling;

// Small Android-local runtime primitives that do not own product state.

#[derive(Debug, Default)]
pub struct MonotonicSequence { value:u64 }

impl MonotonicSequence {
    pub fn from_value(value:u64)->Self{Self{value}}
    pub fn current(&self)->u64{self.value}
    pub fn next_value(&mut self)->u64{self.value=self.value.saturating_add(1);self.value}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn sequence_never_moves_backwards(){
        let mut sequence=MonotonicSequence::default();
        assert_eq!(sequence.next_value(),1);
        assert_eq!(sequence.next_value(),2);
        assert_eq!(sequence.current(),2);
        let mut restored=MonotonicSequence::from_value(41);
        assert_eq!(restored.current(),41);
        assert_eq!(restored.next_value(),42);
    }
}
