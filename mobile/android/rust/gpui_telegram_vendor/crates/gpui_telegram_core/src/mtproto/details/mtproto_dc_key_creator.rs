//! MTProto Diffie-Hellman Key Exchange Orchestrator
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_dc_key_creator.h`, `mtproto_dc_key_creator.cpp`

use crate::mtproto::core_types::DcId;
use crate::mtproto::mtproto_auth_key::{AuthKeyType, MtprotoAuthKey};
use crate::mtproto::mtproto_dh_utils::factorize_pq;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KeyCreatorStage {
    #[default]
    None,
    WaitingPQ,
    WaitingDH,
    WaitingDone,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DcKeyResult {
    pub persistent_key: Option<MtprotoAuthKey>,
    pub temporary_key: Option<MtprotoAuthKey>,
    pub temporary_server_salt: u64,
    pub persistent_server_salt: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoDcKeyCreatorDetails {
    pub dc_id: DcId,
    pub stage: KeyCreatorStage,
    pub nonce: [u8; 16],
    pub server_nonce: [u8; 16],
    pub new_nonce: [u8; 32],
    pub pq: u64,
    pub p: u64,
    pub q: u64,
    pub fingerprints: Vec<u64>,
    pub selected_fingerprint: u64,
    pub retries: u32,
}

impl Default for MtprotoDcKeyCreatorDetails {
    fn default() -> Self {
        Self {
            dc_id: 2,
            stage: KeyCreatorStage::None,
            nonce: [0u8; 16],
            server_nonce: [0u8; 16],
            new_nonce: [0u8; 32],
            pq: 0,
            p: 0,
            q: 0,
            fingerprints: Vec::new(),
            selected_fingerprint: 0,
            retries: 0,
        }
    }
}

impl MtprotoDcKeyCreatorDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.stage != KeyCreatorStage::None
    }

    pub fn with_dc_id(dc_id: DcId) -> Self {
        Self {
            dc_id,
            ..Self::default()
        }
    }

    pub fn initiate_handshake(&mut self, nonce: [u8; 16], new_nonce: [u8; 32]) {
        self.nonce = nonce;
        self.new_nonce = new_nonce;
        self.stage = KeyCreatorStage::WaitingPQ;
        self.retries = 0;
    }

    pub fn process_res_pq(&mut self, server_nonce: [u8; 16], pq: u64, fingerprints: Vec<u64>) -> bool {
        self.server_nonce = server_nonce;
        self.pq = pq;
        self.fingerprints = fingerprints.clone();

        if let Some((p, q)) = factorize_pq(pq) {
            self.p = p;
            self.q = q;
            if let Some(&fp) = fingerprints.first() {
                self.selected_fingerprint = fp;
            }
            self.stage = KeyCreatorStage::WaitingDH;
            true
        } else {
            false
        }
    }

    pub fn complete_dh_exchange(&mut self, auth_key_bytes: &[u8], server_salt: u64, is_temp: bool) -> DcKeyResult {
        self.stage = KeyCreatorStage::Ready;
        let key_type = if is_temp {
            AuthKeyType::Temporary
        } else {
            AuthKeyType::Generated
        };
        let key = MtprotoAuthKey::from_key_bytes(auth_key_bytes, self.dc_id, key_type);

        if is_temp {
            DcKeyResult {
                persistent_key: None,
                temporary_key: Some(key),
                temporary_server_salt: server_salt,
                persistent_server_salt: 0,
            }
        } else {
            DcKeyResult {
                persistent_key: Some(key),
                temporary_key: None,
                temporary_server_salt: 0,
                persistent_server_salt: server_salt,
            }
        }
    }
}
