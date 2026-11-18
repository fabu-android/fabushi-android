use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedPreKeyDto {
    pub key_id: u32,
    pub public_key: String,
    pub signature: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OneTimePreKeyDto {
    pub key_id: u32,
    pub public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishPreKeysRequest {
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// X25519 identity public key (32 bytes, base64).
    pub identity_key: String,
    /// Ed25519 identity signing verifying key (32 bytes, base64).
    pub identity_signing_key: String,
    pub signed_prekey: SignedPreKeyDto,
    #[serde(default)]
    pub one_time_prekeys: Vec<OneTimePreKeyDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishPreKeysResponse {
    pub success: bool,
    pub user_id: u64,
    pub device_id: String,
    pub remaining_one_time_prekeys: usize,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevicePreKeyBundle {
    pub user_id: u64,
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    pub identity_key: String,
    pub identity_signing_key: String,
    pub signed_prekey: SignedPreKeyDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub one_time_prekey: Option<OneTimePreKeyDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPreKeyBundlesResponse {
    pub user_id: u64,
    pub devices: Vec<DevicePreKeyBundle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreKeyCountResponse {
    pub user_id: u64,
    pub device_id: String,
    pub remaining_one_time_prekeys: usize,
    pub has_signed_prekey: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatchetHeaderDto {
    pub dh_pub: String,
    pub prev_chain_len: u32,
    pub msg_num: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerDeviceCiphertext {
    pub recipient_user_id: u64,
    pub recipient_device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_signed_prekey_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_one_time_prekey_id: Option<u32>,
    pub header: RatchetHeaderDto,
    pub ciphertext: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SenderKeyDistributionMessage {
    pub chat_id: i64,
    pub sender_user_id: u64,
    pub sender_device_id: String,
    pub chain_id: u32,
    pub iteration: u32,
    pub chain_key: String,
    pub signing_public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupCiphertext {
    pub chat_id: i64,
    pub chain_id: u32,
    pub iteration: u32,
    pub ciphertext: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum EncryptedPayloadKind {
    PairwiseFanout {
        recipients: Vec<PerDeviceCiphertext>,
    },
    SenderKeyDistribution {
        recipients: Vec<PerDeviceCiphertext>,
    },
    GroupSenderKey(GroupCiphertext),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    pub version: u16,
    pub sender_user_id: u64,
    pub sender_device_id: String,
    pub sender_identity_key: String,
    pub sender_signing_key: String,
    pub kind: EncryptedPayloadKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageContent;

    #[test]
    fn e2ee_dtos_and_message_content_roundtrip() {
        let req = PublishPreKeysRequest {
            device_id: "dev_1".into(),
            device_name: Some("MacBook".into()),
            identity_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            identity_signing_key: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=".into(),
            signed_prekey: SignedPreKeyDto {
                key_id: 1,
                public_key: "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC=".into(),
                signature: "DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD==".into(),
                created_at_ms: 1700000000000,
            },
            one_time_prekeys: vec![OneTimePreKeyDto {
                key_id: 10,
                public_key: "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE=".into(),
            }],
        };
        let json = serde_json::to_string(&req).unwrap();
        let dec: PublishPreKeysRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, dec);

        let env = EncryptedEnvelope {
            version: 1,
            sender_user_id: 42,
            sender_device_id: "dev_1".into(),
            sender_identity_key: "ik".into(),
            sender_signing_key: "sk".into(),
            kind: EncryptedPayloadKind::PairwiseFanout {
                recipients: vec![PerDeviceCiphertext {
                    recipient_user_id: 99,
                    recipient_device_id: "dev_2".into(),
                    ephemeral_key: Some("ek".into()),
                    used_signed_prekey_id: Some(1),
                    used_one_time_prekey_id: Some(10),
                    header: RatchetHeaderDto {
                        dh_pub: "dh".into(),
                        prev_chain_len: 0,
                        msg_num: 0,
                    },
                    ciphertext: "ct".into(),
                }],
            },
        };
        let mc = MessageContent::Encrypted(env.clone());
        let mc_json = serde_json::to_string(&mc).unwrap();
        let mc_dec: MessageContent = serde_json::from_str(&mc_json).unwrap();
        assert_eq!(mc, mc_dec);
    }
}

