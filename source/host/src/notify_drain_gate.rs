pub const NOTIFY_SAFETY_POLL_MS: u64 = 120_000;
pub const NOTIFY_DRAIN_FLOOR_MS: u64 = 4_000;

#[derive(Default)]
pub struct NotifyDrainGate {
    notify_pending: bool,
    last_poll_at_ms: Option<u64>,
    notify_seq: u64,
    drained_notify_seq: u64,
}

impl NotifyDrainGate {
    pub fn record_notify(&mut self) {
        self.notify_pending = true;
        self.notify_seq = self.notify_seq.saturating_add(1);
    }

    pub fn should_drain(
        &mut self,
        now_ms: u64,
        has_owed_work: bool,
        connected: bool,
        safety_poll_enabled: bool,
    ) -> bool {
        self.drained_notify_seq = self.notify_seq;
        if has_owed_work || !connected {
            return true;
        }
        let Some(last_poll_at_ms) = self.last_poll_at_ms else {
            return true;
        };
        let since = now_ms.saturating_sub(last_poll_at_ms);
        if self.notify_pending && since >= NOTIFY_DRAIN_FLOOR_MS {
            return true;
        }
        safety_poll_enabled && since >= NOTIFY_SAFETY_POLL_MS
    }

    pub fn record_poll(&mut self, now_ms: u64) {
        if self.notify_seq == self.drained_notify_seq {
            self.notify_pending = false;
        }
        self.last_poll_at_ms = Some(now_ms);
    }

    pub fn reset(&mut self) {
        self.last_poll_at_ms = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_drain_honors_floor_and_safety_poll() {
        let mut gate = NotifyDrainGate::default();
        assert!(gate.should_drain(0, false, true, true));
        gate.record_poll(100);
        gate.record_notify();
        assert!(!gate.should_drain(100 + NOTIFY_DRAIN_FLOOR_MS - 1, false, true, true));
        assert!(gate.should_drain(100 + NOTIFY_DRAIN_FLOOR_MS, false, true, true));
        gate.record_poll(100 + NOTIFY_DRAIN_FLOOR_MS);
        assert!(gate.should_drain(100 + NOTIFY_DRAIN_FLOOR_MS + NOTIFY_SAFETY_POLL_MS, false, true, true));
    }

    #[test]
    fn disconnected_or_owed_work_drains_immediately() {
        let mut gate = NotifyDrainGate::default();
        gate.record_poll(1);
        assert!(gate.should_drain(2, true, true, false));
        assert!(gate.should_drain(2, false, false, false));
    }
}
