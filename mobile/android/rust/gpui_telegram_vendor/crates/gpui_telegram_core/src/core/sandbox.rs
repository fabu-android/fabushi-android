//! Telegram Desktop Environment Sandbox & Single Instance Coordination
//! Upstream sources: `Telegram/SourceFiles/core/sandbox.h`, `sandbox.cpp`


/// Sandbox environment managing process locking, event loop nesting, and postponed invocations.
#[derive(Default)]
pub struct Sandbox {
    pub local_server_name: String,
    pub is_primary_instance: bool,
    pub event_nesting_level: i32,
    pub loop_nesting_level: i32,
    pub postponed_calls: Vec<Box<dyn FnOnce() + Send>>,
    pub postponed_calls_deferred: bool,
    pub quit_when_started: bool,
}

impl Sandbox {
    pub fn new(instance_hash: impl Into<String>) -> Self {
        let hash = instance_hash.into();
        Self {
            local_server_name: format!("tdesktop_{}_lock", hash),
            is_primary_instance: true,
            event_nesting_level: 0,
            loop_nesting_level: 0,
            postponed_calls: Vec::new(),
            postponed_calls_deferred: false,
            quit_when_started: false,
        }
    }

    pub fn start(&mut self) -> Result<bool, String> {
        if self.quit_when_started {
            return Ok(false);
        }
        self.is_primary_instance = true;
        Ok(true)
    }

    pub fn postpone_call<F>(&mut self, callable: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.postponed_calls.push(Box::new(callable));
    }

    pub fn drain_postponed_calls(&mut self) {
        let calls = std::mem::take(&mut self.postponed_calls);
        for call in calls {
            call();
        }
    }

    pub fn set_postponed_calls_deferred(&mut self, deferred: bool) {
        self.postponed_calls_deferred = deferred;
    }

    pub fn increment_event_nesting(&mut self) {
        self.event_nesting_level = self.event_nesting_level.saturating_add(1);
    }

    pub fn decrement_event_nesting(&mut self) {
        self.event_nesting_level = self.event_nesting_level.saturating_sub(1);
        if self.event_nesting_level == 0 && !self.postponed_calls_deferred {
            self.drain_postponed_calls();
        }
    }

    pub fn is_single_instance(&self) -> bool {
        self.is_primary_instance
    }
}
