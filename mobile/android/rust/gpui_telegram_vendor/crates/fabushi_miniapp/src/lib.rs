//! Pure-Rust shared Mini App core for 全球法布施 (Global Dharma Offering & Zen Room).
//!
//! Default features compile cleanly to `wasm32-unknown-unknown` with zero OS/DB dependencies.
//! Enable the `sqlite` feature on Android/mobile for SQLite persistence (`storage::*`) without
//! embedding desktop `.wasm` or `.webp` assets.

pub mod achievements;
pub mod countries;
pub mod scene_math;
pub mod state;
pub mod sutras;

#[cfg(any(feature = "sqlite", feature = "server"))]
pub mod storage;

/// Cosmic dark glass design tokens ported from `lib/core/design_system/colors.dart` & `app_theme.dart`.
pub mod theme {
    pub const SPACE_DEEP_BLUE: &str = "#0B0E14";
    pub const SPACE_BLUE: &str = "#1B263B";
    pub const STARLIGHT_WHITE: &str = "#E8EAF6";
    pub const NEBULA_PURPLE: &str = "#7B1FA2";
    pub const NEBULA_PINK: &str = "#E91E63";
    pub const COSMIC_GOLD: &str = "#FFD700";
    pub const ZEN_GOLD: &str = "#D4AF37";
    pub const GLASS_BORDER: &str = "rgba(255, 255, 255, 0.15)";
    pub const GLASS_SURFACE: &str = "rgba(255, 255, 255, 0.10)";
}
