//! End-to-End Encryption (E2EE) subsystem for Secret Chats, Group Sender Keys,
//! and VoIP Call Verification:
//! - X3DH (X25519 + Ed25519 signed prekey + HKDF-SHA256)
//! - Double Ratchet (Asymmetric X25519 DH + Symmetric HMAC-SHA256 + ChaCha20-Poly1305 AEAD)
//! - Group Sender Keys (Symmetric chain ratchet + Ed25519 per-message signature)
//! - Order-invariant 4-emoji X25519 Safety Fingerprint
//! - Telegram 333-emoji 4-glyph `CallsEmojiFingerprint`

use base64::Engine;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use x25519_dalek::{PublicKey as X25519Public, StaticSecret as X25519Secret};
use zeroize::Zeroize;

type HmacSha256 = Hmac<Sha256>;

pub const K_EMOJI_COUNT: usize = 333;
const X3DH_INFO: &[u8] = b"gpui-telegram-e2ee-x3dh-v1";
const RK_INFO: &[u8] = b"gpui-telegram-dr-root-v1";
const SENDER_KEY_INFO: &[u8] = b"gpui-telegram-sender-key-v1";
const MAX_SKIP: u32 = 256;

/// Telegram 333-emoji verification palette (upstream `calls_emoji_fingerprint.cpp`).
pub static EMOJI_PALETTE: &[&str; K_EMOJI_COUNT] = &[
    "😉", "😍", "😛", "😭", "😱", "😡", "😎", "😴", "😵", "😈", "😬", "😇",
    "😏", "👮", "👷", "💂", "👶", "👨", "👩", "👴", "👵", "😻", "😽", "🙀",
    "👺", "🙈", "🙉", "🙊", "💀", "👽", "💩", "🔥", "💥", "💤", "👂", "👀",
    "👃", "👅", "👄", "👍", "👎", "👌", "👊", "✌", "✋", "👐", "👆", "👇",
    "👉", "👈", "🙏", "👏", "💪", "🚶", "🏃", "💃", "👫", "👪", "👬", "👭",
    "💅", "🎩", "👑", "👒", "👟", "👞", "👠", "👕", "👗", "👖", "👙", "👜",
    "👓", "🎀", "💄", "💛", "💙", "💜", "💚", "💍", "💎", "🐶", "🐺", "🐱",
    "🐭", "🐹", "🐰", "🐸", "🐯", "🐨", "🐻", "🐷", "🐮", "🐗", "🐴", "🐑",
    "🐘", "🐼", "🐧", "🐥", "🐔", "🐍", "🐢", "🐛", "🐝", "🐜", "🐞", "🐌",
    "🐙", "🐚", "🐟", "🐬", "🐋", "🐐", "🐊", "🐫", "🍀", "🌹", "🌻", "🍁",
    "🌾", "🍄", "🌵", "🌴", "🌳", "🌞", "🌚", "🌙", "🌎", "🌋", "⚡", "☔",
    "❄", "⛄", "🌀", "🌈", "🌊", "🎓", "🎆", "🎃", "👻", "🎅", "🎄", "🎁",
    "🎈", "🔮", "🎥", "📷", "💿", "💻", "☎", "📡", "📺", "📻", "🔉", "🔔",
    "⏳", "⏰", "⌚", "🔒", "🔑", "🔎", "💡", "🔦", "🔌", "🔋", "🚿", "🚽",
    "🔧", "🔨", "🚪", "🚬", "💣", "🔫", "🔪", "💊", "💉", "💰", "💵", "💳",
    "✉", "📫", "📦", "📅", "📁", "✂", "📌", "📎", "✒", "✏", "📐", "📚",
    "🔬", "🔭", "🎨", "🎬", "🎤", "🎧", "🎵", "🎹", "🎻", "🎺", "🎸", "👾",
    "🎮", "🃏", "🎲", "🎯", "🏈", "🏀", "⚽", "⚾", "🎾", "🎱", "🏉", "🎳",
    "🏁", "🏇", "🏆", "🏊", "🏄", "☕", "🍼", "🍺", "🍷", "🍴", "🍕", "🍔",
    "🍟", "🍗", "🍱", "🍚", "🍜", "🍡", "🍳", "🍞", "🍩", "🍦", "🎂", "🍰",
    "🍪", "🍫", "🍭", "🍯", "🍎", "🍏", "🍊", "🍋", "🍒", "🍇", "🍉", "🍓",
    "🍑", "🍌", "🍐", "🍍", "🍆", "🍅", "🌽", "🏡", "🏥", "🏦", "⛪", "🏰",
    "⛺", "🏭", "🗻", "🗽", "🎠", "🎡", "⛲", "🎢", "🚢", "🚤", "⚓", "🚀",
    "✈", "🚁", "🚂", "🚋", "🚎", "🚌", "🚙", "🚗", "🚕", "🚛", "🚨", "🚔",
    "🚒", "🚑", "🚲", "🚠", "🚜", "🚦", "⚠", "🚧", "⛽", "🎰", "🗿", "🎪",
    "🎭", "🇯🇵", "🇰🇷", "🇩🇪", "🇨🇳", "🇺🇸", "🇫🇷", "🇪🇸", "🇮🇹", "🇷🇺", "🇬🇧", "1⃣",
    "2⃣", "3⃣", "4⃣", "5⃣", "6⃣", "7⃣", "8⃣", "9⃣", "0⃣", "🔟", "❗", "❓",
    "♥", "♦", "💯", "🔗", "🔱", "🔴", "🔵", "🔶", "🔷",
];

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn decode_hex_32(hex: &str) -> Result<[u8; 32], String> {
    let bytes = decode_hex_vec(hex)?;
    bytes
        .try_into()
        .map_err(|_| "Expected 32-byte (64-hex-char) key".to_string())
}

pub fn decode_hex_64(hex: &str) -> Result<[u8; 64], String> {
    let bytes = decode_hex_vec(hex)?;
    bytes
        .try_into()
        .map_err(|_| "Expected 64-byte (128-hex-char) signature".to_string())
}

pub fn decode_hex_vec(hex: &str) -> Result<Vec<u8>, String> {
    let clean = hex.trim();
    if clean.len() % 2 != 0 {
        return Err("Odd hex length".into());
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    for i in (0..clean.len()).step_by(2) {
        let byte = u8::from_str_radix(&clean[i..i + 2], 16)
            .map_err(|e| format!("Invalid hex byte: {e}"))?;
        out.push(byte);
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrekeyBundlePublic {
    pub device_id: String,
    pub identity_dh_pub: [u8; 32],
    pub identity_sign_pub: [u8; 32],
    pub signed_prekey_id: u32,
    pub signed_prekey_pub: [u8; 32],
    pub signed_prekey_sig: Vec<u8>,
    pub one_time_prekey: Option<(u32, [u8; 32])>,
}

#[derive(Serialize, Deserialize)]
pub struct DeviceKeyMaterial {
    pub device_id: String,
    pub identity_dh_secret: [u8; 32],
    pub identity_dh_pub: [u8; 32],
    pub identity_sign_secret: [u8; 32],
    pub identity_sign_pub: [u8; 32],
    pub signed_prekey_id: u32,
    pub signed_prekey_secret: [u8; 32],
    pub signed_prekey_pub: [u8; 32],
    pub signed_prekey_sig: Vec<u8>,
    pub one_time_prekey_id: u32,
    pub one_time_prekey_secret: [u8; 32],
    pub one_time_prekey_pub: [u8; 32],
}

impl DeviceKeyMaterial {
    pub fn generate(device_id: impl Into<String>) -> Self {
        let mut ik_bytes = [0u8; 32];
        let mut sign_bytes = [0u8; 32];
        let mut spk_bytes = [0u8; 32];
        let mut opk_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut ik_bytes);
        OsRng.fill_bytes(&mut sign_bytes);
        OsRng.fill_bytes(&mut spk_bytes);
        OsRng.fill_bytes(&mut opk_bytes);

        let ik_sec = X25519Secret::from(ik_bytes);
        let ik_pub = X25519Public::from(&ik_sec).to_bytes();

        let sign_key = SigningKey::from_bytes(&sign_bytes);
        let verify_pub = sign_key.verifying_key().to_bytes();

        let spk_sec = X25519Secret::from(spk_bytes);
        let spk_pub = X25519Public::from(&spk_sec).to_bytes();
        let spk_sig = sign_key.sign(&spk_pub).to_bytes().to_vec();

        let opk_sec = X25519Secret::from(opk_bytes);
        let opk_pub = X25519Public::from(&opk_sec).to_bytes();

        Self {
            device_id: device_id.into(),
            identity_dh_secret: ik_sec.to_bytes(),
            identity_dh_pub: ik_pub,
            identity_sign_secret: sign_bytes,
            identity_sign_pub: verify_pub,
            signed_prekey_id: 1,
            signed_prekey_secret: spk_sec.to_bytes(),
            signed_prekey_pub: spk_pub,
            signed_prekey_sig: spk_sig,
            one_time_prekey_id: 1001,
            one_time_prekey_secret: opk_sec.to_bytes(),
            one_time_prekey_pub: opk_pub,
        }
    }

    pub fn public_bundle(&self) -> PrekeyBundlePublic {
        PrekeyBundlePublic {
            device_id: self.device_id.clone(),
            identity_dh_pub: self.identity_dh_pub,
            identity_sign_pub: self.identity_sign_pub,
            signed_prekey_id: self.signed_prekey_id,
            signed_prekey_pub: self.signed_prekey_pub,
            signed_prekey_sig: self.signed_prekey_sig.clone(),
            one_time_prekey: Some((self.one_time_prekey_id, self.one_time_prekey_pub)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X3dhInitHeader {
    pub initiator_identity_dh_pub: [u8; 32],
    pub ephemeral_pub: [u8; 32],
    pub signed_prekey_id: u32,
    pub one_time_prekey_id: Option<u32>,
}

pub fn verify_prekey_bundle(bundle: &PrekeyBundlePublic) -> Result<(), String> {
    let vk = VerifyingKey::from_bytes(&bundle.identity_sign_pub)
        .map_err(|e| format!("Invalid identity verifying key: {e}"))?;
    let sig_arr: [u8; 64] = bundle
        .signed_prekey_sig
        .as_slice()
        .try_into()
        .map_err(|_| "Signed prekey signature must be 64 bytes".to_string())?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(&bundle.signed_prekey_pub, &sig)
        .map_err(|_| "Signed prekey Ed25519 signature verification failed".to_string())
}

pub fn x3dh_initiator(
    local_identity_dh_secret: &[u8; 32],
    remote: &PrekeyBundlePublic,
) -> Result<([u8; 32], X3dhInitHeader), String> {
    verify_prekey_bundle(remote)?;

    let ik_a = X25519Secret::from(*local_identity_dh_secret);
    let ik_a_pub = X25519Public::from(&ik_a);

    let mut ek_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut ek_bytes);
    let ek_a = X25519Secret::from(ek_bytes);
    let ek_a_pub = X25519Public::from(&ek_a);
    ek_bytes.zeroize();

    let ik_b = X25519Public::from(remote.identity_dh_pub);
    let spk_b = X25519Public::from(remote.signed_prekey_pub);

    let dh1 = ik_a.diffie_hellman(&spk_b);
    let dh2 = ek_a.diffie_hellman(&ik_b);
    let dh3 = ek_a.diffie_hellman(&spk_b);

    let mut ikm = Vec::with_capacity(32 * 5);
    ikm.extend_from_slice(&[0xFFu8; 32]);
    ikm.extend_from_slice(dh1.as_bytes());
    ikm.extend_from_slice(dh2.as_bytes());
    ikm.extend_from_slice(dh3.as_bytes());

    let mut used_opk_id = None;
    if let Some((opk_id, opk_pub_bytes)) = remote.one_time_prekey {
        let opk_b = X25519Public::from(opk_pub_bytes);
        let dh4 = ek_a.diffie_hellman(&opk_b);
        ikm.extend_from_slice(dh4.as_bytes());
        used_opk_id = Some(opk_id);
    }

    let hk = Hkdf::<Sha256>::new(Some(&[0u8; 32]), &ikm);
    let mut sk = [0u8; 32];
    hk.expand(X3DH_INFO, &mut sk)
        .map_err(|_| "X3DH HKDF expand failed".to_string())?;
    ikm.zeroize();

    Ok((
        sk,
        X3dhInitHeader {
            initiator_identity_dh_pub: ik_a_pub.to_bytes(),
            ephemeral_pub: ek_a_pub.to_bytes(),
            signed_prekey_id: remote.signed_prekey_id,
            one_time_prekey_id: used_opk_id,
        },
    ))
}

pub fn x3dh_responder(
    local_identity_dh_secret: &[u8; 32],
    local_signed_prekey_secret: &[u8; 32],
    local_one_time_prekey_secret: Option<&[u8; 32]>,
    header: &X3dhInitHeader,
) -> Result<[u8; 32], String> {
    let ik_b = X25519Secret::from(*local_identity_dh_secret);
    let spk_b = X25519Secret::from(*local_signed_prekey_secret);
    let ik_a = X25519Public::from(header.initiator_identity_dh_pub);
    let ek_a = X25519Public::from(header.ephemeral_pub);

    let dh1 = spk_b.diffie_hellman(&ik_a);
    let dh2 = ik_b.diffie_hellman(&ek_a);
    let dh3 = spk_b.diffie_hellman(&ek_a);

    let mut ikm = Vec::with_capacity(32 * 5);
    ikm.extend_from_slice(&[0xFFu8; 32]);
    ikm.extend_from_slice(dh1.as_bytes());
    ikm.extend_from_slice(dh2.as_bytes());
    ikm.extend_from_slice(dh3.as_bytes());

    if header.one_time_prekey_id.is_some() {
        let opk_sec_bytes =
            local_one_time_prekey_secret.ok_or_else(|| "Missing one-time prekey secret".to_string())?;
        let opk_b = X25519Secret::from(*opk_sec_bytes);
        let dh4 = opk_b.diffie_hellman(&ek_a);
        ikm.extend_from_slice(dh4.as_bytes());
    }

    let hk = Hkdf::<Sha256>::new(Some(&[0u8; 32]), &ikm);
    let mut sk = [0u8; 32];
    hk.expand(X3DH_INFO, &mut sk)
        .map_err(|_| "X3DH responder HKDF expand failed".to_string())?;
    ikm.zeroize();
    Ok(sk)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrHeader {
    pub dh_pub: [u8; 32],
    pub prev_chain_len: u32,
    pub msg_num: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedDrMessage {
    pub header: DrHeader,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

fn kdf_rk(rk: &[u8; 32], dh_out: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let hk = Hkdf::<Sha256>::new(Some(rk), dh_out);
    let mut out = [0u8; 64];
    hk.expand(RK_INFO, &mut out).expect("64-byte HKDF expand");
    let mut next_rk = [0u8; 32];
    let mut ck = [0u8; 32];
    next_rk.copy_from_slice(&out[..32]);
    ck.copy_from_slice(&out[32..]);
    out.zeroize();
    (next_rk, ck)
}

fn kdf_ck(ck: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut mac_mk = <HmacSha256 as Mac>::new_from_slice(ck).expect("HMAC key");
    mac_mk.update(&[0x01]);
    let mk_bytes = mac_mk.finalize().into_bytes();

    let mut mac_ck = <HmacSha256 as Mac>::new_from_slice(ck).expect("HMAC key");
    mac_ck.update(&[0x02]);
    let next_ck_bytes = mac_ck.finalize().into_bytes();

    let mut next_ck = [0u8; 32];
    let mut mk = [0u8; 32];
    next_ck.copy_from_slice(&next_ck_bytes);
    mk.copy_from_slice(&mk_bytes);
    (next_ck, mk)
}

#[derive(Serialize, Deserialize)]
pub struct DoubleRatchetSession {
    dh_self_secret: [u8; 32],
    dh_self_pub: [u8; 32],
    dh_remote_pub: Option<[u8; 32]>,
    root_key: [u8; 32],
    chain_key_send: Option<[u8; 32]>,
    chain_key_recv: Option<[u8; 32]>,
    send_count: u32,
    recv_count: u32,
    prev_send_count: u32,
    skipped_keys: HashMap<(String, u32), [u8; 32]>,
    seen_messages: HashSet<(String, u32)>,
}

impl DoubleRatchetSession {
    pub fn init_initiator(shared_secret: [u8; 32], remote_ratchet_pub: [u8; 32]) -> Self {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let dh_self = X25519Secret::from(seed);
        let dh_self_pub = X25519Public::from(&dh_self).to_bytes();
        seed.zeroize();

        let remote_pub = X25519Public::from(remote_ratchet_pub);
        let dh_out = dh_self.diffie_hellman(&remote_pub);
        let (rk, cks) = kdf_rk(&shared_secret, dh_out.as_bytes());

        Self {
            dh_self_secret: dh_self.to_bytes(),
            dh_self_pub,
            dh_remote_pub: Some(remote_ratchet_pub),
            root_key: rk,
            chain_key_send: Some(cks),
            chain_key_recv: None,
            send_count: 0,
            recv_count: 0,
            prev_send_count: 0,
            skipped_keys: HashMap::new(),
            seen_messages: HashSet::new(),
        }
    }

    pub fn init_responder(shared_secret: [u8; 32], local_ratchet_secret: [u8; 32]) -> Self {
        let dh_self = X25519Secret::from(local_ratchet_secret);
        let dh_self_pub = X25519Public::from(&dh_self).to_bytes();

        Self {
            dh_self_secret: dh_self.to_bytes(),
            dh_self_pub,
            dh_remote_pub: None,
            root_key: shared_secret,
            chain_key_send: None,
            chain_key_recv: None,
            send_count: 0,
            recv_count: 0,
            prev_send_count: 0,
            skipped_keys: HashMap::new(),
            seen_messages: HashSet::new(),
        }
    }

    pub fn encrypt(&mut self, plaintext: &[u8], associated_data: &[u8]) -> Result<EncryptedDrMessage, String> {
        let cks = self
            .chain_key_send
            .ok_or_else(|| "Sending chain not initialized yet".to_string())?;
        let (next_cks, mut mk) = kdf_ck(&cks);
        self.chain_key_send = Some(next_cks);

        let header = DrHeader {
            dh_pub: self.dh_self_pub,
            prev_chain_len: self.prev_send_count,
            msg_num: self.send_count,
        };
        self.send_count += 1;

        let aad = build_aad(associated_data, &header);
        let cipher = ChaCha20Poly1305::new((&mk).into());
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| "ChaCha20-Poly1305 encryption failed".to_string())?;
        mk.zeroize();

        Ok(EncryptedDrMessage {
            header,
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    pub fn decrypt(&mut self, msg: &EncryptedDrMessage, associated_data: &[u8]) -> Result<Vec<u8>, String> {
        let key_id = (encode_hex(&msg.header.dh_pub), msg.header.msg_num);
        if self.seen_messages.contains(&key_id) {
            return Err("Duplicate/replayed Double Ratchet message rejected".into());
        }

        if let Some(mut mk) = self.skipped_keys.remove(&key_id) {
            let pt = decrypt_with_mk(&mk, msg, associated_data)?;
            mk.zeroize();
            self.seen_messages.insert(key_id);
            return Ok(pt);
        }

        if self.dh_remote_pub != Some(msg.header.dh_pub) {
            self.skip_message_keys(msg.header.prev_chain_len)?;
            self.dh_ratchet(&msg.header.dh_pub);
        }

        self.skip_message_keys(msg.header.msg_num)?;
        let ckr = self
            .chain_key_recv
            .ok_or_else(|| "Receiving chain not initialized".to_string())?;
        let (next_ckr, mut mk) = kdf_ck(&ckr);
        self.chain_key_recv = Some(next_ckr);
        self.recv_count += 1;

        let pt = decrypt_with_mk(&mk, msg, associated_data)?;
        mk.zeroize();
        self.seen_messages.insert(key_id);
        Ok(pt)
    }

    fn skip_message_keys(&mut self, until: u32) -> Result<(), String> {
        if self.recv_count + MAX_SKIP < until {
            return Err(format!("Skipped message window exceeded ({MAX_SKIP})"));
        }
        if let Some(mut ckr) = self.chain_key_recv {
            while self.recv_count < until {
                let (next_ckr, mk) = kdf_ck(&ckr);
                ckr = next_ckr;
                if let Some(rem) = self.dh_remote_pub {
                    self.skipped_keys
                        .insert((encode_hex(&rem), self.recv_count), mk);
                }
                self.recv_count += 1;
            }
            self.chain_key_recv = Some(ckr);
        }
        Ok(())
    }

    fn dh_ratchet(&mut self, new_remote_pub: &[u8; 32]) {
        self.prev_send_count = self.send_count;
        self.send_count = 0;
        self.recv_count = 0;
        self.dh_remote_pub = Some(*new_remote_pub);

        let remote = X25519Public::from(*new_remote_pub);
        let cur_self = X25519Secret::from(self.dh_self_secret);
        let dh_recv = cur_self.diffie_hellman(&remote);
        let (rk1, ckr) = kdf_rk(&self.root_key, dh_recv.as_bytes());
        self.chain_key_recv = Some(ckr);

        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let next_self = X25519Secret::from(seed);
        seed.zeroize();
        self.dh_self_pub = X25519Public::from(&next_self).to_bytes();
        let dh_send = next_self.diffie_hellman(&remote);
        self.dh_self_secret = next_self.to_bytes();

        let (rk2, cks) = kdf_rk(&rk1, dh_send.as_bytes());
        self.root_key = rk2;
        self.chain_key_send = Some(cks);
    }
}

fn build_aad(ad: &[u8], header: &DrHeader) -> Vec<u8> {
    let hdr_bytes = serde_json::to_vec(header).unwrap_or_default();
    let mut aad = Vec::with_capacity(ad.len() + hdr_bytes.len());
    aad.extend_from_slice(ad);
    aad.extend_from_slice(&hdr_bytes);
    aad
}

fn decrypt_with_mk(mk: &[u8; 32], msg: &EncryptedDrMessage, ad: &[u8]) -> Result<Vec<u8>, String> {
    let aad = build_aad(ad, &msg.header);
    let cipher = ChaCha20Poly1305::new(mk.into());
    cipher
        .decrypt(
            Nonce::from_slice(&msg.nonce),
            Payload {
                msg: &msg.ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| "ChaCha20-Poly1305 authentication tag mismatch".to_string())
}

/// Group E2EE Sender Key State (Signal-style symmetric chain ratchet + Ed25519 signature).
#[derive(Clone, Serialize, Deserialize)]
pub struct SenderKeyState {
    pub group_id: String,
    pub sender_id: String,
    pub key_id: u32,
    pub chain_key: [u8; 32],
    pub iteration: u32,
    pub sign_secret: Option<[u8; 32]>,
    pub verify_pub: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSenderKeyMessage {
    pub group_id: String,
    pub sender_id: String,
    pub key_id: u32,
    pub iteration: u32,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub signature: Vec<u8>,
}

impl SenderKeyState {
    pub fn generate(group_id: impl Into<String>, sender_id: impl Into<String>, key_id: u32) -> Self {
        let mut ck = [0u8; 32];
        let mut sk_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut ck);
        OsRng.fill_bytes(&mut sk_bytes);
        let sk = SigningKey::from_bytes(&sk_bytes);
        let vk = sk.verifying_key().to_bytes();

        Self {
            group_id: group_id.into(),
            sender_id: sender_id.into(),
            key_id,
            chain_key: ck,
            iteration: 0,
            sign_secret: Some(sk_bytes),
            verify_pub: vk,
        }
    }

    pub fn receiver_view(&self) -> Self {
        Self {
            group_id: self.group_id.clone(),
            sender_id: self.sender_id.clone(),
            key_id: self.key_id,
            chain_key: self.chain_key,
            iteration: self.iteration,
            sign_secret: None,
            verify_pub: self.verify_pub,
        }
    }

    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<EncryptedSenderKeyMessage, String> {
        let sign_bytes = self
            .sign_secret
            .ok_or_else(|| "SenderKeyState is read-only receiver view".to_string())?;
        let (next_ck, mut mk) = sender_chain_step(&self.chain_key);
        let iter = self.iteration;
        self.chain_key = next_ck;
        self.iteration += 1;

        let cipher = ChaCha20Poly1305::new((&mk).into());
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let aad = format!("{}:{}:{}:{}", self.group_id, self.sender_id, self.key_id, iter);
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| "SenderKey ChaCha20-Poly1305 encryption failed".to_string())?;
        mk.zeroize();

        let signing_key = SigningKey::from_bytes(&sign_bytes);
        let mut to_sign = Vec::with_capacity(aad.len() + 12 + ciphertext.len());
        to_sign.extend_from_slice(aad.as_bytes());
        to_sign.extend_from_slice(&nonce);
        to_sign.extend_from_slice(&ciphertext);
        let sig = signing_key.sign(&to_sign).to_bytes().to_vec();

        Ok(EncryptedSenderKeyMessage {
            group_id: self.group_id.clone(),
            sender_id: self.sender_id.clone(),
            key_id: self.key_id,
            iteration: iter,
            nonce,
            ciphertext,
            signature: sig,
        })
    }

    pub fn decrypt(&mut self, msg: &EncryptedSenderKeyMessage) -> Result<Vec<u8>, String> {
        if msg.iteration < self.iteration {
            return Err("Stale or replayed group SenderKey iteration".into());
        }
        let aad = format!("{}:{}:{}:{}", msg.group_id, msg.sender_id, msg.key_id, msg.iteration);
        let vk = VerifyingKey::from_bytes(&self.verify_pub)
            .map_err(|e| format!("Invalid sender verify key: {e}"))?;
        let sig_arr: [u8; 64] = msg
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| "SenderKey signature must be 64 bytes".to_string())?;
        let mut signed_payload = Vec::with_capacity(aad.len() + 12 + msg.ciphertext.len());
        signed_payload.extend_from_slice(aad.as_bytes());
        signed_payload.extend_from_slice(&msg.nonce);
        signed_payload.extend_from_slice(&msg.ciphertext);
        vk.verify(&signed_payload, &Signature::from_bytes(&sig_arr))
            .map_err(|_| "SenderKey Ed25519 signature verification failed".to_string())?;

        while self.iteration < msg.iteration {
            let (next_ck, _) = sender_chain_step(&self.chain_key);
            self.chain_key = next_ck;
            self.iteration += 1;
        }
        let (next_ck, mut mk) = sender_chain_step(&self.chain_key);
        self.chain_key = next_ck;
        self.iteration += 1;

        let cipher = ChaCha20Poly1305::new((&mk).into());
        let pt = cipher
            .decrypt(
                Nonce::from_slice(&msg.nonce),
                Payload {
                    msg: &msg.ciphertext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| "SenderKey ChaCha20-Poly1305 decryption failed".to_string());
        mk.zeroize();
        pt
    }
}

fn sender_chain_step(ck: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let hk = Hkdf::<Sha256>::new(Some(ck), SENDER_KEY_INFO);
    let mut out = [0u8; 64];
    hk.expand(b"step", &mut out).expect("64-byte sender step");
    let mut next_ck = [0u8; 32];
    let mut mk = [0u8; 32];
    next_ck.copy_from_slice(&out[..32]);
    mk.copy_from_slice(&out[32..]);
    out.zeroize();
    (next_ck, mk)
}

/// Computes the 63-bit big-endian integer from an 8-byte slice matching Telegram `calls_emoji_fingerprint.cpp`.
pub fn compute_emoji_index_from_slice(bytes: &[u8]) -> u64 {
    assert!(bytes.len() >= 8);
    ((bytes[0] as u64 & 0x7F) << 56)
        | ((bytes[1] as u64) << 48)
        | ((bytes[2] as u64) << 40)
        | ((bytes[3] as u64) << 32)
        | ((bytes[4] as u64) << 24)
        | ((bytes[5] as u64) << 16)
        | ((bytes[6] as u64) << 8)
        | (bytes[7] as u64)
}

pub fn compute_emoji_fingerprint_indices(sha256: &[u8; 32]) -> [usize; 4] {
    let mut indices = [0usize; 4];
    for i in 0..4 {
        let chunk = &sha256[i * 8..(i + 1) * 8];
        let val = compute_emoji_index_from_slice(chunk);
        indices[i] = (val as usize) % K_EMOJI_COUNT;
    }
    indices
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallsEmojiFingerprint {
    pub key_sha256_hex: String,
    pub emoji_indices: [usize; 4],
    pub emoji_chars: [String; 4],
    pub emoji_display: String,
    pub palette_size: usize,
    pub is_verified: bool,
}

impl CallsEmojiFingerprint {
    pub fn from_shared_key(shared_key: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(shared_key);
        let digest: [u8; 32] = hasher.finalize().into();
        Self::from_sha256(digest)
    }

    pub fn from_sha256(sha256: [u8; 32]) -> Self {
        let indices = compute_emoji_fingerprint_indices(&sha256);
        let chars = [
            EMOJI_PALETTE[indices[0]].to_string(),
            EMOJI_PALETTE[indices[1]].to_string(),
            EMOJI_PALETTE[indices[2]].to_string(),
            EMOJI_PALETTE[indices[3]].to_string(),
        ];
        let display = chars.join(" ");
        Self {
            key_sha256_hex: encode_hex(&sha256),
            emoji_indices: indices,
            emoji_chars: chars,
            emoji_display: display,
            palette_size: K_EMOJI_COUNT,
            is_verified: true,
        }
    }
}

/// Order-invariant 4-emoji X25519 safety fingerprint for two identity public keys.
pub fn compute_safety_fingerprint(identity_a: &[u8; 32], identity_b: &[u8; 32]) -> CallsEmojiFingerprint {
    let (first, second) = if identity_a <= identity_b {
        (identity_a, identity_b)
    } else {
        (identity_b, identity_a)
    };
    let mut hasher = Sha256::new();
    hasher.update(b"fabushi-e2ee-safety-v1:");
    hasher.update(first);
    hasher.update(second);
    let digest: [u8; 32] = hasher.finalize().into();
    CallsEmojiFingerprint::from_sha256(digest)
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS e2ee_devices (
            device_id TEXT PRIMARY KEY,
            material_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS e2ee_secret_chats (
            chat_id TEXT PRIMARY KEY,
            peer_id TEXT NOT NULL,
            local_identity_hex TEXT NOT NULL,
            peer_identity_hex TEXT NOT NULL,
            initiator_session_json TEXT NOT NULL,
            responder_session_json TEXT,
            safety_emojis TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS e2ee_sender_keys (
            group_id TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            state_json TEXT NOT NULL,
            PRIMARY KEY (group_id, sender_id)
        );",
    )
    .map_err(|e| format!("Failed to initialize E2EE schema: {e}"))
}

fn load_or_create_device(conn: &Connection, device_id: &str) -> Result<DeviceKeyMaterial, String> {
    if let Some(raw) = conn
        .query_row(
            "SELECT material_json FROM e2ee_devices WHERE device_id = ?1",
            params![device_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        return serde_json::from_str(&raw).map_err(|e| e.to_string());
    }
    let mat = DeviceKeyMaterial::generate(device_id);
    let raw = serde_json::to_string(&mat).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO e2ee_devices (device_id, material_json, created_at_ms) VALUES (?1, ?2, ?3)",
        params![device_id, raw, chrono::Utc::now().timestamp_millis()],
    )
    .map_err(|e| e.to_string())?;
    Ok(mat)
}

pub fn handle_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    match submethod {
        "initDevice" => {
            let device_id = params
                .get("deviceId")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("local-android-device");
            let mat = load_or_create_device(conn, device_id)?;
            let self_fp = compute_safety_fingerprint(&mat.identity_dh_pub, &mat.identity_dh_pub);
            Ok(json!({
                "deviceId": mat.device_id,
                "identityDhPubHex": encode_hex(&mat.identity_dh_pub),
                "identitySignPubHex": encode_hex(&mat.identity_sign_pub),
                "signedPrekeyId": mat.signed_prekey_id,
                "signedPrekeyPubHex": encode_hex(&mat.signed_prekey_pub),
                "signedPrekeySigHex": encode_hex(&mat.signed_prekey_sig),
                "oneTimePrekeyId": mat.one_time_prekey_id,
                "oneTimePrekeyPubHex": encode_hex(&mat.one_time_prekey_pub),
                "selfFingerprint": self_fp,
            }))
        }

        "createSecretChat" => {
            let local_device_id = params
                .get("deviceId")
                .and_then(Value::as_str)
                .unwrap_or("local-android-device");
            let peer_id = params
                .get("peerId")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("peer-default");
            let chat_id = params
                .get("chatId")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("secret_{}", uuid::Uuid::new_v4().simple()));

            let local = load_or_create_device(conn, local_device_id)?;
            let peer_device_key = format!("peer:{peer_id}");
            let peer = load_or_create_device(conn, &peer_device_key)?;
            let peer_bundle = peer.public_bundle();

            let (sk_alice, hdr) = x3dh_initiator(&local.identity_dh_secret, &peer_bundle)?;
            let sk_bob = x3dh_responder(
                &peer.identity_dh_secret,
                &peer.signed_prekey_secret,
                Some(&peer.one_time_prekey_secret),
                &hdr,
            )?;
            if sk_alice != sk_bob {
                return Err("X3DH shared secret mismatch".into());
            }

            let initiator = DoubleRatchetSession::init_initiator(sk_alice, peer.signed_prekey_pub);
            let responder = DoubleRatchetSession::init_responder(sk_bob, peer.signed_prekey_secret);
            let fp = compute_safety_fingerprint(&local.identity_dh_pub, &peer.identity_dh_pub);

            let init_json = serde_json::to_string(&initiator).map_err(|e| e.to_string())?;
            let resp_json = serde_json::to_string(&responder).map_err(|e| e.to_string())?;

            conn.execute(
                "INSERT OR REPLACE INTO e2ee_secret_chats
                (chat_id, peer_id, local_identity_hex, peer_identity_hex, initiator_session_json, responder_session_json, safety_emojis, created_at_ms)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    chat_id,
                    peer_id,
                    encode_hex(&local.identity_dh_pub),
                    encode_hex(&peer.identity_dh_pub),
                    init_json,
                    resp_json,
                    fp.emoji_display,
                    chrono::Utc::now().timestamp_millis(),
                ],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "chatId": chat_id,
                "peerId": peer_id,
                "protocol": "X3DH+DoubleRatchet-ChaCha20Poly1305",
                "localIdentityHex": encode_hex(&local.identity_dh_pub),
                "peerIdentityHex": encode_hex(&peer.identity_dh_pub),
                "safetyFingerprint": fp,
            }))
        }

        "sendEncrypted" => {
            let mode = params
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("secret_chat");
            let plaintext = params
                .get("plaintext")
                .or_else(|| params.get("text"))
                .and_then(Value::as_str)
                .ok_or_else(|| "sendEncrypted requires 'plaintext' or 'text'".to_string())?;

            if mode == "group" {
                let group_id = params
                    .get("groupId")
                    .or_else(|| params.get("chatId"))
                    .and_then(Value::as_str)
                    .unwrap_or("group-default");
                let sender_id = params
                    .get("senderId")
                    .and_then(Value::as_str)
                    .unwrap_or("local-android-device");
                let mut sender_state: SenderKeyState = if let Some(raw) = conn
                    .query_row(
                        "SELECT state_json FROM e2ee_sender_keys WHERE group_id = ?1 AND sender_id = ?2",
                        params![group_id, sender_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    serde_json::from_str(&raw).map_err(|e| e.to_string())?
                } else {
                    SenderKeyState::generate(group_id, sender_id, 1)
                };
                let mut receiver = sender_state.receiver_view();
                let encrypted = sender_state.encrypt(plaintext.as_bytes())?;
                let verified_bytes = receiver.decrypt(&encrypted)?;
                let state_json = serde_json::to_string(&sender_state).map_err(|e| e.to_string())?;
                conn.execute(
                    "INSERT OR REPLACE INTO e2ee_sender_keys (group_id, sender_id, state_json) VALUES (?1, ?2, ?3)",
                    params![group_id, sender_id, state_json],
                )
                .map_err(|e| e.to_string())?;

                return Ok(json!({
                    "mode": "group_sender_key",
                    "groupId": group_id,
                    "senderId": sender_id,
                    "iteration": encrypted.iteration,
                    "ciphertextBase64": base64::engine::general_purpose::STANDARD.encode(&encrypted.ciphertext),
                    "nonceHex": encode_hex(&encrypted.nonce),
                    "signatureHex": encode_hex(&encrypted.signature),
                    "verifiedRoundtrip": String::from_utf8_lossy(&verified_bytes) == plaintext,
                }));
            }

            let chat_id = params
                .get("chatId")
                .and_then(Value::as_str)
                .ok_or_else(|| "sendEncrypted requires 'chatId'".to_string())?;
            let (init_raw, resp_raw): (String, Option<String>) = conn
                .query_row(
                    "SELECT initiator_session_json, responder_session_json FROM e2ee_secret_chats WHERE chat_id = ?1",
                    params![chat_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Secret chat '{chat_id}' not found"))?;

            let mut initiator: DoubleRatchetSession =
                serde_json::from_str(&init_raw).map_err(|e| e.to_string())?;
            let encrypted = initiator.encrypt(plaintext.as_bytes(), chat_id.as_bytes())?;

            let mut verified = false;
            let mut next_resp_raw = resp_raw;
            if let Some(ref r_str) = next_resp_raw {
                let mut responder: DoubleRatchetSession =
                    serde_json::from_str(r_str).map_err(|e| e.to_string())?;
                let dec = responder.decrypt(&encrypted, chat_id.as_bytes())?;
                verified = dec == plaintext.as_bytes();
                next_resp_raw = Some(serde_json::to_string(&responder).map_err(|e| e.to_string())?);
            }

            let next_init_raw = serde_json::to_string(&initiator).map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE e2ee_secret_chats SET initiator_session_json = ?1, responder_session_json = ?2 WHERE chat_id = ?3",
                params![next_init_raw, next_resp_raw, chat_id],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "mode": "double_ratchet",
                "chatId": chat_id,
                "algorithm": "ChaCha20-Poly1305",
                "msgNum": encrypted.header.msg_num,
                "ratchetPubHex": encode_hex(&encrypted.header.dh_pub),
                "nonceHex": encode_hex(&encrypted.nonce),
                "ciphertextBase64": base64::engine::general_purpose::STANDARD.encode(&encrypted.ciphertext),
                "verifiedRoundtrip": verified,
            }))
        }

        "safetyFingerprint" => {
            if let Some(chat_id) = params.get("chatId").and_then(Value::as_str) {
                if let Some((loc_hex, peer_hex)) = conn
                    .query_row(
                        "SELECT local_identity_hex, peer_identity_hex FROM e2ee_secret_chats WHERE chat_id = ?1",
                        params![chat_id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                {
                    let a = decode_hex_32(&loc_hex)?;
                    let b = decode_hex_32(&peer_hex)?;
                    let fp = compute_safety_fingerprint(&a, &b);
                    return Ok(serde_json::to_value(fp).map_err(|e| e.to_string())?);
                }
            }
            let a_hex = params
                .get("localIdentityHex")
                .and_then(Value::as_str)
                .ok_or_else(|| "safetyFingerprint requires 'chatId' or 'localIdentityHex'".to_string())?;
            let b_hex = params
                .get("peerIdentityHex")
                .and_then(Value::as_str)
                .ok_or_else(|| "safetyFingerprint requires 'peerIdentityHex'".to_string())?;
            let a = decode_hex_32(a_hex)?;
            let b = decode_hex_32(b_hex)?;
            let fp = compute_safety_fingerprint(&a, &b);
            serde_json::to_value(fp).map_err(|e| e.to_string())
        }

        "callEmojiFingerprint" => {
            let fp = if let Some(sha_hex) = params.get("keySha256Hex").and_then(Value::as_str) {
                let sha = decode_hex_32(sha_hex)?;
                CallsEmojiFingerprint::from_sha256(sha)
            } else if let Some(key_hex) = params.get("sharedKeyHex").and_then(Value::as_str) {
                let key = decode_hex_vec(key_hex)?;
                CallsEmojiFingerprint::from_shared_key(&key)
            } else if let Some(b64) = params.get("sharedKeyBase64").and_then(Value::as_str) {
                let key = base64::engine::general_purpose::STANDARD
                    .decode(b64.trim())
                    .map_err(|e| format!("Invalid sharedKeyBase64: {e}"))?;
                CallsEmojiFingerprint::from_shared_key(&key)
            } else {
                let call_id = params
                    .get("callId")
                    .and_then(Value::as_str)
                    .unwrap_or("default-call");
                CallsEmojiFingerprint::from_shared_key(call_id.as_bytes())
            };
            serde_json::to_value(fp).map_err(|e| e.to_string())
        }

        other => Err(format!("Unsupported telegram.e2ee method: {other}")),
    }
}
