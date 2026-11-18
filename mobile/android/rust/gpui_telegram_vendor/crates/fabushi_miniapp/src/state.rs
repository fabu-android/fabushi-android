use crate::achievements::{Achievement, AchievementSystem};
use crate::countries::{all_countries, country_by_code, country_name_zh, search_countries, CountryNode};
use crate::scene_math::{
    render_home_scene, render_zen_scene, CanvasDrawCmd, GlobeSceneState, ZenSceneState,
};
use crate::sutras::{
    all_dharma_packages, all_sutras, dharma_package_by_key, format_duration_zh, format_file_size,
    format_leaderboard_bytes, sutra_by_title, DharmaPackage, Sutra,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AppTab {
    #[default]
    Home,
    ZenRoom,
    Profile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    #[default]
    TwoD,
    ThreeD,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LocaleCode {
    #[default]
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "en")]
    En,
}

impl LocaleCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::En => "en",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        let low = s.trim().to_ascii_lowercase();
        if low.starts_with("en") {
            Self::En
        } else {
            Self::ZhCn
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiChatMessage {
    pub id: String,
    pub text: String,
    pub is_user: bool,
    pub is_error: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPractice {
    pub title: String,
    pub category: String,
    pub file_path: String,
    pub selected_at_ms: i64,
    pub is_locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedDharmaContent {
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub preview_text: String,
    pub source_url: Option<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardEntryDto {
    pub user_id: i64,
    pub username: String,
    pub display_name: String,
    pub avatar: Option<String>,
    pub total_bytes: u64,
    pub total_bytes_formatted: String,
    pub total_records: u64,
    pub total_count: u64,
    pub total_duration: u64,
    pub total_days: u64,
    pub latest_sutra: Option<String>,
    pub latest_record_date: Option<String>,
    pub rank: u32,
    pub follower_count: u64,
    pub following_count: u64,
    pub is_following: bool,
    pub is_self: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PracticeRecordDto {
    pub id: i64,
    pub sutra_name: String,
    pub sutra_source: String,
    pub duration_minutes: u32,
    pub chant_count: u32,
    pub record_date: String,
    pub local_time: String,
    pub is_manual: bool,
    pub notes: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SubjectStatsDto {
    pub sutra_name: String,
    pub count: u64,
    pub duration_minutes: u64,
    pub days: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PracticeStatsDto {
    pub today_sutra: Option<String>,
    pub today_count: u64,
    pub today_duration_minutes: u64,
    pub total_records: u64,
    pub total_count: u64,
    pub total_duration_minutes: u64,
    pub total_days: u64,
    pub consecutive_days: u64,
    pub by_subject: Vec<SubjectStatsDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserProfileDto {
    pub user_id: i64,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub bio: String,
    pub user_no: i64,
    pub is_vip: bool,
    pub vip_plan: String,
    pub vip_expires_at: Option<String>,
    pub is_admin: bool,
    pub locale: String,
    pub total_transfer_bytes: u64,
    pub total_transfer_count: u64,
    pub total_practice_sessions: u64,
    pub total_practice_minutes: u64,
    pub total_practice_chants: u64,
    pub consecutive_days: u64,
    pub locked_practice_title: Option<String>,
}

impl Default for UserProfileDto {
    fn default() -> Self {
        Self {
            user_id: 0,
            username: "guest".to_string(),
            display_name: "修行行者".to_string(),
            avatar_url: String::new(),
            bio: String::new(),
            user_no: 10001,
            is_vip: false,
            vip_plan: "free".to_string(),
            vip_expires_at: None,
            is_admin: false,
            locale: "zh-CN".to_string(),
            total_transfer_bytes: 0,
            total_transfer_count: 0,
            total_practice_sessions: 0,
            total_practice_minutes: 0,
            total_practice_chants: 0,
            consecutive_days: 0,
            locked_practice_title: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipPlanInfo {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub price: String,
    pub duration_label: String,
    pub duration_days: u32,
    pub recommended: bool,
    pub features: Vec<String>,
}

pub fn membership_plans() -> Vec<MembershipPlanInfo> {
    vec![
        MembershipPlanInfo {
            id: "monthly".to_string(),
            name: "月度会员".to_string(),
            name_en: "Monthly VIP".to_string(),
            price: "¥21.00".to_string(),
            duration_label: "30天".to_string(),
            duration_days: 30,
            recommended: false,
            features: vec![
                "解锁 3D 地球与 3D 禅室佛像".to_string(),
                "开启本地转经轮独立回环".to_string(),
                "全套功德统计与云端备份".to_string(),
            ],
        },
        MembershipPlanInfo {
            id: "quarterly".to_string(),
            name: "季度会员".to_string(),
            name_en: "Quarterly VIP".to_string(),
            price: "¥63.00".to_string(),
            duration_label: "90天".to_string(),
            duration_days: 90,
            recommended: false,
            features: vec![
                "解锁 3D 地球与 3D 禅室佛像".to_string(),
                "开启本地转经轮独立回环".to_string(),
                "优先支持与全球同步".to_string(),
            ],
        },
        MembershipPlanInfo {
            id: "yearly".to_string(),
            name: "年度会员".to_string(),
            name_en: "Annual VIP".to_string(),
            price: "¥252.00".to_string(),
            duration_label: "365天".to_string(),
            duration_days: 365,
            recommended: true,
            features: vec![
                "解锁 3D 地球与 3D 禅室佛像".to_string(),
                "开启本地转经轮独立回环".to_string(),
                "全年无限制法布施与护持".to_string(),
            ],
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingReflectionSession {
    pub sutra: String,
    pub duration_seconds: u64,
    pub duration_minutes: u32,
    pub chant_count: u32,
    pub start_time_ms: i64,
    pub end_time_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedStrings {
    pub app_title: String,
    pub tab_home: String,
    pub tab_zen: String,
    pub tab_profile: String,
    pub online_global_prefix: String,
    pub ask_ai_hint: String,
    pub dharma_hint: String,
    pub global_pudu: String,
    pub leaderboard_title: String,
    pub start_practice: String,
    pub end_practice: String,
    pub tap_to_count: String,
    pub chant_unit: String,
    pub circumambulate: String,
    pub select_practice_title: String,
    pub practice_records_title: String,
    pub membership_title: String,
    pub edit_profile: String,
}

impl LocalizedStrings {
    pub fn for_locale(locale: LocaleCode) -> Self {
        match locale {
            LocaleCode::ZhCn => Self {
                app_title: "全球法布施".to_string(),
                tab_home: "首页".to_string(),
                tab_zen: "禅室".to_string(),
                tab_profile: "我的".to_string(),
                online_global_prefix: "🌍 正在全球发送:".to_string(),
                ask_ai_hint: "问问 AI".to_string(),
                dharma_hint: "输入法布施文字或链接".to_string(),
                global_pudu: "全球普渡".to_string(),
                leaderboard_title: "全球布施排行榜".to_string(),
                start_practice: "开始修行".to_string(),
                end_practice: "结束修行".to_string(),
                tap_to_count: "点击屏幕计数".to_string(),
                chant_unit: "遍".to_string(),
                circumambulate: "绕佛".to_string(),
                select_practice_title: "一门深入 · 选择主修功课".to_string(),
                practice_records_title: "修行记录".to_string(),
                membership_title: "法布施会员".to_string(),
                edit_profile: "编辑资料".to_string(),
            },
            LocaleCode::En => Self {
                app_title: "Global Dharma".to_string(),
                tab_home: "Home".to_string(),
                tab_zen: "Zen Room".to_string(),
                tab_profile: "Profile".to_string(),
                online_global_prefix: "🌍 Sending globally:".to_string(),
                ask_ai_hint: "Ask Dharma AI".to_string(),
                dharma_hint: "Enter Dharma text or link".to_string(),
                global_pudu: "Global Dharma".to_string(),
                leaderboard_title: "Global Offering Leaderboard".to_string(),
                start_practice: "Start Practice".to_string(),
                end_practice: "End Practice".to_string(),
                tap_to_count: "Tap anywhere to count".to_string(),
                chant_unit: "chants".to_string(),
                circumambulate: "Circumambulate".to_string(),
                select_practice_title: "One Practice · Lock Main Sutra".to_string(),
                practice_records_title: "Practice Records".to_string(),
                membership_title: "Dharma VIP".to_string(),
                edit_profile: "Edit Profile".to_string(),
            },
        }
    }
}

/// All UI commands dispatched from canvas/DOM into the Rust state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiCommand {
    InitBootstrap,
    SelectTab { tab: AppTab },
    SetLocale { locale: LocaleCode },
    SelectHomeRenderMode { mode: RenderMode },
    SelectZenRenderMode { mode: RenderMode },
    DragScene { dx: f64, dy: f64 },
    SetComposerText { text: String },
    TogglePlusMenu { open: Option<bool> },
    ActivateDharmaMode,
    ClearDharmaMode,
    SelectDharmaSutra { title: String },
    SelectDharmaPackage { key: String },
    AttachCustomFile { name: String, size_bytes: u64, preview_text: Option<String> },
    ClearSelectedContent,
    ToggleLooping,
    OpenRegionSelector { open: bool },
    SetRegionSearchQuery { query: String },
    ToggleRegionAll { selected: bool },
    ToggleRegionCountry { code: String },
    SetFieldEnergyMode { enabled: bool },
    SetLocalLoopbackEnabled { enabled: bool },
    ApplyRegionSelection {
        country_codes: Vec<String>,
        field_energy: bool,
        local_loopback: bool,
    },
    SubmitComposer,
    StopTransfer,
    StopAiGeneration,
    OpenLeaderboard { open: bool },
    RefreshLeaderboard,
    SelectLeaderboardUser { username: Option<String> },
    ToggleFollow { target_username: String },
    OpenPracticeLeaderboard { open: bool },
    OpenPracticeSelector { open: bool },
    SelectAndLockPractice { title: String },
    OpenSutraReader { open: bool, title: Option<String> },
    StartPracticeSession,
    PausePracticeSession,
    ResumePracticeSession,
    IncrementChant,
    UndoChant,
    ToggleCircumambulation,
    EndPracticeSession,
    SaveReflection { notes: String },
    SkipReflection,
    DismissAchievementPopup,
    OpenPracticeRecords { open: bool },
    AddPracticeRecord {
        sutra_name: String,
        chant_count: u32,
        duration_minutes: u32,
        record_date: String,
        local_time: String,
        notes: Option<String>,
    },
    UpdatePracticeRecord {
        id: i64,
        sutra_name: String,
        chant_count: u32,
        duration_minutes: u32,
        record_date: String,
        local_time: String,
        notes: Option<String>,
    },
    DeletePracticeRecord { id: i64 },
    OpenMembership { open: bool },
    UnlockMembership { plan: String },
    OpenEditProfile { open: bool },
    UpdateProfile { display_name: String, bio: String },
    ClearToast,
}

/// Outbound HTTP & Telegram WebApp bridge effects requested by the state machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HttpActionEffect {
    Bootstrap,
    RecordTransfer {
        content_title: String,
        content_kind: String,
        target_countries: Vec<String>,
        bytes_sent: u64,
        beam_count: u32,
    },
    SavePracticeSession {
        sutra: String,
        duration_minutes: u32,
        chant_count: u32,
        start_time_ms: i64,
        end_time_ms: i64,
        notes: Option<String>,
    },
    CreatePracticeRecord {
        sutra_name: String,
        chant_count: u32,
        duration_minutes: u32,
        record_date: String,
        local_time: String,
        notes: Option<String>,
    },
    UpdatePracticeRecord {
        id: i64,
        sutra_name: String,
        chant_count: u32,
        duration_minutes: u32,
        record_date: String,
        local_time: String,
        notes: Option<String>,
    },
    DeletePracticeRecord {
        id: i64,
    },
    UpdateProfile {
        display_name: Option<String>,
        bio: Option<String>,
        locked_practice: Option<LockedPractice>,
        locale: Option<String>,
    },
    ActivateMembership {
        plan: String,
    },
    ToggleFollow {
        target_username: String,
    },
    FetchLeaderboard {
        kind: String,
    },
    Heartbeat {
        activity_type: String,
    },
    SendAiChat {
        message: String,
        history: Vec<AiChatMessage>,
    },
    SyncCloudStorage {
        key: String,
        value: String,
    },
    EmitWebAppData {
        data: String,
    },
}

/// Inbound HTTP response payloads fed back via `apply_http_effect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HttpResponsePayload {
    Bootstrap {
        profile: UserProfileDto,
        #[serde(default)]
        locked_practice: Option<LockedPractice>,
        #[serde(default)]
        leaderboard: Vec<LeaderboardEntryDto>,
        #[serde(default)]
        practice_leaderboard: Vec<LeaderboardEntryDto>,
        #[serde(default)]
        practice_records: Vec<PracticeRecordDto>,
        #[serde(default)]
        practice_stats: PracticeStatsDto,
        #[serde(default)]
        unlocked_achievement_ids: Vec<String>,
        #[serde(default)]
        online_global_sending_count: u64,
        #[serde(default)]
        online_zen_room_count: u64,
    },
    TransferRecorded {
        total_transfer_bytes: u64,
        total_transfer_count: u64,
        #[serde(default)]
        leaderboard: Option<Vec<LeaderboardEntryDto>>,
    },
    PracticeSessionSaved {
        record: PracticeRecordDto,
        practice_stats: PracticeStatsDto,
        profile: UserProfileDto,
    },
    PracticeRecordsRefreshed {
        practice_records: Vec<PracticeRecordDto>,
        practice_stats: PracticeStatsDto,
    },
    ProfileUpdated {
        profile: UserProfileDto,
        #[serde(default)]
        locked_practice: Option<LockedPractice>,
    },
    MembershipActivated {
        profile: UserProfileDto,
    },
    FollowToggled {
        target_username: String,
        is_following: bool,
        follower_count: u64,
    },
    LeaderboardFetched {
        kind: String,
        entries: Vec<LeaderboardEntryDto>,
    },
    HeartbeatAck {
        activity_type: String,
        online_count: u64,
    },
    AiChatReply {
        reply: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryOptionView {
    pub code: String,
    pub name_zh: String,
    pub name_en: String,
    pub continent: String,
    pub selected: bool,
}

/// Full immutable UI snapshot serialized across WASM/DOM boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewModelSnapshot {
    pub active_tab: AppTab,
    pub locale: LocaleCode,
    pub strings: LocalizedStrings,
    pub home_render_mode: RenderMode,
    pub zen_render_mode: RenderMode,
    pub can_use_3d: bool,
    pub online_global_sending_count: u64,
    pub online_zen_room_count: u64,
    pub is_dharma_composer_mode: bool,
    pub is_plus_menu_open: bool,
    pub composer_text: String,
    pub is_ai_generating: bool,
    pub ai_messages: Vec<AiChatMessage>,
    pub region_summary: String,
    pub country_list: Vec<String>,
    pub is_global_send_enabled: bool,
    pub is_looping: bool,
    pub is_field_energy_mode: bool,
    pub is_local_loopback_enabled: bool,
    pub selected_content: Option<SelectedDharmaContent>,
    pub is_preparing_send: bool,
    pub is_transferring: bool,
    pub current_sending_scripture: String,
    pub current_sending_country: String,
    pub current_country_progress: f64,
    pub is_current_material_completed: bool,
    pub global_sent_count: u64,
    pub global_data_sent_formatted: String,
    pub loop_count: u64,
    pub loopback_count: u64,
    pub field_broadcast_count: u64,
    pub is_region_selector_open: bool,
    pub region_search_query: String,
    pub region_countries: Vec<CountryOptionView>,
    pub is_leaderboard_open: bool,
    pub leaderboard: Vec<LeaderboardEntryDto>,
    pub selected_leaderboard_entry: Option<LeaderboardEntryDto>,
    pub is_practice_leaderboard_open: bool,
    pub practice_leaderboard: Vec<LeaderboardEntryDto>,
    pub locked_practice: Option<LockedPractice>,
    pub is_practice_selector_open: bool,
    pub is_sutra_reader_open: bool,
    pub reader_title: String,
    pub reader_category: String,
    pub reader_content: String,
    pub is_in_session: bool,
    pub is_paused: bool,
    pub session_elapsed_secs: u64,
    pub session_duration_formatted: String,
    pub chant_count: u32,
    pub can_undo_chant: bool,
    pub is_circumambulating: bool,
    pub incense_progress: f64,
    pub is_reflection_open: bool,
    pub pending_reflection: Option<PendingReflectionSession>,
    pub recent_unlocked_achievement: Option<Achievement>,
    pub achievements: Vec<Achievement>,
    pub profile: UserProfileDto,
    pub is_practice_records_open: bool,
    pub practice_records: Vec<PracticeRecordDto>,
    pub practice_stats: PracticeStatsDto,
    pub is_membership_open: bool,
    pub membership_plans: Vec<MembershipPlanInfo>,
    pub is_edit_profile_open: bool,
    pub toast_message: Option<String>,
    pub sutras: Vec<Sutra>,
    pub dharma_packages: Vec<DharmaPackage>,
    pub globe_rot_lon_deg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchResult {
    pub snapshot: ViewModelSnapshot,
    pub draw_commands: Vec<CanvasDrawCmd>,
    pub effects: Vec<HttpActionEffect>,
}

#[derive(Debug, Clone)]
struct TransferTarget {
    #[allow(dead_code)]
    code: String,
    name_zh: String,
    lat: f64,
    lng: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabushiClientState {
    pub active_tab: AppTab,
    pub locale: LocaleCode,
    pub home_render_mode: RenderMode,
    pub zen_render_mode: RenderMode,
    pub user_lat: f64,
    pub user_lng: f64,
    pub tz_offset_minutes: i32,
    pub now_ms: f64,
    pub last_viewport_w: f64,
    pub last_viewport_h: f64,

    // Online counters
    pub online_global_sending_count: u64,
    pub online_zen_room_count: u64,

    // Home composer & AI chat
    pub is_dharma_composer_mode: bool,
    pub is_plus_menu_open: bool,
    pub composer_text: String,
    pub is_ai_generating: bool,
    pub ai_messages: Vec<AiChatMessage>,
    pub country_list: Vec<String>,
    pub is_global_send_enabled: bool,
    pub is_looping: bool,
    pub is_field_energy_mode: bool,
    pub is_local_loopback_enabled: bool,
    pub selected_content: Option<SelectedDharmaContent>,

    // Active global transfer
    pub is_preparing_send: bool,
    pub is_transferring: bool,
    pub current_sending_scripture: String,
    pub current_sending_country: String,
    pub current_country_progress: f64,
    pub is_current_material_completed: bool,
    pub global_sent_count: u64,
    pub global_data_sent_bytes: u64,
    pub loop_count: u64,
    pub loopback_count: u64,
    pub field_broadcast_count: u64,
    #[serde(skip)]
    transfer_targets: Vec<TransferTarget>,
    transfer_target_idx: usize,
    transfer_hop_elapsed_ms: f64,
    transfer_bytes_per_hop: u64,
    loopback_accum_ms: f64,
    field_accum_ms: f64,

    // Modals & sheets
    pub is_region_selector_open: bool,
    pub region_search_query: String,
    pub is_leaderboard_open: bool,
    pub leaderboard: Vec<LeaderboardEntryDto>,
    pub selected_leaderboard_username: Option<String>,
    pub is_practice_leaderboard_open: bool,
    pub practice_leaderboard: Vec<LeaderboardEntryDto>,

    // Zen Room
    pub locked_practice: Option<LockedPractice>,
    pub is_practice_selector_open: bool,
    pub is_sutra_reader_open: bool,
    pub reader_title: String,
    pub reader_category: String,
    pub reader_content: String,
    pub is_in_session: bool,
    pub is_paused: bool,
    pub session_elapsed_secs: f64,
    pub session_start_ms: i64,
    pub chant_count: u32,
    pub undo_stack_depth: u32,
    pub is_circumambulating: bool,
    pub is_reflection_open: bool,
    pub pending_reflection: Option<PendingReflectionSession>,
    pub achievement_system: AchievementSystem,
    pub recent_unlocked_achievement: Option<Achievement>,

    // Profile, Records & VIP
    pub profile: UserProfileDto,
    pub is_practice_records_open: bool,
    pub practice_records: Vec<PracticeRecordDto>,
    pub practice_stats: PracticeStatsDto,
    pub is_membership_open: bool,
    pub is_edit_profile_open: bool,
    pub toast_message: Option<String>,

    // Scene states
    pub globe_scene: GlobeSceneState,
    pub zen_scene: ZenSceneState,
}

impl Default for FabushiClientState {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl FabushiClientState {
    pub fn new(initial_now_ms: f64) -> Self {
        Self {
            active_tab: AppTab::Home,
            locale: LocaleCode::ZhCn,
            home_render_mode: RenderMode::TwoD,
            zen_render_mode: RenderMode::TwoD,
            user_lat: 39.916667,
            user_lng: 116.383333,
            tz_offset_minutes: 480,
            now_ms: initial_now_ms,
            last_viewport_w: 420.0,
            last_viewport_h: 780.0,
            online_global_sending_count: 1,
            online_zen_room_count: 1,
            is_dharma_composer_mode: false,
            is_plus_menu_open: false,
            composer_text: String::new(),
            is_ai_generating: false,
            ai_messages: Vec::new(),
            country_list: vec!["ALL".to_string()],
            is_global_send_enabled: true,
            is_looping: false,
            is_field_energy_mode: false,
            is_local_loopback_enabled: false,
            selected_content: None,
            is_preparing_send: false,
            is_transferring: false,
            current_sending_scripture: String::new(),
            current_sending_country: String::new(),
            current_country_progress: 0.0,
            is_current_material_completed: false,
            global_sent_count: 0,
            global_data_sent_bytes: 0,
            loop_count: 0,
            loopback_count: 0,
            field_broadcast_count: 0,
            transfer_targets: Vec::new(),
            transfer_target_idx: 0,
            transfer_hop_elapsed_ms: 0.0,
            transfer_bytes_per_hop: 0,
            loopback_accum_ms: 0.0,
            field_accum_ms: 0.0,
            is_region_selector_open: false,
            region_search_query: String::new(),
            is_leaderboard_open: false,
            leaderboard: Vec::new(),
            selected_leaderboard_username: None,
            is_practice_leaderboard_open: false,
            practice_leaderboard: Vec::new(),
            locked_practice: None,
            is_practice_selector_open: false,
            is_sutra_reader_open: false,
            reader_title: String::new(),
            reader_category: String::new(),
            reader_content: String::new(),
            is_in_session: false,
            is_paused: false,
            session_elapsed_secs: 0.0,
            session_start_ms: 0,
            chant_count: 0,
            undo_stack_depth: 0,
            is_circumambulating: false,
            is_reflection_open: false,
            pending_reflection: None,
            achievement_system: AchievementSystem::new(),
            recent_unlocked_achievement: None,
            profile: UserProfileDto::default(),
            is_practice_records_open: false,
            practice_records: Vec::new(),
            practice_stats: PracticeStatsDto::default(),
            is_membership_open: false,
            is_edit_profile_open: false,
            toast_message: None,
            globe_scene: GlobeSceneState::default(),
            zen_scene: ZenSceneState::default(),
        }
    }

    pub fn can_use_3d(&self) -> bool {
        self.profile.is_vip || self.profile.is_admin
    }

    pub fn set_initial_tab_from_query(&mut self, tab: &str) {
        self.active_tab = match tab.trim().to_ascii_lowercase().as_str() {
            "meditation" | "meditation-room" | "zen" => AppTab::ZenRoom,
            "profile" | "me" | "mine" => AppTab::Profile,
            _ => AppTab::Home,
        };
    }

    pub fn region_summary(&self) -> String {
        let mut labels = Vec::new();
        if self.is_global_send_enabled && !self.country_list.is_empty() {
            if self.country_list.iter().any(|c| c == "ALL") {
                labels.push(if self.locale == LocaleCode::En {
                    "Global".to_string()
                } else {
                    "全球".to_string()
                });
            } else {
                let names: Vec<String> = self
                    .country_list
                    .iter()
                    .filter_map(|code| country_by_code(code).map(|c| c.name_zh.clone()))
                    .collect();
                if names.len() <= 2 && !names.is_empty() {
                    labels.push(names.join("、"));
                } else if !names.is_empty() {
                    labels.push(format!("{} 个国家", names.len()));
                }
            }
        }
        if self.is_field_energy_mode {
            labels.push("本地场能".to_string());
        }
        if self.is_local_loopback_enabled {
            labels.push("本地转经轮".to_string());
        }
        if labels.is_empty() {
            "未选择".to_string()
        } else {
            labels.join("、")
        }
    }

    pub fn dispatch(&mut self, cmd: UiCommand) -> DispatchResult {
        let mut effects = Vec::new();
        match cmd {
            UiCommand::InitBootstrap => {
                effects.push(HttpActionEffect::Bootstrap);
                effects.push(HttpActionEffect::Heartbeat {
                    activity_type: "global_sending".to_string(),
                });
            }
            UiCommand::SelectTab { tab } => {
                self.active_tab = tab;
                self.is_plus_menu_open = false;
                if tab == AppTab::ZenRoom {
                    if self.is_in_session && self.is_paused {
                        self.is_paused = false;
                    }
                    effects.push(HttpActionEffect::Heartbeat {
                        activity_type: "zen_room".to_string(),
                    });
                } else {
                    if self.is_in_session && !self.is_paused {
                        self.is_paused = true;
                    }
                    if tab == AppTab::Home {
                        effects.push(HttpActionEffect::Heartbeat {
                            activity_type: "global_sending".to_string(),
                        });
                    }
                }
            }
            UiCommand::SetLocale { locale } => {
                self.locale = locale;
                self.profile.locale = locale.as_str().to_string();
                effects.push(HttpActionEffect::UpdateProfile {
                    display_name: None,
                    bio: None,
                    locked_practice: None,
                    locale: Some(self.profile.locale.clone()),
                });
            }
            UiCommand::SelectHomeRenderMode { mode } => {
                if mode == RenderMode::ThreeD && !self.can_use_3d() {
                    self.home_render_mode = RenderMode::TwoD;
                    self.toast_message =
                        Some("3D 模式为有效会员专享，当前已切换为高性能 2D。".to_string());
                    self.is_membership_open = true;
                } else {
                    self.home_render_mode = mode;
                    self.globe_scene.tilt_lat_deg = if mode == RenderMode::ThreeD {
                        12.0
                    } else {
                        0.0
                    };
                }
            }
            UiCommand::SelectZenRenderMode { mode } => {
                if mode == RenderMode::ThreeD && !self.can_use_3d() {
                    self.zen_render_mode = RenderMode::TwoD;
                    self.toast_message =
                        Some("3D 模式为有效会员专享，当前已切换为高性能 2D。".to_string());
                    self.is_membership_open = true;
                } else {
                    self.zen_render_mode = mode;
                }
            }
            UiCommand::DragScene { dx, dy } => match self.active_tab {
                AppTab::Home => {
                    let is_3d = self.home_render_mode == RenderMode::ThreeD && self.can_use_3d();
                    self.globe_scene.drag(dx, dy, is_3d);
                }
                AppTab::ZenRoom => {
                    self.zen_scene.drag(dx, dy);
                }
                AppTab::Profile => {}
            },
            UiCommand::SetComposerText { text } => {
                self.composer_text = text;
            }
            UiCommand::TogglePlusMenu { open } => {
                self.is_plus_menu_open = open.unwrap_or(!self.is_plus_menu_open);
            }
            UiCommand::ActivateDharmaMode => {
                self.is_plus_menu_open = false;
                if self.country_list.is_empty() {
                    self.country_list = vec!["ALL".to_string()];
                }
                if !self.is_global_send_enabled
                    && !self.is_field_energy_mode
                    && !self.is_local_loopback_enabled
                {
                    self.is_global_send_enabled = true;
                    self.country_list = vec!["ALL".to_string()];
                }
                self.is_dharma_composer_mode = true;
            }
            UiCommand::ClearDharmaMode => {
                self.is_dharma_composer_mode = false;
                self.composer_text.clear();
                self.selected_content = None;
            }
            UiCommand::SelectDharmaSutra { title } => {
                self.is_plus_menu_open = false;
                self.is_dharma_composer_mode = true;
                if let Some(sutra) = sutra_by_title(&title) {
                    let bytes = sutra.content.len() as u64;
                    self.selected_content = Some(SelectedDharmaContent {
                        kind: "佛典经文".to_string(),
                        title: sutra.title.clone(),
                        subtitle: format!("{} · {}", sutra.category, format_file_size(bytes)),
                        preview_text: sutra.content.clone(),
                        source_url: None,
                        size_bytes: bytes,
                    });
                }
            }
            UiCommand::SelectDharmaPackage { key } => {
                self.is_plus_menu_open = false;
                self.is_dharma_composer_mode = true;
                if let Some(pkg) = dharma_package_by_key(&key) {
                    self.selected_content = Some(SelectedDharmaContent {
                        kind: "素材文件".to_string(),
                        title: pkg.display_name.clone(),
                        subtitle: format!(
                            "{} · R2法宝",
                            format_leaderboard_bytes(pkg.size_bytes)
                        ),
                        preview_text: format!(
                            "法宝包: {}\n对象键: {}\n大小: {}",
                            pkg.display_name,
                            pkg.key,
                            format_leaderboard_bytes(pkg.size_bytes)
                        ),
                        source_url: None,
                        size_bytes: pkg.size_bytes,
                    });
                }
            }
            UiCommand::AttachCustomFile {
                name,
                size_bytes,
                preview_text,
            } => {
                self.is_plus_menu_open = false;
                self.is_dharma_composer_mode = true;
                let preview = preview_text.unwrap_or_else(|| {
                    format!("文件名: {name}\n大小: {}", format_file_size(size_bytes))
                });
                self.selected_content = Some(SelectedDharmaContent {
                    kind: "本机文件".to_string(),
                    title: name,
                    subtitle: format_file_size(size_bytes),
                    preview_text: preview,
                    source_url: None,
                    size_bytes: size_bytes.max(256),
                });
            }
            UiCommand::ClearSelectedContent => {
                self.selected_content = None;
            }
            UiCommand::ToggleLooping => {
                self.is_looping = !self.is_looping;
            }
            UiCommand::OpenRegionSelector { open } => {
                self.is_region_selector_open = open;
                if !open {
                    self.region_search_query.clear();
                }
            }
            UiCommand::SetRegionSearchQuery { query } => {
                self.region_search_query = query;
            }
            UiCommand::ToggleRegionAll { selected } => {
                if selected {
                    self.country_list = vec!["ALL".to_string()];
                    self.is_global_send_enabled = true;
                } else {
                    self.country_list.retain(|c| c != "ALL");
                    self.is_global_send_enabled = !self.country_list.is_empty();
                }
            }
            UiCommand::ToggleRegionCountry { code } => {
                let norm = code.trim().to_ascii_uppercase();
                self.country_list.retain(|c| c != "ALL");
                if let Some(pos) = self.country_list.iter().position(|c| c == &norm) {
                    self.country_list.remove(pos);
                } else if country_by_code(&norm).is_some() {
                    self.country_list.push(norm);
                }
                self.is_global_send_enabled = !self.country_list.is_empty();
            }
            UiCommand::SetFieldEnergyMode { enabled } => {
                self.is_field_energy_mode = enabled;
            }
            UiCommand::SetLocalLoopbackEnabled { enabled } => {
                if enabled && !self.can_use_3d() {
                    self.is_local_loopback_enabled = false;
                    self.toast_message =
                        Some("本地回环是会员功能，开通会员后即可开启。".to_string());
                    self.is_membership_open = true;
                } else {
                    self.is_local_loopback_enabled = enabled;
                    if !enabled {
                        self.loopback_count = 0;
                    }
                }
            }
            UiCommand::ApplyRegionSelection {
                country_codes,
                field_energy,
                local_loopback,
            } => {
                let valid_codes: Vec<String> = if country_codes.iter().any(|c| c == "ALL") {
                    vec!["ALL".to_string()]
                } else {
                    country_codes
                        .into_iter()
                        .map(|c| c.trim().to_ascii_uppercase())
                        .filter(|c| country_by_code(c).is_some())
                        .collect()
                };
                self.country_list = valid_codes;
                self.is_global_send_enabled = !self.country_list.is_empty();
                self.is_field_energy_mode = field_energy;
                if local_loopback && !self.can_use_3d() {
                    self.is_local_loopback_enabled = false;
                    self.toast_message =
                        Some("本地回环是会员功能，开通会员后即可开启。".to_string());
                } else {
                    self.is_local_loopback_enabled = local_loopback;
                }
                self.is_region_selector_open = false;
                self.region_search_query.clear();
            }
            UiCommand::SubmitComposer => {
                if self.is_dharma_composer_mode {
                    self.start_dharma_transfer(&mut effects);
                } else {
                    let question = self.composer_text.trim().to_string();
                    if !question.is_empty() && !self.is_ai_generating {
                        self.composer_text.clear();
                        let msg_id = format!("u_{}", self.now_ms as i64);
                        self.ai_messages.push(AiChatMessage {
                            id: msg_id,
                            text: question.clone(),
                            is_user: true,
                            is_error: false,
                            created_at_ms: self.now_ms as i64,
                        });
                        self.is_ai_generating = true;
                        let history = self
                            .ai_messages
                            .iter()
                            .rev()
                            .take(6)
                            .cloned()
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect();
                        effects.push(HttpActionEffect::SendAiChat {
                            message: question,
                            history,
                        });
                    }
                }
            }
            UiCommand::StopTransfer => {
                self.stop_dharma_transfer();
            }
            UiCommand::StopAiGeneration => {
                self.is_ai_generating = false;
            }
            UiCommand::OpenLeaderboard { open } => {
                self.is_leaderboard_open = open;
                if open {
                    effects.push(HttpActionEffect::FetchLeaderboard {
                        kind: "global".to_string(),
                    });
                } else {
                    self.selected_leaderboard_username = None;
                }
            }
            UiCommand::RefreshLeaderboard => {
                effects.push(HttpActionEffect::FetchLeaderboard {
                    kind: "global".to_string(),
                });
            }
            UiCommand::SelectLeaderboardUser { username } => {
                self.selected_leaderboard_username = username;
            }
            UiCommand::ToggleFollow { target_username } => {
                for list in [&mut self.leaderboard, &mut self.practice_leaderboard] {
                    if let Some(entry) = list.iter_mut().find(|e| e.username == target_username && !e.is_self) {
                        entry.is_following = !entry.is_following;
                        if entry.is_following {
                            entry.follower_count = entry.follower_count.saturating_add(1);
                        } else {
                            entry.follower_count = entry.follower_count.saturating_sub(1);
                        }
                    }
                }
                effects.push(HttpActionEffect::ToggleFollow { target_username });
            }
            UiCommand::OpenPracticeLeaderboard { open } => {
                self.is_practice_leaderboard_open = open;
                if open {
                    effects.push(HttpActionEffect::FetchLeaderboard {
                        kind: "practice".to_string(),
                    });
                }
            }
            UiCommand::OpenPracticeSelector { open } => {
                self.is_practice_selector_open = open;
            }
            UiCommand::SelectAndLockPractice { title } => {
                let clean = title.trim().to_string();
                if self.locked_practice.is_some() {
                    self.toast_message = Some("功课已锁定，一门深入，长时熏修".to_string());
                    self.is_practice_selector_open = false;
                } else if !clean.is_empty() {
                    let sutra_opt = sutra_by_title(&clean);
                    let practice = LockedPractice {
                        title: clean.clone(),
                        category: sutra_opt
                            .map(|s| s.category.clone())
                            .unwrap_or_else(|| "自选功课".to_string()),
                        file_path: sutra_opt
                            .map(|s| format!("sutra:{}", s.id))
                            .unwrap_or_else(|| format!("manual:{clean}")),
                        selected_at_ms: self.now_ms as i64,
                        is_locked: true,
                    };
                    self.locked_practice = Some(practice.clone());
                    self.profile.locked_practice_title = Some(clean.clone());
                    self.is_practice_selector_open = false;
                    self.toast_message = Some(format!("已选定功课：{clean}"));
                    effects.push(HttpActionEffect::UpdateProfile {
                        display_name: None,
                        bio: None,
                        locked_practice: Some(practice),
                        locale: None,
                    });
                    self.sync_summary_effect(&mut effects);
                }
            }
            UiCommand::OpenSutraReader { open, title } => {
                self.is_sutra_reader_open = open;
                if open {
                    let target_title = title
                        .or_else(|| self.locked_practice.as_ref().map(|p| p.title.clone()))
                        .unwrap_or_else(|| "金刚经".to_string());
                    if let Some(sutra) = sutra_by_title(&target_title) {
                        self.reader_title = sutra.title.clone();
                        self.reader_category = sutra.category.clone();
                        self.reader_content = sutra.content.clone();
                    } else {
                        self.reader_title = target_title.clone();
                        self.reader_category = "自选功课".to_string();
                        self.reader_content = format!("南无本师释迦牟尼佛 · 专修《{target_title}》");
                    }
                }
            }
            UiCommand::StartPracticeSession => {
                if !self.is_in_session {
                    if self.locked_practice.is_none() {
                        self.toast_message = Some("请先选择一门深入的修行功课".to_string());
                        self.is_practice_selector_open = true;
                    } else {
                        self.is_in_session = true;
                        self.is_paused = false;
                        self.session_elapsed_secs = 0.0;
                        self.session_start_ms = self.now_ms as i64;
                        self.chant_count = 0;
                        self.undo_stack_depth = 0;
                        self.zen_scene.is_burning = true;
                        self.zen_scene.incense_progress = 0.0;
                        let unlocked = self
                            .achievement_system
                            .on_session_start(self.now_ms, self.tz_offset_minutes);
                        if let Some(first) = unlocked.into_iter().next() {
                            self.recent_unlocked_achievement = Some(first);
                        }
                        effects.push(HttpActionEffect::Heartbeat {
                            activity_type: "zen_room".to_string(),
                        });
                    }
                }
            }
            UiCommand::PausePracticeSession => {
                if self.is_in_session {
                    self.is_paused = true;
                }
            }
            UiCommand::ResumePracticeSession => {
                if self.is_in_session {
                    self.is_paused = false;
                }
            }
            UiCommand::IncrementChant => {
                if self.is_in_session {
                    self.chant_count = self.chant_count.saturating_add(1);
                    self.undo_stack_depth = self.undo_stack_depth.saturating_add(1);
                }
            }
            UiCommand::UndoChant => {
                if self.is_in_session && self.chant_count > 0 && self.undo_stack_depth > 0 {
                    self.chant_count -= 1;
                    self.undo_stack_depth -= 1;
                }
            }
            UiCommand::ToggleCircumambulation => {
                self.is_circumambulating = !self.is_circumambulating;
                self.zen_scene.auto_rotate = self.is_circumambulating;
            }
            UiCommand::EndPracticeSession => {
                if self.is_in_session {
                    self.is_in_session = false;
                    self.is_paused = false;
                    self.zen_scene.is_burning = false;
                    self.zen_scene.incense_progress = 0.0;

                    let duration_seconds = self.session_elapsed_secs.round() as u64;
                    let duration_minutes = (duration_seconds / 60) as u32;
                    let sutra_title = self
                        .locked_practice
                        .as_ref()
                        .map(|p| p.title.clone())
                        .unwrap_or_else(|| "默认功课".to_string());

                    let unlocked = self.achievement_system.on_session_end(
                        duration_seconds,
                        self.chant_count as u64,
                        &sutra_title,
                        self.now_ms,
                    );
                    if let Some(first) = unlocked.into_iter().next() {
                        self.recent_unlocked_achievement = Some(first);
                    }

                    self.pending_reflection = Some(PendingReflectionSession {
                        sutra: sutra_title,
                        duration_seconds,
                        duration_minutes,
                        chant_count: self.chant_count,
                        start_time_ms: self.session_start_ms,
                        end_time_ms: self.now_ms as i64,
                    });
                    self.is_reflection_open = true;
                }
            }
            UiCommand::SaveReflection { notes } => {
                if let Some(pending) = self.pending_reflection.take() {
                    self.is_reflection_open = false;
                    let clean_notes = notes.trim();
                    let notes_opt = if clean_notes.is_empty() {
                        None
                    } else {
                        Some(clean_notes.to_string())
                    };
                    effects.push(HttpActionEffect::SavePracticeSession {
                        sutra: pending.sutra,
                        duration_minutes: pending.duration_minutes,
                        chant_count: pending.chant_count,
                        start_time_ms: pending.start_time_ms,
                        end_time_ms: pending.end_time_ms,
                        notes: notes_opt,
                    });
                    self.sync_summary_effect(&mut effects);
                }
            }
            UiCommand::SkipReflection => {
                if let Some(pending) = self.pending_reflection.take() {
                    self.is_reflection_open = false;
                    effects.push(HttpActionEffect::SavePracticeSession {
                        sutra: pending.sutra,
                        duration_minutes: pending.duration_minutes,
                        chant_count: pending.chant_count,
                        start_time_ms: pending.start_time_ms,
                        end_time_ms: pending.end_time_ms,
                        notes: None,
                    });
                    self.sync_summary_effect(&mut effects);
                }
            }
            UiCommand::DismissAchievementPopup => {
                self.recent_unlocked_achievement = None;
            }
            UiCommand::OpenPracticeRecords { open } => {
                self.is_practice_records_open = open;
            }
            UiCommand::AddPracticeRecord {
                sutra_name,
                chant_count,
                duration_minutes,
                record_date,
                local_time,
                notes,
            } => {
                let clean = sutra_name.trim().to_string();
                if !clean.is_empty() {
                    effects.push(HttpActionEffect::CreatePracticeRecord {
                        sutra_name: clean,
                        chant_count,
                        duration_minutes,
                        record_date,
                        local_time,
                        notes,
                    });
                }
            }
            UiCommand::UpdatePracticeRecord {
                id,
                sutra_name,
                chant_count,
                duration_minutes,
                record_date,
                local_time,
                notes,
            } => {
                effects.push(HttpActionEffect::UpdatePracticeRecord {
                    id,
                    sutra_name,
                    chant_count,
                    duration_minutes,
                    record_date,
                    local_time,
                    notes,
                });
            }
            UiCommand::DeletePracticeRecord { id } => {
                self.practice_records.retain(|r| r.id != id);
                effects.push(HttpActionEffect::DeletePracticeRecord { id });
            }
            UiCommand::OpenMembership { open } => {
                self.is_membership_open = open;
            }
            UiCommand::UnlockMembership { plan } => {
                self.profile.is_vip = true;
                self.profile.vip_plan = plan.clone();
                self.is_membership_open = false;
                self.toast_message = Some("支付成功！会员已激活".to_string());
                effects.push(HttpActionEffect::ActivateMembership { plan });
                self.sync_summary_effect(&mut effects);
            }
            UiCommand::OpenEditProfile { open } => {
                self.is_edit_profile_open = open;
            }
            UiCommand::UpdateProfile { display_name, bio } => {
                let clean_name = display_name.trim().to_string();
                if !clean_name.is_empty() {
                    self.profile.display_name = clean_name.clone();
                }
                self.profile.bio = bio.trim().to_string();
                self.is_edit_profile_open = false;
                effects.push(HttpActionEffect::UpdateProfile {
                    display_name: Some(self.profile.display_name.clone()),
                    bio: Some(self.profile.bio.clone()),
                    locked_practice: None,
                    locale: None,
                });
            }
            UiCommand::ClearToast => {
                self.toast_message = None;
            }
        }

        self.render_result(effects)
    }

    pub fn tick(
        &mut self,
        dt_ms: f64,
        now_ms: f64,
        viewport_w: f64,
        viewport_h: f64,
    ) -> DispatchResult {
        self.now_ms = now_ms;
        if viewport_w > 0.0 && viewport_h > 0.0 {
            self.last_viewport_w = viewport_w;
            self.last_viewport_h = viewport_h;
        }
        let mut effects = Vec::new();

        match self.active_tab {
            AppTab::Home => {
                self.globe_scene.advance(dt_ms, now_ms);
                self.advance_dharma_transfer(dt_ms, now_ms, &mut effects);
            }
            AppTab::ZenRoom => {
                self.zen_scene.advance(dt_ms);
                if self.is_in_session && !self.is_paused {
                    self.session_elapsed_secs += (dt_ms / 1000.0).clamp(0.0, 1.0);
                    self.zen_scene.is_burning = true;
                    // Burn over 2 hours matching `MeditationRoomScreen`
                    self.zen_scene.incense_progress =
                        (self.session_elapsed_secs / 7200.0).clamp(0.0, 1.0);
                }
            }
            AppTab::Profile => {}
        }

        self.render_result(effects)
    }

    pub fn apply_http_effect(&mut self, payload: HttpResponsePayload) -> DispatchResult {
        let mut effects = Vec::new();
        match payload {
            HttpResponsePayload::Bootstrap {
                profile,
                locked_practice,
                leaderboard,
                practice_leaderboard,
                practice_records,
                practice_stats,
                unlocked_achievement_ids,
                online_global_sending_count,
                online_zen_room_count,
            } => {
                self.locale = LocaleCode::from_str_lossy(&profile.locale);
                self.profile = profile;
                if let Some(lp) = locked_practice {
                    self.locked_practice = Some(lp);
                } else if let Some(title) = &self.profile.locked_practice_title {
                    let sutra_opt = sutra_by_title(title);
                    self.locked_practice = Some(LockedPractice {
                        title: title.clone(),
                        category: sutra_opt
                            .map(|s| s.category.clone())
                            .unwrap_or_else(|| "自选功课".to_string()),
                        file_path: sutra_opt
                            .map(|s| format!("sutra:{}", s.id))
                            .unwrap_or_else(|| format!("manual:{title}")),
                        selected_at_ms: self.now_ms as i64,
                        is_locked: true,
                    });
                }
                self.leaderboard = leaderboard;
                self.practice_leaderboard = practice_leaderboard;
                self.practice_records = practice_records;
                self.practice_stats = practice_stats;
                self.achievement_system = AchievementSystem::from_persisted(
                    self.profile.total_practice_sessions,
                    self.profile.total_practice_minutes,
                    self.profile.total_practice_chants,
                    self.profile.consecutive_days,
                    None,
                    unlocked_achievement_ids,
                );
                if online_global_sending_count > 0 {
                    self.online_global_sending_count = online_global_sending_count;
                }
                if online_zen_room_count > 0 {
                    self.online_zen_room_count = online_zen_room_count;
                }
            }
            HttpResponsePayload::TransferRecorded {
                total_transfer_bytes,
                total_transfer_count,
                leaderboard,
            } => {
                self.profile.total_transfer_bytes = total_transfer_bytes;
                self.profile.total_transfer_count = total_transfer_count;
                if let Some(entries) = leaderboard {
                    self.leaderboard = entries;
                }
                self.sync_summary_effect(&mut effects);
            }
            HttpResponsePayload::PracticeSessionSaved {
                record,
                practice_stats,
                profile,
            } => {
                self.practice_records.insert(0, record);
                self.practice_stats = practice_stats;
                self.profile = profile;
                self.toast_message = Some("修行记录已同步到云端".to_string());
                self.sync_summary_effect(&mut effects);
            }
            HttpResponsePayload::PracticeRecordsRefreshed {
                practice_records,
                practice_stats,
            } => {
                self.practice_records = practice_records;
                self.practice_stats = practice_stats;
            }
            HttpResponsePayload::ProfileUpdated {
                profile,
                locked_practice,
            } => {
                self.profile = profile;
                if let Some(lp) = locked_practice {
                    self.locked_practice = Some(lp);
                }
            }
            HttpResponsePayload::MembershipActivated { profile } => {
                self.profile = profile;
            }
            HttpResponsePayload::FollowToggled {
                target_username,
                is_following,
                follower_count,
            } => {
                for list in [&mut self.leaderboard, &mut self.practice_leaderboard] {
                    if let Some(entry) = list.iter_mut().find(|e| e.username == target_username) {
                        entry.is_following = is_following;
                        entry.follower_count = follower_count;
                    }
                }
            }
            HttpResponsePayload::LeaderboardFetched { kind, entries } => {
                if kind == "practice" {
                    self.practice_leaderboard = entries;
                } else {
                    self.leaderboard = entries;
                }
            }
            HttpResponsePayload::HeartbeatAck {
                activity_type,
                online_count,
            } => {
                if activity_type == "zen_room" {
                    self.online_zen_room_count = online_count.max(1);
                } else {
                    self.online_global_sending_count = online_count.max(1);
                }
            }
            HttpResponsePayload::AiChatReply { reply } => {
                self.is_ai_generating = false;
                self.ai_messages.push(AiChatMessage {
                    id: format!("a_{}", self.now_ms as i64),
                    text: reply,
                    is_user: false,
                    is_error: false,
                    created_at_ms: self.now_ms as i64,
                });
            }
            HttpResponsePayload::Error { message } => {
                self.is_ai_generating = false;
                self.is_preparing_send = false;
                self.toast_message = Some(message);
            }
        }

        self.render_result(effects)
    }

    fn start_dharma_transfer(&mut self, effects: &mut Vec<HttpActionEffect>) {
        let text = self.composer_text.trim().to_string();
        if !text.is_empty() {
            let is_url = text.starts_with("http://") || text.starts_with("https://");
            let kind = if is_url { "链接" } else { "文本" };
            let title = if is_url {
                text.clone()
            } else {
                "法布施".to_string()
            };
            let text_bytes = text.len().max(256) as u64;
            let text_chars = text.chars().count();
            self.selected_content = Some(SelectedDharmaContent {
                kind: kind.to_string(),
                title,
                subtitle: format!("{text_chars} 字 · {}", format_file_size(text.len() as u64)),
                preview_text: text.clone(),
                source_url: if is_url { Some(text) } else { None },
                size_bytes: text_bytes,
            });
            self.composer_text.clear();
        }

        let Some(content) = self.selected_content.clone() else {
            self.toast_message = Some("请输入要法布施的文字或链接，或点 + 选择经文与文件。".to_string());
            return;
        };

        let targets = self.resolve_transfer_targets();
        if targets.is_empty() && !self.is_field_energy_mode && !self.is_local_loopback_enabled {
            self.toast_message = Some("请先在地区中选择全球、国家、本地场能或本地转经轮。".to_string());
            return;
        }

        self.is_preparing_send = false;
        self.is_transferring = true;
        self.current_sending_scripture = content.title.clone();
        self.loop_count = if self.is_looping { 1 } else { 0 };
        self.global_sent_count = 0;
        self.transfer_target_idx = 0;
        self.transfer_hop_elapsed_ms = 0.0;
        self.is_current_material_completed = false;
        self.current_country_progress = 0.0;
        self.transfer_targets = targets;

        let target_count = self.transfer_targets.len().max(1) as u64;
        self.transfer_bytes_per_hop = (content.size_bytes / target_count).max(1024);

        if let Some(first) = self.transfer_targets.first().cloned() {
            self.current_sending_country = first.name_zh.clone();
            self.globe_scene.add_beam(
                self.user_lng_safe_lat(),
                self.user_lng,
                first.lat,
                first.lng,
                Some(first.name_zh),
                self.now_ms,
                1600.0,
            );
        } else {
            self.current_sending_country = "本地法界".to_string();
        }

        let reported_codes: Vec<String> = if self.country_list.iter().any(|c| c == "ALL") {
            vec!["ALL".to_string()]
        } else {
            self.country_list.clone()
        };
        let reported_bytes = content.size_bytes.max(self.transfer_bytes_per_hop * target_count);
        effects.push(HttpActionEffect::RecordTransfer {
            content_title: content.title,
            content_kind: content.kind,
            target_countries: reported_codes,
            bytes_sent: reported_bytes,
            beam_count: target_count as u32,
        });
        effects.push(HttpActionEffect::Heartbeat {
            activity_type: "global_sending".to_string(),
        });
    }

    fn user_lng_safe_lat(&self) -> f64 {
        self.user_lat
    }

    fn resolve_transfer_targets(&self) -> Vec<TransferTarget> {
        if !self.is_global_send_enabled || self.country_list.is_empty() {
            return Vec::new();
        }
        if self.country_list.iter().any(|c| c == "ALL") {
            // Put primary spiritual hubs first, then stream all 249+ global countries
            let priority_codes = ["CN", "TW", "HK", "JP", "KR", "SG", "IN", "TH", "GB", "FR", "DE", "US", "CA", "AU", "BR", "RU"];
            let mut list: Vec<TransferTarget> = Vec::with_capacity(all_countries().len());
            for code in priority_codes {
                if let Some(c) = country_by_code(code) {
                    list.push(TransferTarget {
                        code: c.code.clone(),
                        name_zh: c.name_zh.clone(),
                        lat: c.latitude,
                        lng: c.longitude,
                    });
                }
            }
            for c in all_countries() {
                if !priority_codes.contains(&c.code.as_str()) {
                    list.push(TransferTarget {
                        code: c.code.clone(),
                        name_zh: c.name_zh.clone(),
                        lat: c.latitude,
                        lng: c.longitude,
                    });
                }
            }
            return list;
        }
        self.country_list
            .iter()
            .filter_map(|code| {
                country_by_code(code).map(|c| TransferTarget {
                    code: c.code.clone(),
                    name_zh: c.name_zh.clone(),
                    lat: c.latitude,
                    lng: c.longitude,
                })
            })
            .collect()
    }

    fn advance_dharma_transfer(
        &mut self,
        dt_ms: f64,
        now_ms: f64,
        effects: &mut Vec<HttpActionEffect>,
    ) {
        if !self.is_transferring {
            return;
        }

        if self.is_local_loopback_enabled {
            self.loopback_accum_ms += dt_ms;
            if self.loopback_accum_ms >= 350.0 {
                self.loopback_accum_ms = 0.0;
                self.loopback_count = self.loopback_count.saturating_add(1);
            }
        }
        if self.is_field_energy_mode {
            self.field_accum_ms += dt_ms;
            if self.field_accum_ms >= 850.0 {
                self.field_accum_ms = 0.0;
                self.field_broadcast_count = self.field_broadcast_count.saturating_add(1);
            }
        }

        if self.transfer_targets.is_empty() {
            return;
        }

        const HOP_DURATION_MS: f64 = 1600.0;
        self.transfer_hop_elapsed_ms += dt_ms;
        self.current_country_progress = (self.transfer_hop_elapsed_ms / HOP_DURATION_MS).clamp(0.0, 1.0);
        self.is_current_material_completed = self.current_country_progress >= 0.92;

        if self.transfer_hop_elapsed_ms >= HOP_DURATION_MS {
            self.transfer_hop_elapsed_ms = 0.0;
            self.global_sent_count = self.global_sent_count.saturating_add(1);
            self.global_data_sent_bytes = self
                .global_data_sent_bytes
                .saturating_add(self.transfer_bytes_per_hop);
            self.transfer_target_idx += 1;

            if self.transfer_target_idx >= self.transfer_targets.len() {
                if self.is_looping {
                    self.transfer_target_idx = 0;
                    self.loop_count = self.loop_count.saturating_add(1);
                    if let Some(content) = &self.selected_content {
                        let reported_codes = if self.country_list.iter().any(|c| c == "ALL") {
                            vec!["ALL".to_string()]
                        } else {
                            self.country_list.clone()
                        };
                        effects.push(HttpActionEffect::RecordTransfer {
                            content_title: content.title.clone(),
                            content_kind: content.kind.clone(),
                            target_countries: reported_codes,
                            bytes_sent: content.size_bytes.max(1024),
                            beam_count: self.transfer_targets.len() as u32,
                        });
                    }
                } else {
                    let count = self.global_sent_count;
                    self.stop_dharma_transfer();
                    self.toast_message = Some(format!("✨ 所选内容已发送完成，共完成 {count} 个国家！"));
                    return;
                }
            }

            if let Some(next) = self.transfer_targets.get(self.transfer_target_idx).cloned() {
                self.is_current_material_completed = false;
                self.current_country_progress = 0.0;
                self.current_sending_country = next.name_zh.clone();
                self.globe_scene.add_beam(
                    self.user_lat,
                    self.user_lng,
                    next.lat,
                    next.lng,
                    Some(next.name_zh),
                    now_ms,
                    HOP_DURATION_MS,
                );
            }
        }
    }

    fn stop_dharma_transfer(&mut self) {
        self.is_transferring = false;
        self.is_preparing_send = false;
        self.current_country_progress = 0.0;
        self.is_current_material_completed = false;
        self.current_sending_country.clear();
        self.current_sending_scripture.clear();
        self.globe_scene.clear_beams();
    }

    fn sync_summary_effect(&self, effects: &mut Vec<HttpActionEffect>) {
        let summary = serde_json::json!({
            "lockedPractice": self.locked_practice.as_ref().map(|p| &p.title),
            "totalTransferBytes": self.profile.total_transfer_bytes,
            "totalPracticeMinutes": self.profile.total_practice_minutes,
            "totalChants": self.profile.total_practice_chants,
            "isVip": self.profile.is_vip,
            "locale": self.locale.as_str(),
        });
        effects.push(HttpActionEffect::SyncCloudStorage {
            key: "fabushi_summary".to_string(),
            value: summary.to_string(),
        });
    }

    pub fn render_scene(&self, width: f64, height: f64) -> Vec<CanvasDrawCmd> {
        match self.active_tab {
            AppTab::Home => render_home_scene(
                &self.globe_scene,
                width,
                height,
                self.now_ms,
                self.home_render_mode == RenderMode::ThreeD && self.can_use_3d(),
            ),
            AppTab::ZenRoom => render_zen_scene(
                &self.zen_scene,
                width,
                height,
                self.zen_render_mode == RenderMode::ThreeD && self.can_use_3d(),
                !self.zen_scene.is_burning,
                self.locked_practice.as_ref().map(|p| p.title.as_str()),
            ),
            AppTab::Profile => Vec::new(),
        }
    }

    pub fn snapshot(&self) -> ViewModelSnapshot {
        let selected_codes: std::collections::HashSet<&str> =
            self.country_list.iter().map(|s| s.as_str()).collect();
        let region_countries: Vec<CountryOptionView> = search_countries(&self.region_search_query)
            .into_iter()
            .map(|c: &CountryNode| CountryOptionView {
                code: c.code.clone(),
                name_zh: c.name_zh.clone(),
                name_en: c.name_en.clone(),
                continent: c.continent.clone(),
                selected: selected_codes.contains(c.code.as_str()),
            })
            .collect();

        let total_secs = self.session_elapsed_secs.floor() as u64;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        let session_duration_formatted = format!("{mins:02}:{secs:02}");

        let selected_leaderboard_entry = self
            .selected_leaderboard_username
            .as_ref()
            .and_then(|uname| self.leaderboard.iter().find(|e| &e.username == uname).cloned());

        ViewModelSnapshot {
            active_tab: self.active_tab,
            locale: self.locale,
            strings: LocalizedStrings::for_locale(self.locale),
            home_render_mode: self.home_render_mode,
            zen_render_mode: self.zen_render_mode,
            can_use_3d: self.can_use_3d(),
            online_global_sending_count: self.online_global_sending_count,
            online_zen_room_count: self.online_zen_room_count,
            is_dharma_composer_mode: self.is_dharma_composer_mode,
            is_plus_menu_open: self.is_plus_menu_open,
            composer_text: self.composer_text.clone(),
            is_ai_generating: self.is_ai_generating,
            ai_messages: self.ai_messages.clone(),
            region_summary: self.region_summary(),
            country_list: self.country_list.clone(),
            is_global_send_enabled: self.is_global_send_enabled,
            is_looping: self.is_looping,
            is_field_energy_mode: self.is_field_energy_mode,
            is_local_loopback_enabled: self.is_local_loopback_enabled,
            selected_content: self.selected_content.clone(),
            is_preparing_send: self.is_preparing_send,
            is_transferring: self.is_transferring,
            current_sending_scripture: self.current_sending_scripture.clone(),
            current_sending_country: self.current_sending_country.clone(),
            current_country_progress: self.current_country_progress,
            is_current_material_completed: self.is_current_material_completed,
            global_sent_count: self.global_sent_count,
            global_data_sent_formatted: format_leaderboard_bytes(self.global_data_sent_bytes),
            loop_count: self.loop_count,
            loopback_count: self.loopback_count,
            field_broadcast_count: self.field_broadcast_count,
            is_region_selector_open: self.is_region_selector_open,
            region_search_query: self.region_search_query.clone(),
            region_countries,
            is_leaderboard_open: self.is_leaderboard_open,
            leaderboard: self.leaderboard.clone(),
            selected_leaderboard_entry,
            is_practice_leaderboard_open: self.is_practice_leaderboard_open,
            practice_leaderboard: self.practice_leaderboard.clone(),
            locked_practice: self.locked_practice.clone(),
            is_practice_selector_open: self.is_practice_selector_open,
            is_sutra_reader_open: self.is_sutra_reader_open,
            reader_title: self.reader_title.clone(),
            reader_category: self.reader_category.clone(),
            reader_content: self.reader_content.clone(),
            is_in_session: self.is_in_session,
            is_paused: self.is_paused,
            session_elapsed_secs: total_secs,
            session_duration_formatted,
            chant_count: self.chant_count,
            can_undo_chant: self.is_in_session && self.chant_count > 0 && self.undo_stack_depth > 0,
            is_circumambulating: self.is_circumambulating,
            incense_progress: self.zen_scene.incense_progress,
            is_reflection_open: self.is_reflection_open,
            pending_reflection: self.pending_reflection.clone(),
            recent_unlocked_achievement: self.recent_unlocked_achievement.clone(),
            achievements: self.achievement_system.catalog_snapshot(),
            profile: self.profile.clone(),
            is_practice_records_open: self.is_practice_records_open,
            practice_records: self.practice_records.clone(),
            practice_stats: self.practice_stats.clone(),
            is_membership_open: self.is_membership_open,
            membership_plans: membership_plans(),
            is_edit_profile_open: self.is_edit_profile_open,
            toast_message: self.toast_message.clone(),
            sutras: all_sutras().to_vec(),
            dharma_packages: all_dharma_packages().to_vec(),
            globe_rot_lon_deg: self.globe_scene.rotation_lon_deg,
        }
    }

    fn render_result(&self, effects: Vec<HttpActionEffect>) -> DispatchResult {
        DispatchResult {
            snapshot: self.snapshot(),
            draw_commands: self.render_scene(self.last_viewport_w, self.last_viewport_h),
            effects,
        }
    }
}

/// Compassionate pure-Rust Dharma assistant fallback reply when no external LLM key is configured.
pub fn dharma_ai_fallback_reply(
    question: &str,
    locked_practice: Option<&str>,
    locale: &str,
) -> String {
    let q = question.trim();
    let practice_ctx = locked_practice
        .map(|s| format!("您当前一门深入的主修功课是《{s}》。"))
        .unwrap_or_default();

    if locale.to_ascii_lowercase().starts_with("en") {
        if q.to_ascii_lowercase().contains("diamond") || q.contains("金刚经") {
            return "In the Diamond Sutra, the Buddha teaches: 'All conditioned phenomena are like a dream, an illusion, a bubble, a shadow.' Practise generosity without attachment to self or reward.".to_string();
        }
        return format!(
            "Namo Amitabha. Regarding \"{}\", cultivate mindfulness with a calm, compassionate heart. You can transmit Dharma offerings across all nations on the Home globe or enter the Zen Room for focused recitation.",
            q
        );
    }

    if q.contains("金刚经") {
        return format!(
            "南无阿弥陀佛。{practice_ctx}《金刚经》云：「应无所住而生其心。」日常行持可从布施、忍辱与观照自心做起，不执着相状而常行善法。"
        );
    }
    if q.contains("心经") {
        return format!(
            "南无阿弥陀佛。{practice_ctx}《心经》以「照见五蕴皆空，度一切苦厄」为核心，诵持时保持呼吸平稳、心念澄明即可。"
        );
    }
    if q.contains("法布施") || q.contains("发送") || q.contains("地球") {
        return format!(
            "南无阿弥陀佛。{practice_ctx}点击首页输入框左侧「+」选择「全球法布施」，即可选定国家或全球节点发送经文与法宝，愿法音遍传十方。"
        );
    }
    format!(
        "南无阿弥陀佛。{practice_ctx}关于「{}」：修行贵在觉察当下、一门深入。您可以点击下方进入「禅室」点香诵经，或在「首页」向全球十方发送法布施，随喜功德。",
        q
    )
}

pub fn format_country_label(code: &str) -> String {
    format!("{} {}", country_name_zh(code), code)
}

pub fn format_duration_minutes_zh(minutes: u32) -> String {
    format_duration_zh(minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vip_locks_3d_and_loopback_until_membership_unlocked() {
        let mut st = FabushiClientState::new(1_000_000.0);
        let r1 = st.dispatch(UiCommand::SelectHomeRenderMode {
            mode: RenderMode::ThreeD,
        });
        assert_eq!(r1.snapshot.home_render_mode, RenderMode::TwoD);
        assert!(r1.snapshot.is_membership_open);

        let r2 = st.dispatch(UiCommand::UnlockMembership {
            plan: "yearly".to_string(),
        });
        assert!(r2.snapshot.can_use_3d);
        assert!(r2
            .effects
            .iter()
            .any(|e| matches!(e, HttpActionEffect::ActivateMembership { plan } if plan == "yearly")));

        let r3 = st.dispatch(UiCommand::SelectHomeRenderMode {
            mode: RenderMode::ThreeD,
        });
        assert_eq!(r3.snapshot.home_render_mode, RenderMode::ThreeD);
    }

    #[test]
    fn dharma_composer_region_selector_and_transfer_work_end_to_end() {
        let mut st = FabushiClientState::new(10_000.0);
        st.dispatch(UiCommand::ActivateDharmaMode);
        assert!(st.is_dharma_composer_mode);
        assert_eq!(st.region_summary(), "全球");

        st.dispatch(UiCommand::ApplyRegionSelection {
            country_codes: vec!["CN".into(), "JP".into()],
            field_energy: true,
            local_loopback: false,
        });
        assert_eq!(st.region_summary(), "中国、日本、本地场能");

        st.dispatch(UiCommand::SelectDharmaSutra {
            title: "心经".into(),
        });
        let res = st.dispatch(UiCommand::SubmitComposer);
        assert!(res.snapshot.is_transferring);
        assert_eq!(res.snapshot.current_sending_scripture, "心经");
        assert!(res
            .effects
            .iter()
            .any(|e| matches!(e, HttpActionEffect::RecordTransfer { .. })));

        // Advance 2 hops -> completes both CN and JP and stops
        st.tick(1700.0, 11_700.0, 420.0, 780.0);
        let fin = st.tick(1700.0, 13_400.0, 420.0, 780.0);
        assert!(!fin.snapshot.is_transferring);
        assert_eq!(fin.snapshot.global_sent_count, 2);
    }

    #[test]
    fn zen_room_practice_lock_chant_undo_and_reflection_flow() {
        let mut st = FabushiClientState::new(50_000.0);
        st.dispatch(UiCommand::SelectTab {
            tab: AppTab::ZenRoom,
        });

        // Starting without locked practice opens selector
        let r0 = st.dispatch(UiCommand::StartPracticeSession);
        assert!(!r0.snapshot.is_in_session);
        assert!(r0.snapshot.is_practice_selector_open);

        // Lock Diamond Sutra
        let r1 = st.dispatch(UiCommand::SelectAndLockPractice {
            title: "金刚经".into(),
        });
        assert_eq!(r1.snapshot.locked_practice.as_ref().unwrap().title, "金刚经");
        // Subsequent attempt keeps original lock
        let r1_dup = st.dispatch(UiCommand::SelectAndLockPractice {
            title: "心经".into(),
        });
        assert_eq!(r1_dup.snapshot.locked_practice.as_ref().unwrap().title, "金刚经");

        // Idle Zen room displays the sutra book; active burning session hides it to avoid overlapping .zen-counter
        let idle_cmds = st.render_scene(420.0, 780.0);
        assert!(idle_cmds.iter().any(|c| matches!(c, CanvasDrawCmd::DrawText { text, .. } if text == "金刚经")));

        // Start session, chant twice, undo once, advance time, end & save reflection
        st.dispatch(UiCommand::StartPracticeSession);
        let active_cmds = st.render_scene(420.0, 780.0);
        assert!(!active_cmds.iter().any(|c| matches!(c, CanvasDrawCmd::DrawText { text, .. } if text == "金刚经")));
        st.dispatch(UiCommand::IncrementChant);
        st.dispatch(UiCommand::IncrementChant);
        let r_undo = st.dispatch(UiCommand::UndoChant);
        assert_eq!(r_undo.snapshot.chant_count, 1);

        st.tick(250.0, 50_250.0, 420.0, 780.0);
        st.tick(250.0, 50_500.0, 420.0, 780.0);
        let r_end = st.dispatch(UiCommand::EndPracticeSession);
        assert!(!r_end.snapshot.is_in_session);
        assert!(r_end.snapshot.is_reflection_open);
        assert!(r_end.draw_commands.iter().any(|c| matches!(c, CanvasDrawCmd::DrawText { text, .. } if text == "金刚经")));

        let r_save = st.dispatch(UiCommand::SaveReflection {
            notes: "清净欢喜".into(),
        });
        assert!(!r_save.snapshot.is_reflection_open);
        assert!(r_save
            .effects
            .iter()
            .any(|e| matches!(e, HttpActionEffect::SavePracticeSession { notes: Some(n), .. } if n == "清净欢喜")));
    }

    #[test]
    fn ask_ai_i18n_and_sutra_reader_work() {
        let mut st = FabushiClientState::new(1000.0);
        st.set_initial_tab_from_query("zen");
        assert_eq!(st.active_tab, AppTab::ZenRoom);

        let reader = st.dispatch(UiCommand::OpenSutraReader {
            open: true,
            title: Some("心经".into()),
        });
        assert!(reader.snapshot.is_sutra_reader_open);
        assert!(reader.snapshot.reader_content.contains("观自在菩萨"));

        st.dispatch(UiCommand::SelectTab { tab: AppTab::Home });
        st.dispatch(UiCommand::SetComposerText {
            text: "如何修习金刚经".into(),
        });
        let ai_res = st.dispatch(UiCommand::SubmitComposer);
        assert!(ai_res.snapshot.is_ai_generating);
        assert!(ai_res
            .effects
            .iter()
            .any(|e| matches!(e, HttpActionEffect::SendAiChat { .. })));

        let fallback = dharma_ai_fallback_reply("如何修习金刚经", Some("金刚经"), "zh-CN");
        assert!(fallback.contains("应无所住而生其心"));

        let replied = st.apply_http_effect(HttpResponsePayload::AiChatReply {
            reply: fallback,
        });
        assert!(!replied.snapshot.is_ai_generating);
        assert_eq!(replied.snapshot.ai_messages.len(), 2);

        let en = st.dispatch(UiCommand::SetLocale {
            locale: LocaleCode::En,
        });
        assert_eq!(en.snapshot.strings.tab_zen, "Zen Room");
    }
}
