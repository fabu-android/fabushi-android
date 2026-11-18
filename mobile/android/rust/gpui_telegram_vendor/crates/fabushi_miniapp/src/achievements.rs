use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Tier classification matching `AchievementTier` in `achievement_system.dart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AchievementTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Legendary,
}

impl AchievementTier {
    pub fn color_hex(self) -> &'static str {
        match self {
            Self::Bronze => "#CD7F32",
            Self::Silver => "#C0C0C0",
            Self::Gold => "#FFD700",
            Self::Platinum => "#E5E4E2",
            Self::Legendary => "#9400D3",
        }
    }

    pub fn name_zh(self) -> &'static str {
        match self {
            Self::Bronze => "铜",
            Self::Silver => "银",
            Self::Gold => "金",
            Self::Platinum => "白金",
            Self::Legendary => "传说",
        }
    }
}

/// Visual celebration style matching `AchievementAnimation` in `achievement_system.dart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AchievementAnimation {
    Sunrise,
    Sparkle,
    Flame,
    StarBurst,
    Diamond,
    Trophy,
    Crown,
    MoonGlow,
    Pulse,
    Hourglass,
    ClockChime,
    Mala,
    Target,
    Medal,
    GoldMedal,
    Cosmic,
}

/// Achievement badge definition and unlock metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Achievement {
    pub id: String,
    pub title: String,
    pub title_en: String,
    pub description: String,
    pub description_en: String,
    pub icon: String,
    pub tier: AchievementTier,
    pub tier_color: String,
    pub tier_name: String,
    pub animation: AchievementAnimation,
    pub unlocked: bool,
    pub unlocked_at_ms: Option<f64>,
}

struct AchievementDef {
    id: &'static str,
    title: &'static str,
    title_en: &'static str,
    description: &'static str,
    description_en: &'static str,
    icon: &'static str,
    tier: AchievementTier,
    animation: AchievementAnimation,
}

const CATALOG: &[AchievementDef] = &[
    AchievementDef {
        id: "daily_start",
        title: "今日已开启修行",
        title_en: "Daily Practice Begun",
        description: "迈出修行的第一步",
        description_en: "Took the first step of today's practice",
        icon: "🌅",
        tier: AchievementTier::Bronze,
        animation: AchievementAnimation::Sunrise,
    },
    AchievementDef {
        id: "first_session",
        title: "初心不改",
        title_en: "Beginner's Mind",
        description: "完成首次修行",
        description_en: "Completed your first Zen practice session",
        icon: "✨",
        tier: AchievementTier::Gold,
        animation: AchievementAnimation::Sparkle,
    },
    AchievementDef {
        id: "streak_3",
        title: "三日精进",
        title_en: "3-Day Diligence",
        description: "连续修行3天",
        description_en: "Practiced for 3 consecutive days",
        icon: "🔥",
        tier: AchievementTier::Bronze,
        animation: AchievementAnimation::Flame,
    },
    AchievementDef {
        id: "streak_7",
        title: "七日不辍",
        title_en: "7-Day Steadfastness",
        description: "连续修行7天",
        description_en: "Practiced for 7 consecutive days",
        icon: "🌟",
        tier: AchievementTier::Silver,
        animation: AchievementAnimation::StarBurst,
    },
    AchievementDef {
        id: "streak_21",
        title: "习惯养成",
        title_en: "21-Day Habit",
        description: "连续修行21天",
        description_en: "Practiced for 21 consecutive days",
        icon: "💎",
        tier: AchievementTier::Gold,
        animation: AchievementAnimation::Diamond,
    },
    AchievementDef {
        id: "streak_30",
        title: "一月精修",
        title_en: "30-Day Mastery",
        description: "连续修行30天",
        description_en: "Practiced for 30 consecutive days",
        icon: "🏆",
        tier: AchievementTier::Platinum,
        animation: AchievementAnimation::Trophy,
    },
    AchievementDef {
        id: "streak_100",
        title: "百日筑基",
        title_en: "100-Day Foundation",
        description: "连续修行100天",
        description_en: "Practiced for 100 consecutive days",
        icon: "👑",
        tier: AchievementTier::Legendary,
        animation: AchievementAnimation::Crown,
    },
    AchievementDef {
        id: "midnight_practice",
        title: "子时修行",
        title_en: "Midnight Vigil",
        description: "在子时开始修行",
        description_en: "Began practice during the midnight watch (23:00-01:00)",
        icon: "🌙",
        tier: AchievementTier::Silver,
        animation: AchievementAnimation::MoonGlow,
    },
    AchievementDef {
        id: "morning_practice",
        title: "清晨早课",
        title_en: "Dawn Chanting",
        description: "在清晨开始修行",
        description_en: "Began morning practice between 05:00 and 07:00",
        icon: "🌄",
        tier: AchievementTier::Silver,
        animation: AchievementAnimation::Sunrise,
    },
    AchievementDef {
        id: "duration_5",
        title: "五分精进",
        title_en: "5-Minute Focus",
        description: "单次修行5分钟",
        description_en: "Meditated for 5 minutes in a single session",
        icon: "⏱️",
        tier: AchievementTier::Bronze,
        animation: AchievementAnimation::Pulse,
    },
    AchievementDef {
        id: "duration_30",
        title: "半时功德",
        title_en: "Half-Hour Merit",
        description: "单次修行30分钟",
        description_en: "Meditated for 30 minutes in a single session",
        icon: "⏳",
        tier: AchievementTier::Silver,
        animation: AchievementAnimation::Hourglass,
    },
    AchievementDef {
        id: "duration_60",
        title: "一时圆满",
        title_en: "Full-Hour Perfection",
        description: "单次修行60分钟",
        description_en: "Meditated for 60 minutes in a single session",
        icon: "🕐",
        tier: AchievementTier::Gold,
        animation: AchievementAnimation::ClockChime,
    },
    AchievementDef {
        id: "chant_108",
        title: "一百零八遍",
        title_en: "108 Mala Chants",
        description: "单次念诵108遍",
        description_en: "Completed 108 chants in a single session",
        icon: "📿",
        tier: AchievementTier::Gold,
        animation: AchievementAnimation::Mala,
    },
    AchievementDef {
        id: "chant_1000",
        title: "千遍精进",
        title_en: "1,000 Chants",
        description: "累计念诵1000遍",
        description_en: "Accumulated 1,000 total chants",
        icon: "🙏",
        tier: AchievementTier::Platinum,
        animation: AchievementAnimation::Sparkle,
    },
    AchievementDef {
        id: "chant_10000",
        title: "万遍圆满",
        title_en: "10,000 Chants",
        description: "累计念诵10000遍",
        description_en: "Accumulated 10,000 total chants",
        icon: "🪷",
        tier: AchievementTier::Legendary,
        animation: AchievementAnimation::Cosmic,
    },
    AchievementDef {
        id: "sessions_10",
        title: "十次精进",
        title_en: "10 Sessions",
        description: "累计修行10次",
        description_en: "Completed 10 total practice sessions",
        icon: "🎯",
        tier: AchievementTier::Bronze,
        animation: AchievementAnimation::Target,
    },
    AchievementDef {
        id: "sessions_50",
        title: "五十功德",
        title_en: "50 Sessions",
        description: "累计修行50次",
        description_en: "Completed 50 total practice sessions",
        icon: "🎖️",
        tier: AchievementTier::Silver,
        animation: AchievementAnimation::Medal,
    },
    AchievementDef {
        id: "sessions_100",
        title: "百次圆满",
        title_en: "100 Sessions",
        description: "累计修行100次",
        description_en: "Completed 100 total practice sessions",
        icon: "🏅",
        tier: AchievementTier::Gold,
        animation: AchievementAnimation::GoldMedal,
    },
    AchievementDef {
        id: "total_60min",
        title: "累计一小时",
        title_en: "1 Cumulative Hour",
        description: "累计修行60分钟",
        description_en: "Accumulated 60 minutes of practice",
        icon: "⌛",
        tier: AchievementTier::Bronze,
        animation: AchievementAnimation::Hourglass,
    },
    AchievementDef {
        id: "total_10hr",
        title: "累计十小时",
        title_en: "10 Cumulative Hours",
        description: "累计修行10小时",
        description_en: "Accumulated 10 hours of practice",
        icon: "🌟",
        tier: AchievementTier::Silver,
        animation: AchievementAnimation::StarBurst,
    },
    AchievementDef {
        id: "total_100hr",
        title: "累计百时",
        title_en: "100 Cumulative Hours",
        description: "累计修行100小时",
        description_en: "Accumulated 100 hours of practice",
        icon: "💫",
        tier: AchievementTier::Platinum,
        animation: AchievementAnimation::Cosmic,
    },
];

fn def_to_achievement(def: &AchievementDef, unlocked: bool, unlocked_at_ms: Option<f64>) -> Achievement {
    Achievement {
        id: def.id.to_string(),
        title: def.title.to_string(),
        title_en: def.title_en.to_string(),
        description: def.description.to_string(),
        description_en: def.description_en.to_string(),
        icon: def.icon.to_string(),
        tier: def.tier,
        tier_color: def.tier.color_hex().to_string(),
        tier_name: def.tier.name_zh().to_string(),
        animation: def.animation,
        unlocked,
        unlocked_at_ms,
    }
}

/// Pure-Rust micro-achievement system faithfully ported from `achievement_system.dart`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AchievementSystem {
    pub total_sessions: u64,
    pub total_minutes: u64,
    pub total_chants: u64,
    pub consecutive_days: u64,
    pub last_session_day: Option<i64>,
    pub unlocked_ids: BTreeSet<String>,
    pub unlocked_timestamps_ms: std::collections::BTreeMap<String, f64>,
}

impl AchievementSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_persisted(
        total_sessions: u64,
        total_minutes: u64,
        total_chants: u64,
        consecutive_days: u64,
        last_session_day: Option<i64>,
        unlocked_ids: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            total_sessions,
            total_minutes,
            total_chants,
            consecutive_days,
            last_session_day,
            unlocked_ids: unlocked_ids.into_iter().collect(),
            unlocked_timestamps_ms: Default::default(),
        }
    }

    /// Computes local calendar day index and hour (0..23) purely from UTC epoch ms + offset.
    pub fn local_day_and_hour(unix_timestamp_ms: f64, tz_offset_minutes: i32) -> (i64, u32) {
        let local_secs = (unix_timestamp_ms / 1000.0).floor() as i64 + (tz_offset_minutes as i64) * 60;
        let day = local_secs.div_euclid(86_400);
        let secs_in_day = local_secs.rem_euclid(86_400);
        let hour = (secs_in_day / 3600) as u32;
        (day, hour)
    }

    /// Evaluates start-of-session achievements (`daily_start`, streaks, `midnight_practice`, `morning_practice`).
    /// Note: `daily_start` can re-fire on a new calendar day while permanent milestones unlock once.
    pub fn on_session_start(
        &mut self,
        unix_timestamp_ms: f64,
        tz_offset_minutes: i32,
    ) -> Vec<Achievement> {
        let (today, hour) = Self::local_day_and_hour(unix_timestamp_ms, tz_offset_minutes);
        let mut newly_unlocked = Vec::new();

        if self.last_session_day != Some(today) {
            // Daily greeting achievement (fires on each new practice day, recorded in unlocked set).
            if let Some(ach) = self.unlock_or_trigger_daily("daily_start", unix_timestamp_ms) {
                newly_unlocked.push(ach);
            }

            if let Some(last_day) = self.last_session_day {
                if last_day == today - 1 {
                    self.consecutive_days = self.consecutive_days.saturating_add(1);
                    for &(days, id) in &[
                        (3, "streak_3"),
                        (7, "streak_7"),
                        (21, "streak_21"),
                        (30, "streak_30"),
                        (100, "streak_100"),
                    ] {
                        if self.consecutive_days >= days {
                            if let Some(ach) = self.unlock_once(id, unix_timestamp_ms) {
                                newly_unlocked.push(ach);
                            }
                        }
                    }
                } else {
                    self.consecutive_days = 1;
                }
            } else {
                self.consecutive_days = 1;
            }
            self.last_session_day = Some(today);
        }

        // Midnight practice: 23:00 - 01:00
        if hour >= 23 || hour < 1 {
            if let Some(ach) = self.unlock_once("midnight_practice", unix_timestamp_ms) {
                newly_unlocked.push(ach);
            }
        }

        // Morning practice: 05:00 - 07:00
        if (5..7).contains(&hour) {
            if let Some(ach) = self.unlock_once("morning_practice", unix_timestamp_ms) {
                newly_unlocked.push(ach);
            }
        }

        newly_unlocked
    }

    /// Evaluates end-of-session achievements (`first_session`, duration, chant, session, total minute milestones).
    pub fn on_session_end(
        &mut self,
        duration_seconds: u64,
        chant_count: u64,
        _sutra: &str,
        unix_timestamp_ms: f64,
    ) -> Vec<Achievement> {
        let mut newly_unlocked = Vec::new();
        let duration_minutes = duration_seconds / 60;

        self.total_sessions = self.total_sessions.saturating_add(1);
        self.total_minutes = self.total_minutes.saturating_add(duration_minutes);
        self.total_chants = self.total_chants.saturating_add(chant_count);

        if self.total_sessions == 1 {
            if let Some(ach) = self.unlock_once("first_session", unix_timestamp_ms) {
                newly_unlocked.push(ach);
            }
        }

        // Single-session duration milestones
        for &(mins, id) in &[(5, "duration_5"), (30, "duration_30"), (60, "duration_60")] {
            if duration_minutes >= mins {
                if let Some(ach) = self.unlock_once(id, unix_timestamp_ms) {
                    newly_unlocked.push(ach);
                }
            }
        }

        // Chant milestones: 108 in a session, plus 1000 & 10000 cumulative (or single session)
        if chant_count >= 108 {
            if let Some(ach) = self.unlock_once("chant_108", unix_timestamp_ms) {
                newly_unlocked.push(ach);
            }
        }
        if self.total_chants >= 1000 || chant_count >= 1000 {
            if let Some(ach) = self.unlock_once("chant_1000", unix_timestamp_ms) {
                newly_unlocked.push(ach);
            }
        }
        if self.total_chants >= 10000 || chant_count >= 10000 {
            if let Some(ach) = self.unlock_once("chant_10000", unix_timestamp_ms) {
                newly_unlocked.push(ach);
            }
        }

        // Cumulative session milestones
        for &(sessions, id) in &[
            (10, "sessions_10"),
            (50, "sessions_50"),
            (100, "sessions_100"),
        ] {
            if self.total_sessions >= sessions {
                if let Some(ach) = self.unlock_once(id, unix_timestamp_ms) {
                    newly_unlocked.push(ach);
                }
            }
        }

        // Cumulative duration milestones
        for &(mins, id) in &[
            (60, "total_60min"),
            (600, "total_10hr"),
            (6000, "total_100hr"),
        ] {
            if self.total_minutes >= mins {
                if let Some(ach) = self.unlock_once(id, unix_timestamp_ms) {
                    newly_unlocked.push(ach);
                }
            }
        }

        newly_unlocked
    }

    pub fn is_unlocked(&self, id: &str) -> bool {
        self.unlocked_ids.contains(id)
    }

    pub fn unlocked_list(&self) -> Vec<String> {
        self.unlocked_ids.iter().cloned().collect()
    }

    /// Returns the full achievement catalog annotated with current unlock state.
    pub fn catalog_snapshot(&self) -> Vec<Achievement> {
        CATALOG
            .iter()
            .map(|def| {
                let unlocked = self.unlocked_ids.contains(def.id);
                let ts = self.unlocked_timestamps_ms.get(def.id).copied();
                def_to_achievement(def, unlocked, ts)
            })
            .collect()
    }

    fn unlock_once(&mut self, id: &str, unix_timestamp_ms: f64) -> Option<Achievement> {
        if self.unlocked_ids.contains(id) {
            return None;
        }
        let def = CATALOG.iter().find(|d| d.id == id)?;
        self.unlocked_ids.insert(id.to_string());
        self.unlocked_timestamps_ms.insert(id.to_string(), unix_timestamp_ms);
        Some(def_to_achievement(def, true, Some(unix_timestamp_ms)))
    }

    fn unlock_or_trigger_daily(&mut self, id: &str, unix_timestamp_ms: f64) -> Option<Achievement> {
        let def = CATALOG.iter().find(|d| d.id == id)?;
        self.unlocked_ids.insert(id.to_string());
        self.unlocked_timestamps_ms.insert(id.to_string(), unix_timestamp_ms);
        Some(def_to_achievement(def, true, Some(unix_timestamp_ms)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlocks_daily_midnight_streak_and_milestones() {
        let mut sys = AchievementSystem::new();
        // 2026-11-16 23:30:00 UTC -> day D, hour 23
        let day0_ms = 1_794_871_800_000.0; // exact hour 23 check via helper:
        let (d0, _) = AchievementSystem::local_day_and_hour(day0_ms, 0);
        let start0_ms = (d0 as f64 * 86_400.0 + 23.0 * 3600.0 + 600.0) * 1000.0;

        let unlocked = sys.on_session_start(start0_ms, 0);
        assert!(unlocked.iter().any(|a| a.id == "daily_start"));
        assert!(unlocked.iter().any(|a| a.id == "midnight_practice"));
        assert_eq!(sys.consecutive_days, 1);

        // Next 2 consecutive days -> streak_3
        let start1_ms = ((d0 + 1) as f64 * 86_400.0 + 6.0 * 3600.0) * 1000.0;
        let u1 = sys.on_session_start(start1_ms, 0);
        assert!(u1.iter().any(|a| a.id == "morning_practice"));
        assert_eq!(sys.consecutive_days, 2);

        let start2_ms = ((d0 + 2) as f64 * 86_400.0 + 10.0 * 3600.0) * 1000.0;
        let u2 = sys.on_session_start(start2_ms, 0);
        assert!(u2.iter().any(|a| a.id == "streak_3"));

        // End session with 35 minutes & 108 chants
        let end_ms = start2_ms + 35.0 * 60_000.0;
        let end_unlocked = sys.on_session_end(35 * 60, 108, "金刚经", end_ms);
        assert!(end_unlocked.iter().any(|a| a.id == "first_session"));
        assert!(end_unlocked.iter().any(|a| a.id == "duration_5"));
        assert!(end_unlocked.iter().any(|a| a.id == "duration_30"));
        assert!(end_unlocked.iter().any(|a| a.id == "chant_108"));

        // Accumulate >= 1000 chants -> chant_1000
        let end2 = sys.on_session_end(60 * 60, 900, "金刚经", end_ms + 3_600_000.0);
        assert!(end2.iter().any(|a| a.id == "duration_60"));
        assert!(end2.iter().any(|a| a.id == "chant_1000"));
        assert!(end2.iter().any(|a| a.id == "total_60min"));
    }
}
