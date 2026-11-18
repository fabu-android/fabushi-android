//! Telegram Desktop 1:1 Parity Module: `TestScenario`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_scenario.cpp

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestScenarioStep {
    pub name: String,
    pub executed: bool,
}

/// Scenario descriptor holding named execution checkpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestScenario {
    pub name: String,
    pub steps: Vec<TestScenarioStep>,
}

impl Default for TestScenario {
    fn default() -> Self {
        Self {
            name: "default_scenario".to_string(),
            steps: Vec::new(),
        }
    }
}

impl TestScenario {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    pub fn add_step(&mut self, step_name: impl Into<String>) {
        self.steps.push(TestScenarioStep {
            name: step_name.into(),
            executed: false,
        });
    }

    pub fn complete_step(&mut self, index: usize) {
        if let Some(step) = self.steps.get_mut(index) {
            step.executed = true;
        }
    }

    pub fn is_finished(&self) -> bool {
        !self.steps.is_empty() && self.steps.iter().all(|s| s.executed)
    }
}
