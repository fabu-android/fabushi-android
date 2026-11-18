//! Telegram Desktop 1:1 Parity Submodule: `ApiInviteLinks`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_invite_links.cpp, Telegram/SourceFiles/api/api_invite_links.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InviteLink {
    pub link: String,
    pub label: String,
    pub admin_id: u64,
    pub date: i64,
    pub start_date: i64,
    pub expire_date: i64,
    pub usage_limit: i32,
    pub usage: i32,
    pub requested: i32,
    pub request_approval: bool,
    pub permanent: bool,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinedByLinkUser {
    pub user_id: u64,
    pub date: i64,
    pub via_filter_link: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateInviteLinkArgs {
    pub peer_id: u64,
    pub label: String,
    pub expire_date: i64,
    pub usage_limit: i32,
    pub request_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiInviteLinks {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub peer_links: HashMap<u64, Vec<InviteLink>>,
    pub joined_users: HashMap<String, Vec<JoinedByLinkUser>>,
    pub updated_at: i64,
}

impl Default for ApiInviteLinks {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiInviteLinks".to_string(),
            peer_links: HashMap::new(),
            joined_users: HashMap::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiInviteLinks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn create(&mut self, args: CreateInviteLinkArgs) -> InviteLink {
        let slug = uuid::Uuid::new_v4().as_fields().0;
        let link_url = format!("https://t.me/+join_{:x}", slug);
        let link = InviteLink {
            link: link_url,
            label: args.label,
            admin_id: 0,
            date: chrono::Utc::now().timestamp(),
            start_date: 0,
            expire_date: args.expire_date,
            usage_limit: args.usage_limit,
            usage: 0,
            requested: 0,
            request_approval: args.request_approval,
            permanent: false,
            revoked: false,
        };
        self.peer_links.entry(args.peer_id).or_default().push(link.clone());
        self.updated_at = chrono::Utc::now().timestamp();
        link
    }

    pub fn revoke(&mut self, peer_id: u64, link_url: &str) -> bool {
        if let Some(links) = self.peer_links.get_mut(&peer_id) {
            if let Some(link) = links.iter_mut().find(|l| l.link == link_url) {
                link.revoked = true;
                self.updated_at = chrono::Utc::now().timestamp();
                return true;
            }
        }
        false
    }
}
