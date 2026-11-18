use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct UserId(pub u64);

impl UserId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "user_{}", self.0)
    }
}

impl FromStr for UserId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let trimmed = s.strip_prefix("user_").unwrap_or(s);
        trimmed.parse::<u64>().map(UserId)
    }
}

impl From<u64> for UserId {
    fn from(val: u64) -> Self {
        UserId(val)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct ChatId(pub i64);

impl ChatId {
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> i64 {
        self.0
    }
}

impl fmt::Display for ChatId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "chat_{}", self.0)
    }
}

impl FromStr for ChatId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let trimmed = s.strip_prefix("chat_").unwrap_or(s);
        trimmed.parse::<i64>().map(ChatId)
    }
}

impl From<i64> for ChatId {
    fn from(val: i64) -> Self {
        ChatId(val)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct ChannelId(pub u64);

impl ChannelId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "channel_{}", self.0)
    }
}

impl FromStr for ChannelId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let trimmed = s.strip_prefix("channel_").unwrap_or(s);
        trimmed.parse::<u64>().map(ChannelId)
    }
}

impl From<u64> for ChannelId {
    fn from(val: u64) -> Self {
        ChannelId(val)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct MsgId(pub i64);

impl MsgId {
    /// Upstream Telegram Desktop MsgId constants from `data_msg_id.h`:
    /// Client range: negative IDs starting from `0x01 - (1LL << 58)`
    pub const START_CLIENT: i64 = 0x01 - (1i64 << 58);
    pub const CLIENT_COUNT: i64 = 1i64 << 31;
    pub const END_CLIENT: i64 = Self::START_CLIENT + Self::CLIENT_COUNT;
    pub const SERVER_MAX: i64 = 1i64 << 56;

    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> i64 {
        self.0
    }

    pub fn is_client(&self) -> bool {
        self.0 >= Self::START_CLIENT && self.0 < Self::END_CLIENT
    }

    pub fn is_server(&self) -> bool {
        self.0 > 0 && self.0 < Self::SERVER_MAX
    }

    pub fn generate_client(offset: u32) -> Self {
        Self(Self::START_CLIENT + (offset as i64))
    }
}

impl fmt::Display for MsgId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "msg_{}", self.0)
    }
}

impl FromStr for MsgId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let trimmed = s.strip_prefix("msg_").unwrap_or(s);
        trimmed.parse::<i64>().map(MsgId)
    }
}

impl From<i64> for MsgId {
    fn from(val: i64) -> Self {
        MsgId(val)
    }
}

/// Backward compatibility alias
pub type MessageId = MsgId;

/// Backward compatibility constructor for tuple struct instantiation
#[allow(non_snake_case)]
pub const fn MessageId(id: i64) -> MsgId {
    MsgId(id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct TopicId(pub i64);

impl TopicId {
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> i64 {
        self.0
    }

    pub const fn to_msg_id(&self) -> MsgId {
        MsgId(self.0)
    }

    pub const fn from_msg_id(msg: MsgId) -> Self {
        Self(msg.0)
    }
}

impl fmt::Display for TopicId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "topic_{}", self.0)
    }
}

impl FromStr for TopicId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let trimmed = s.strip_prefix("topic_").unwrap_or(s);
        trimmed.parse::<i64>().map(TopicId)
    }
}

impl From<i64> for TopicId {
    fn from(val: i64) -> Self {
        TopicId(val)
    }
}

/// PeerId enum with upstream Telegram 48-bit bitfield shift encoding
/// matching `tdesktop/Telegram/SourceFiles/data/data_peer_id.h`:
/// - `BareId` occupies the lower 48 bits (`MASK_BARE = 0x0000_FFFF_FFFF_FFFF`).
/// - Entity shift occupies bits 48..55:
///   * Shift 0: User (`UserId`)
///   * Shift 1: Chat (`ChatId`)
///   * Shift 2: Channel (`ChannelId`)
///   * Shift 0x7F: Fake / Internal (`u64`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PeerId {
    User(UserId),
    Chat(ChatId),
    Channel(ChannelId),
    Fake(u64),
}

impl PeerId {
    pub const MASK_BARE: u64 = 0x0000_FFFF_FFFF_FFFF;
    pub const SHIFT_USER: u8 = 0;
    pub const SHIFT_CHAT: u8 = 1;
    pub const SHIFT_CHANNEL: u8 = 2;
    pub const SHIFT_FAKE: u8 = 0x7F;

    pub fn to_raw(&self) -> u64 {
        match self {
            PeerId::User(u) => (u.0 & Self::MASK_BARE) | ((Self::SHIFT_USER as u64) << 48),
            PeerId::Chat(c) => ((c.0 as u64) & Self::MASK_BARE) | ((Self::SHIFT_CHAT as u64) << 48),
            PeerId::Channel(ch) => (ch.0 & Self::MASK_BARE) | ((Self::SHIFT_CHANNEL as u64) << 48),
            PeerId::Fake(f) => (f & Self::MASK_BARE) | ((Self::SHIFT_FAKE as u64) << 48),
        }
    }

    pub fn to_raw_u64(&self) -> u64 {
        self.to_raw()
    }

    pub fn from_raw(raw: u64) -> Self {
        let shift = ((raw >> 48) & 0xFF) as u8;
        let bare = raw & Self::MASK_BARE;
        match shift {
            Self::SHIFT_USER => PeerId::User(UserId(bare)),
            Self::SHIFT_CHAT => PeerId::Chat(ChatId(bare as i64)),
            Self::SHIFT_CHANNEL => PeerId::Channel(ChannelId(bare)),
            _ => PeerId::Fake(bare),
        }
    }

    pub fn from_raw_u64(raw: u64) -> Self {
        Self::from_raw(raw)
    }

    pub fn is_user(&self) -> bool {
        matches!(self, PeerId::User(_))
    }

    pub fn is_chat(&self) -> bool {
        matches!(self, PeerId::Chat(_))
    }

    pub fn is_channel(&self) -> bool {
        matches!(self, PeerId::Channel(_))
    }

    pub fn is_fake(&self) -> bool {
        matches!(self, PeerId::Fake(_))
    }

    pub fn as_user(&self) -> Option<UserId> {
        match self {
            PeerId::User(u) => Some(*u),
            _ => None,
        }
    }

    pub fn as_chat(&self) -> Option<ChatId> {
        match self {
            PeerId::Chat(c) => Some(*c),
            _ => None,
        }
    }

    pub fn as_channel(&self) -> Option<ChannelId> {
        match self {
            PeerId::Channel(ch) => Some(*ch),
            _ => None,
        }
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerId::User(u) => write!(f, "peer_{}", u),
            PeerId::Chat(c) => write!(f, "peer_{}", c),
            PeerId::Channel(ch) => write!(f, "peer_{}", ch),
            PeerId::Fake(id) => write!(f, "peer_fake_{}", id),
        }
    }
}

impl FromStr for PeerId {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Some(rest) = s.strip_prefix("peer_user_") {
            let id = rest.parse::<u64>().map_err(|e| e.to_string())?;
            Ok(PeerId::User(UserId(id)))
        } else if let Some(rest) = s.strip_prefix("peer_chat_") {
            let id = rest.parse::<i64>().map_err(|e| e.to_string())?;
            Ok(PeerId::Chat(ChatId(id)))
        } else if let Some(rest) = s.strip_prefix("peer_channel_") {
            let id = rest.parse::<u64>().map_err(|e| e.to_string())?;
            Ok(PeerId::Channel(ChannelId(id)))
        } else if let Some(rest) = s.strip_prefix("peer_fake_") {
            let id = rest.parse::<u64>().map_err(|e| e.to_string())?;
            Ok(PeerId::Fake(id))
        } else if let Ok(raw) = s.parse::<u64>() {
            Ok(PeerId::from_raw(raw))
        } else {
            Err(format!("Invalid PeerId string format: {}", s))
        }
    }
}

/// Composite message identifier `(PeerId, MsgId)` matching upstream `FullMsgId`
/// in `tdesktop/Telegram/SourceFiles/data/data_msg_id.h`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageKey {
    pub peer: PeerId,
    pub msg_id: MsgId,
}

impl MessageKey {
    pub const fn new(peer: PeerId, msg_id: MsgId) -> Self {
        Self { peer, msg_id }
    }
}

impl fmt::Display for MessageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.peer, self.msg_id)
    }
}

impl FromStr for MessageKey {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (peer_str, msg_str) = s
            .split_once(':')
            .ok_or_else(|| format!("Expected peer:msg delimiter in: {}", s))?;
        let peer = PeerId::from_str(peer_str)?;
        let msg_id = MsgId::from_str(msg_str).map_err(|e| e.to_string())?;
        Ok(MessageKey { peer, msg_id })
    }
}

