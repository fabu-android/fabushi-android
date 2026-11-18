//! Web Proxy Frame Serialization & Deserialization
//! Upstream Source: `Telegram/SourceFiles/mtproto/web_proxy/web_proxy_frame.h`, `web_proxy_frame.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameOpcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xa,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebProxyFrameWebProxy {
    pub opcode: u8,
    pub fin: bool,
    pub payload: Vec<u8>,
}

impl Default for WebProxyFrameWebProxy {
    fn default() -> Self {
        Self {
            opcode: FrameOpcode::Binary as u8,
            fin: true,
            payload: Vec::new(),
        }
    }
}

impl WebProxyFrameWebProxy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        !self.payload.is_empty()
    }

    pub fn binary(payload: Vec<u8>) -> Self {
        Self {
            opcode: FrameOpcode::Binary as u8,
            fin: true,
            payload,
        }
    }

    pub fn ping() -> Self {
        Self {
            opcode: FrameOpcode::Ping as u8,
            fin: true,
            payload: Vec::new(),
        }
    }

    pub fn pong(echo: Vec<u8>) -> Self {
        Self {
            opcode: FrameOpcode::Pong as u8,
            fin: true,
            payload: echo,
        }
    }
}
