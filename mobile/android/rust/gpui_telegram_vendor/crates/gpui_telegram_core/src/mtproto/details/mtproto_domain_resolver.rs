//! MTProto Domain Name & DNS Resolver
//! Upstream Source: `Telegram/SourceFiles/mtproto/details/mtproto_domain_resolver.h`, `mtproto_domain_resolver.cpp`

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedDomainEntry {
    pub ip_addresses: Vec<String>,
    pub resolved_at: i64,
    pub ttl_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoDomainResolverDetails {
    pub cache: HashMap<String, ResolvedDomainEntry>,
    pub default_ttl_seconds: i64,
}

impl Default for MtprotoDomainResolverDetails {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
            default_ttl_seconds: 3600,
        }
    }
}

impl MtprotoDomainResolverDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.default_ttl_seconds > 0
    }

    pub fn insert_resolution(&mut self, domain: impl Into<String>, ips: Vec<String>) {
        let entry = ResolvedDomainEntry {
            ip_addresses: ips,
            resolved_at: chrono::Utc::now().timestamp(),
            ttl_seconds: self.default_ttl_seconds,
        };
        self.cache.insert(domain.into(), entry);
    }

    pub fn resolve(&self, domain: &str, current_time: i64) -> Option<&[String]> {
        if let Some(entry) = self.cache.get(domain) {
            if current_time <= entry.resolved_at + entry.ttl_seconds {
                return Some(&entry.ip_addresses);
            }
        }
        None
    }

    pub fn clear_expired(&mut self, current_time: i64) {
        self.cache.retain(|_, entry| current_time <= entry.resolved_at + entry.ttl_seconds);
    }
}
