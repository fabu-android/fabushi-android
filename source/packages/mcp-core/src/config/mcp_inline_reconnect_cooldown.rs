#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconnectCooldown {
    cooldown_ms: u64,
    last_attempt_ms: Option<u64>,
}

impl ReconnectCooldown {
    pub fn new(cooldown_ms: u64) -> Self {
        Self { cooldown_ms: cooldown_ms.max(1), last_attempt_ms: None }
    }

    pub fn try_begin(&mut self, now_ms: u64) -> bool {
        if self.last_attempt_ms.is_some_and(|last| now_ms < last.saturating_add(self.cooldown_ms)) {
            return false;
        }
        self.last_attempt_ms = Some(now_ms);
        true
    }
}
