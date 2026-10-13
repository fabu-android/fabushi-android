//! Platform-independent managed setup foundations.
//!
//! Android keeps network/auth production binding separate so the existing dashboard credential
//! owner remains canonical; these modules own only cache, normalization and refresh semantics.
pub mod managed_skills_cache;
pub mod managed_skills_service;
pub mod sand_managed_skills;
