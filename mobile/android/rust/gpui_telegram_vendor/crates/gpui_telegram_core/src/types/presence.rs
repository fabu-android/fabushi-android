use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Telegram User Status representation supporting both exact timestamps and inexact privacy ranges
/// matching `tdesktop/Telegram/SourceFiles/data/data_lastseen_status.h`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserStatus {
    Online { expires_at: DateTime<Utc> },
    Offline { was_online: Option<DateTime<Utc>> },
    Recently,
    LastWeek,
    LastMonth,
    LongAgo,
    Hidden,
}

impl UserStatus {
    pub fn online(expires_at: DateTime<Utc>) -> Self {
        Self::Online { expires_at }
    }

    pub fn online_for_duration(dur: Duration) -> Self {
        Self::Online {
            expires_at: Utc::now() + dur,
        }
    }

    pub fn offline() -> Self {
        Self::Offline { was_online: None }
    }

    pub fn offline_at(was_online: DateTime<Utc>) -> Self {
        Self::Offline {
            was_online: Some(was_online),
        }
    }

    pub fn is_online(&self, now: DateTime<Utc>) -> bool {
        match self {
            Self::Online { expires_at } => *expires_at > now,
            _ => false,
        }
    }

    pub fn is_hidden(&self) -> bool {
        matches!(self, Self::Hidden)
    }

    pub fn is_recently(&self) -> bool {
        matches!(self, Self::Recently)
    }

    pub fn was_online(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Offline { was_online } => *was_online,
            _ => None,
        }
    }

    pub fn online_expires_at(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Online { expires_at } => Some(*expires_at),
            _ => None,
        }
    }
}

/// 15 distinct typing progress actions matching upstream
/// `tdesktop/Telegram/SourceFiles/api/api_send_progress.h` (enum `SendProgressType`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypingStatus {
    Typing,
    RecordAudio,
    UploadAudio,
    RecordVideo,
    UploadVideo,
    RecordRound,
    UploadRound,
    UploadPhoto,
    UploadDocument,
    ChooseLocation,
    ChooseContact,
    ChooseSticker,
    PlayGame,
    Speaking,
    Cancel,
}

impl TypingStatus {
    pub const fn action_str(&self) -> &'static str {
        match self {
            Self::Typing => "typing",
            Self::RecordAudio => "record_audio",
            Self::UploadAudio => "upload_audio",
            Self::RecordVideo => "record_video",
            Self::UploadVideo => "upload_video",
            Self::RecordRound => "record_round",
            Self::UploadRound => "upload_round",
            Self::UploadPhoto => "upload_photo",
            Self::UploadDocument => "upload_document",
            Self::ChooseLocation => "choose_location",
            Self::ChooseContact => "choose_contact",
            Self::ChooseSticker => "choose_sticker",
            Self::PlayGame => "play_game",
            Self::Speaking => "speaking",
            Self::Cancel => "cancel",
        }
    }

    pub const fn is_cancelling(&self) -> bool {
        matches!(self, Self::Cancel)
    }

    pub const fn is_audio(&self) -> bool {
        matches!(self, Self::RecordAudio | Self::UploadAudio | Self::Speaking)
    }

    pub const fn is_video(&self) -> bool {
        matches!(
            self,
            Self::RecordVideo | Self::UploadVideo | Self::RecordRound | Self::UploadRound
        )
    }

    pub const fn is_upload(&self) -> bool {
        matches!(
            self,
            Self::UploadAudio
                | Self::UploadVideo
                | Self::UploadRound
                | Self::UploadPhoto
                | Self::UploadDocument
        )
    }
}

