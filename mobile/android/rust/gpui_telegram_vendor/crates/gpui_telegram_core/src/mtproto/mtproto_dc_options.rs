//! MTProto Datacenter Options & Multi-DC Routing Table
//! Upstream Source: `Telegram/SourceFiles/mtproto/mtproto_dc_options.h`, `mtproto_dc_options.cpp`

use crate::mtproto::core_types::{bare_dc_id, DcId, ShiftedDcId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DcType {
    #[default]
    Regular,
    Temporary,
    MediaCluster,
    Cdn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Environment {
    #[default]
    Production,
    Test,
}

pub mod dc_flags {
    pub const IPV6: u32 = 1 << 0;
    pub const MEDIA_ONLY: u32 = 1 << 1;
    pub const TCP_ONLY: u32 = 1 << 2;
    pub const CDN: u32 = 1 << 3;
    pub const STATIC: u32 = 1 << 4;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DcEndpoint {
    pub id: DcId,
    pub flags: u32,
    pub ip: String,
    pub port: u16,
    pub secret: Vec<u8>,
}

impl DcEndpoint {
    pub fn new(id: DcId, ip: impl Into<String>, port: u16, flags: u32) -> Self {
        Self {
            id,
            flags,
            ip: ip.into(),
            port,
            secret: Vec::new(),
        }
    }

    pub fn is_ipv6(&self) -> bool {
        (self.flags & dc_flags::IPV6) != 0
    }

    pub fn is_media_only(&self) -> bool {
        (self.flags & dc_flags::MEDIA_ONLY) != 0
    }

    pub fn is_cdn(&self) -> bool {
        (self.flags & dc_flags::CDN) != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoDcOptions {
    pub environment: Environment,
    pub endpoints: Vec<DcEndpoint>,
}

impl Default for MtprotoDcOptions {
    fn default() -> Self {
        let mut options = Self {
            environment: Environment::Production,
            endpoints: Vec::new(),
        };
        options.construct_from_built_in();
        options
    }
}

impl MtprotoDcOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        !self.endpoints.is_empty()
    }

    pub fn is_test_mode(&self) -> bool {
        self.environment == Environment::Test
    }

    pub fn construct_from_built_in(&mut self) {
        self.endpoints.clear();
        if self.environment == Environment::Test {
            self.endpoints.push(DcEndpoint::new(1, "149.154.175.10", 443, 0));
            self.endpoints.push(DcEndpoint::new(2, "149.154.167.40", 443, 0));
            self.endpoints.push(DcEndpoint::new(3, "149.154.175.117", 443, 0));
        } else {
            // Production default endpoints for DC 1 to 5
            self.endpoints.push(DcEndpoint::new(1, "149.154.175.50", 443, 0));
            self.endpoints.push(DcEndpoint::new(2, "149.154.167.51", 443, 0));
            self.endpoints.push(DcEndpoint::new(3, "149.154.175.100", 443, 0));
            self.endpoints.push(DcEndpoint::new(4, "149.154.167.91", 443, 0));
            self.endpoints.push(DcEndpoint::new(5, "91.108.56.130", 443, 0));

            // IPv6 endpoints
            self.endpoints.push(DcEndpoint::new(1, "2001:b28:f23d:f001::a", 443, dc_flags::IPV6));
            self.endpoints.push(DcEndpoint::new(2, "2001:67c:4e8:f002::a", 443, dc_flags::IPV6));
            self.endpoints.push(DcEndpoint::new(3, "2001:b28:f23d:f003::a", 443, dc_flags::IPV6));
            self.endpoints.push(DcEndpoint::new(4, "2001:67c:4e8:f004::a", 443, dc_flags::IPV6));
            self.endpoints.push(DcEndpoint::new(5, "2001:b28:f23f:f005::a", 443, dc_flags::IPV6));
        }
    }

    pub fn add_endpoint(&mut self, endpoint: DcEndpoint) {
        if let Some(pos) = self.endpoints.iter().position(|e| e.id == endpoint.id && e.ip == endpoint.ip && e.port == endpoint.port) {
            self.endpoints[pos] = endpoint;
        } else {
            self.endpoints.push(endpoint);
        }
    }

    pub fn lookup(&self, dc_id: DcId, prefer_ipv6: bool, prefer_media: bool) -> Option<&DcEndpoint> {
        let matching: Vec<&DcEndpoint> = self.endpoints.iter().filter(|e| e.id == dc_id).collect();
        if matching.is_empty() {
            return None;
        }

        // Try exact match on prefer_ipv6 and prefer_media
        if let Some(&ep) = matching.iter().find(|e| e.is_ipv6() == prefer_ipv6 && e.is_media_only() == prefer_media) {
            return Some(ep);
        }

        // Try match on prefer_ipv6
        if let Some(&ep) = matching.iter().find(|e| e.is_ipv6() == prefer_ipv6) {
            return Some(ep);
        }

        matching.first().copied()
    }

    pub fn dc_type(&self, shifted_dc_id: ShiftedDcId) -> DcType {
        let bare = bare_dc_id(shifted_dc_id);
        if let Some(ep) = self.endpoints.iter().find(|e| e.id == bare) {
            if ep.is_cdn() {
                return DcType::Cdn;
            }
        }
        let shift = shifted_dc_id / crate::mtproto::core_types::DC_SHIFT;
        if (crate::mtproto::core_types::BASE_DOWNLOAD_DC_SHIFT..crate::mtproto::core_types::BASE_DOWNLOAD_DC_SHIFT + crate::mtproto::core_types::MAX_MEDIA_DC_COUNT).contains(&shift) {
            DcType::MediaCluster
        } else {
            DcType::Regular
        }
    }
}
