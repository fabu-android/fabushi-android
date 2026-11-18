use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallInviteRequest {
    pub callee_id: u64,
    #[serde(default)]
    pub is_video: bool,
    #[serde(default)]
    pub chat_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallSignalRequest {
    pub call_id: String,
    pub signal_type: String, // "offer" | "answer" | "ice_candidate" | "ringing" | "accept" | "hangup"
    #[serde(default)]
    pub sdp: Option<String>,
    #[serde(default)]
    pub candidate: Option<String>,
    #[serde(default)]
    pub sdp_mid: Option<String>,
    #[serde(default)]
    pub sdp_mline_index: Option<u32>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallRecordDto {
    pub call_id: String,
    pub caller_id: u64,
    pub callee_id: u64,
    pub is_video: bool,
    pub state: String,
    #[serde(default)]
    pub duration_secs: u32,
    #[serde(default)]
    pub rating: Option<u8>,
    pub started_at_ms: i64,
    #[serde(default)]
    pub ended_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHitDto {
    pub chat_id: i64,
    pub message_id: i64,
    pub sender_id: u64,
    pub date_ms: i64,
    pub text: String,
    #[serde(default)]
    pub snippet_html: String,
    #[serde(default)]
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub total: usize,
    pub hits: Vec<SearchHitDto>,
    #[serde(default)]
    pub results: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyStatPoint {
    pub day: String,
    pub timestamp_ms: i64,
    pub messages: u64,
    pub reactions: u64,
    pub active_senders: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelStatisticsDto {
    pub chat_id: i64,
    pub title: String,
    pub member_count: u64,
    pub total_messages: u64,
    pub messages_last_7d: u64,
    pub total_reactions: u64,
    pub active_posters_7d: u64,
    pub growth_rate_pct: f64,
    pub daily_series: Vec<DailyStatPoint>,
    #[serde(default)]
    pub top_emojis: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletBalanceDto {
    pub user_id: u64,
    pub stars_balance: i64,
    pub ton_nano: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletTransactionDto {
    pub tx_id: String,
    pub kind: String, // "topup" | "gift_sent" | "gift_received" | "transfer"
    pub amount_stars: i64,
    #[serde(default)]
    pub peer_user_id: Option<u64>,
    #[serde(default)]
    pub description: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarGiftRequest {
    pub to_user_id: u64,
    #[serde(default)]
    pub chat_id: Option<i64>,
    pub amount_stars: i64,
    #[serde(default)]
    pub gift_id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}
