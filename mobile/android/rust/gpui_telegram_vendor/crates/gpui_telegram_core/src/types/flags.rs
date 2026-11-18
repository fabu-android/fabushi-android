use serde::{Deserialize, Serialize};

/// Chat / Channel / Group flags matching upstream
/// `tdesktop/Telegram/SourceFiles/data/data_channel.h` (ChannelDataFlag)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChatFlags {
    pub is_group: bool,
    pub is_supergroup: bool,
    pub is_channel: bool,
    pub is_forum: bool,
    pub is_verified: bool,
    pub is_restricted: bool,
    pub is_scam: bool,
    pub is_fake: bool,
    pub no_forwards: bool,
    pub slowmode_enabled: bool,
}

impl ChatFlags {
    pub const BIT_GROUP: u32 = 1 << 0;
    pub const BIT_SUPERGROUP: u32 = 1 << 1;
    pub const BIT_CHANNEL: u32 = 1 << 2;
    pub const BIT_FORUM: u32 = 1 << 3;
    pub const BIT_VERIFIED: u32 = 1 << 4;
    pub const BIT_RESTRICTED: u32 = 1 << 5;
    pub const BIT_SCAM: u32 = 1 << 6;
    pub const BIT_FAKE: u32 = 1 << 7;
    pub const BIT_NO_FORWARDS: u32 = 1 << 8;
    pub const BIT_SLOWMODE_ENABLED: u32 = 1 << 9;

    pub const fn to_bits(&self) -> u32 {
        let mut bits = 0u32;
        if self.is_group {
            bits |= Self::BIT_GROUP;
        }
        if self.is_supergroup {
            bits |= Self::BIT_SUPERGROUP;
        }
        if self.is_channel {
            bits |= Self::BIT_CHANNEL;
        }
        if self.is_forum {
            bits |= Self::BIT_FORUM;
        }
        if self.is_verified {
            bits |= Self::BIT_VERIFIED;
        }
        if self.is_restricted {
            bits |= Self::BIT_RESTRICTED;
        }
        if self.is_scam {
            bits |= Self::BIT_SCAM;
        }
        if self.is_fake {
            bits |= Self::BIT_FAKE;
        }
        if self.no_forwards {
            bits |= Self::BIT_NO_FORWARDS;
        }
        if self.slowmode_enabled {
            bits |= Self::BIT_SLOWMODE_ENABLED;
        }
        bits
    }

    pub const fn from_bits(bits: u32) -> Self {
        Self {
            is_group: (bits & Self::BIT_GROUP) != 0,
            is_supergroup: (bits & Self::BIT_SUPERGROUP) != 0,
            is_channel: (bits & Self::BIT_CHANNEL) != 0,
            is_forum: (bits & Self::BIT_FORUM) != 0,
            is_verified: (bits & Self::BIT_VERIFIED) != 0,
            is_restricted: (bits & Self::BIT_RESTRICTED) != 0,
            is_scam: (bits & Self::BIT_SCAM) != 0,
            is_fake: (bits & Self::BIT_FAKE) != 0,
            no_forwards: (bits & Self::BIT_NO_FORWARDS) != 0,
            slowmode_enabled: (bits & Self::BIT_SLOWMODE_ENABLED) != 0,
        }
    }
}

/// Message attributes and status flags matching upstream
/// `tdesktop/Telegram/SourceFiles/data/data_types.h` (MessageFlag)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MessageFlags {
    pub is_out: bool,
    pub is_pinned: bool,
    pub is_silent: bool,
    pub is_edited: bool,
    pub is_scheduled: bool,
    pub media_unread: bool,
    pub has_reactions: bool,
    pub no_forwards: bool,
    pub is_forwarded: bool,
}

impl MessageFlags {
    pub const BIT_OUT: u32 = 1 << 0;
    pub const BIT_PINNED: u32 = 1 << 1;
    pub const BIT_SILENT: u32 = 1 << 2;
    pub const BIT_EDITED: u32 = 1 << 3;
    pub const BIT_SCHEDULED: u32 = 1 << 4;
    pub const BIT_MEDIA_UNREAD: u32 = 1 << 5;
    pub const BIT_HAS_REACTIONS: u32 = 1 << 6;
    pub const BIT_NO_FORWARDS: u32 = 1 << 7;
    pub const BIT_IS_FORWARDED: u32 = 1 << 8;

    pub const fn to_bits(&self) -> u32 {
        let mut bits = 0u32;
        if self.is_out {
            bits |= Self::BIT_OUT;
        }
        if self.is_pinned {
            bits |= Self::BIT_PINNED;
        }
        if self.is_silent {
            bits |= Self::BIT_SILENT;
        }
        if self.is_edited {
            bits |= Self::BIT_EDITED;
        }
        if self.is_scheduled {
            bits |= Self::BIT_SCHEDULED;
        }
        if self.media_unread {
            bits |= Self::BIT_MEDIA_UNREAD;
        }
        if self.has_reactions {
            bits |= Self::BIT_HAS_REACTIONS;
        }
        if self.no_forwards {
            bits |= Self::BIT_NO_FORWARDS;
        }
        if self.is_forwarded {
            bits |= Self::BIT_IS_FORWARDED;
        }
        bits
    }

    pub const fn from_bits(bits: u32) -> Self {
        Self {
            is_out: (bits & Self::BIT_OUT) != 0,
            is_pinned: (bits & Self::BIT_PINNED) != 0,
            is_silent: (bits & Self::BIT_SILENT) != 0,
            is_edited: (bits & Self::BIT_EDITED) != 0,
            is_scheduled: (bits & Self::BIT_SCHEDULED) != 0,
            media_unread: (bits & Self::BIT_MEDIA_UNREAD) != 0,
            has_reactions: (bits & Self::BIT_HAS_REACTIONS) != 0,
            no_forwards: (bits & Self::BIT_NO_FORWARDS) != 0,
            is_forwarded: (bits & Self::BIT_IS_FORWARDED) != 0,
        }
    }
}

/// User Privacy Settings Flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PrivacyFlags {
    pub hide_last_seen: bool,
    pub hide_phone_number: bool,
    pub allow_calls_from_contacts_only: bool,
    pub allow_voice_messages: bool,
    pub hide_profile_photo: bool,
    pub no_forward_link: bool,
}

impl PrivacyFlags {
    pub const BIT_HIDE_LAST_SEEN: u32 = 1 << 0;
    pub const BIT_HIDE_PHONE_NUMBER: u32 = 1 << 1;
    pub const BIT_ALLOW_CALLS_CONTACTS_ONLY: u32 = 1 << 2;
    pub const BIT_ALLOW_VOICE_MESSAGES: u32 = 1 << 3;
    pub const BIT_HIDE_PROFILE_PHOTO: u32 = 1 << 4;
    pub const BIT_NO_FORWARD_LINK: u32 = 1 << 5;

    pub const fn to_bits(&self) -> u32 {
        let mut bits = 0u32;
        if self.hide_last_seen {
            bits |= Self::BIT_HIDE_LAST_SEEN;
        }
        if self.hide_phone_number {
            bits |= Self::BIT_HIDE_PHONE_NUMBER;
        }
        if self.allow_calls_from_contacts_only {
            bits |= Self::BIT_ALLOW_CALLS_CONTACTS_ONLY;
        }
        if self.allow_voice_messages {
            bits |= Self::BIT_ALLOW_VOICE_MESSAGES;
        }
        if self.hide_profile_photo {
            bits |= Self::BIT_HIDE_PROFILE_PHOTO;
        }
        if self.no_forward_link {
            bits |= Self::BIT_NO_FORWARD_LINK;
        }
        bits
    }

    pub const fn from_bits(bits: u32) -> Self {
        Self {
            hide_last_seen: (bits & Self::BIT_HIDE_LAST_SEEN) != 0,
            hide_phone_number: (bits & Self::BIT_HIDE_PHONE_NUMBER) != 0,
            allow_calls_from_contacts_only: (bits & Self::BIT_ALLOW_CALLS_CONTACTS_ONLY) != 0,
            allow_voice_messages: (bits & Self::BIT_ALLOW_VOICE_MESSAGES) != 0,
            hide_profile_photo: (bits & Self::BIT_HIDE_PROFILE_PHOTO) != 0,
            no_forward_link: (bits & Self::BIT_NO_FORWARD_LINK) != 0,
        }
    }
}

