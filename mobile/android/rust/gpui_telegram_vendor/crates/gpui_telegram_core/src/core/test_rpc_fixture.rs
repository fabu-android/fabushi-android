//! Telegram Desktop 1:1 Parity Module: `TestRpcFixture`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_rpc_fixture.h, Telegram/SourceFiles/test/test_rpc_fixture.cpp

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// RPC mock fixture for simulating MTProto queries and responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestRpcFixture {
    pub responses: HashMap<String, Vec<u8>>,
    pub sent_queries: Vec<String>,
}

impl TestRpcFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_response(&mut self, method: impl Into<String>, response_bytes: Vec<u8>) {
        self.responses.insert(method.into(), response_bytes);
    }

    pub fn send_query(&mut self, method: &str) -> Option<&[u8]> {
        self.sent_queries.push(method.to_string());
        self.responses.get(method).map(|v| v.as_slice())
    }
}
