//! MTProto TLS Emulated/Native Socket
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_tls_socket.h`, `mtproto_tls_socket.cpp`

use super::mtproto_tcp_socket::MtprotoTcpSocketDetails;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoTlsSocketDetails {
    pub tcp: MtprotoTcpSocketDetails,
    pub sni_hostname: String,
    pub handshake_complete: bool,
}

impl Default for MtprotoTlsSocketDetails {
    fn default() -> Self {
        Self {
            tcp: MtprotoTcpSocketDetails::default(),
            sni_hostname: "www.google.com".to_string(),
            handshake_complete: false,
        }
    }
}

impl MtprotoTlsSocketDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.tcp.is_active() && self.handshake_complete
    }

    pub fn start_handshake(&mut self, sni: impl Into<String>) {
        self.sni_hostname = sni.into();
        self.handshake_complete = true;
    }

    pub fn is_secure(&self) -> bool {
        self.handshake_complete
    }
}
