use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteerMessage {
    pub generation: u64,
    pub sequence: u64,
    pub payload: String,
}

#[derive(Clone, Debug)]
pub struct SteerOutbox {
    generation: u64,
    next_sequence: u64,
    queue: VecDeque<SteerMessage>,
}

impl SteerOutbox {
    pub fn new(generation: u64) -> Self {
        Self { generation: generation.max(1), next_sequence: 0, queue: VecDeque::new() }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn push(&mut self, expected_generation: u64, payload: impl Into<String>) -> Result<SteerMessage, &'static str> {
        if expected_generation != self.generation {
            return Err("stale generation");
        }
        self.next_sequence = self.next_sequence.saturating_add(1);
        let message = SteerMessage {
            generation: self.generation,
            sequence: self.next_sequence,
            payload: payload.into(),
        };
        self.queue.push_back(message.clone());
        Ok(message)
    }

    pub fn replay_after(&self, generation: u64, sequence: u64) -> Result<Vec<SteerMessage>, &'static str> {
        if generation != self.generation {
            return Err("stale generation");
        }
        Ok(self.queue.iter().filter(|item| item.sequence > sequence).cloned().collect())
    }

    pub fn reset(&mut self, generation: u64) -> Result<(), &'static str> {
        if generation <= self.generation {
            return Err("new generation must advance");
        }
        self.generation = generation;
        self.next_sequence = 0;
        self.queue.clear();
        Ok(())
    }
}
