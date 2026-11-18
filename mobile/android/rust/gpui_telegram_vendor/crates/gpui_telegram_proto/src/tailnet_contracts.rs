//! Tailscale-like zero-service userspace mesh contracts (control plane, DERP relay,
//! Disco NAT traversal, and E2E PeerAPI RPC).
//!
//! Shared across `gpui_telegram_proto`, `gpui_telegram_client`, `gpui_telegram_server`,
//! and `gpui_telegram_desktop`.

use crate::computer_contracts::{
    AuthorizedControllerEntry, ComputerRunPolicy, PendingComputerCommandDto,
    RemoteComputerCommandResult, RemoteComputerOp,
};
use serde::{Deserialize, Serialize};

/// Active transport mode for a Tailnet mesh node or peer link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TailnetRelayMode {
    DirectUdp,
    DerpRelay,
    #[default]
    HttpFallback,
    Offline,
}

impl TailnetRelayMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DirectUdp => "direct_udp",
            Self::DerpRelay => "derp_relay",
            Self::HttpFallback => "http_fallback",
            Self::Offline => "offline",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "direct_udp" | "direct" | "p2p" | "udp" => Self::DirectUdp,
            "derp_relay" | "derp" | "relay" => Self::DerpRelay,
            "http_fallback" | "http" | "keepalive" | "http_keepalive" => Self::HttpFallback,
            "offline" => Self::Offline,
            _ => Self::HttpFallback,
        }
    }

    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::DirectUdp => "P2P 直连",
            Self::DerpRelay => "DERP 加密中继",
            Self::HttpFallback => "HTTP 保活",
            Self::Offline => "离线",
        }
    }

    pub fn label_en(&self) -> &'static str {
        match self {
            Self::DirectUdp => "P2P Direct",
            Self::DerpRelay => "DERP Encrypted Relay",
            Self::HttpFallback => "HTTP Keepalive",
            Self::Offline => "Offline",
        }
    }
}

/// Classification of a peer's reachable UDP socket endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TailnetEndpointKind {
    #[default]
    Lan,
    StunReflexive,
    Loopback,
}

impl TailnetEndpointKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lan => "lan",
            Self::StunReflexive => "stun_reflexive",
            Self::Loopback => "loopback",
        }
    }
}

/// Reachable `ip:port` candidate advertised by a Tailnet node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TailnetEndpoint {
    pub addr: String,
    pub kind: TailnetEndpointKind,
}

/// Signed device registration / endpoint refresh payload sent to `POST /api/v1/tailnet/register`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TailnetRegisterRequest {
    pub device_id: String,
    pub display_name: String,
    pub lan_ip: String,
    pub os_name: String,
    pub resolution: String,
    pub machine_public_key: String,
    pub node_public_key: String,
    pub disco_public_key: String,
    pub endpoints: Vec<TailnetEndpoint>,
    pub timestamp_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_hex: Option<String>,
}

impl TailnetRegisterRequest {
    /// Returns the canonical payload signed by the node's Ed25519 MachineKey.
    pub fn canonical_signing_message(&self, owner_user_id: u64) -> String {
        format!(
            "gpui-tailnet-reg-v1|{owner_user_id}|{}|{}|{}|{}|{}",
            self.device_id.trim(),
            self.machine_public_key.trim(),
            self.node_public_key.trim(),
            self.disco_public_key.trim(),
            self.timestamp_ms
        )
    }
}

/// Control-plane descriptor for a node participating in a user's Tailnet mesh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TailnetPeerDescriptor {
    pub node_id: String,
    pub device_id: String,
    pub display_name: String,
    pub tailnet_ip: String,
    pub lan_ip: String,
    pub os_name: String,
    pub resolution: String,
    pub machine_public_key: String,
    pub node_public_key: String,
    pub disco_public_key: String,
    pub endpoints: Vec<TailnetEndpoint>,
    pub derp_region: String,
    pub derp_connected: bool,
    pub online: bool,
    pub relay_mode: TailnetRelayMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    pub run_policy: ComputerRunPolicy,
    pub authorized_controllers: Vec<AuthorizedControllerEntry>,
    pub last_seen_ms: i64,
}

/// Network map distributed by the coordination server to a Tailnet node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TailnetNetMap {
    pub owner_user_id: u64,
    pub seq: u64,
    pub generated_at_ms: i64,
    pub derp_region: String,
    pub derp_ws_path: String,
    pub stun_http_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_peer: Option<TailnetPeerDescriptor>,
    pub peers: Vec<TailnetPeerDescriptor>,
}

/// HTTP STUN reflection response from `GET /api/v1/tailnet/stun`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TailnetStunResponse {
    pub observed_ip: String,
    pub observed_port: u16,
    pub server_time_ms: i64,
}

/// Client-to-server WebSocket frames sent over `/api/v1/tailnet/derp`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DerpClientFrame {
    Hello {
        device_id: String,
        node_public_key: String,
        disco_public_key: String,
        #[serde(default)]
        endpoints: Vec<TailnetEndpoint>,
    },
    SendPacket {
        dst_node_key: String,
        packet_b64: String,
    },
    KeepAlive {
        client_time_ms: i64,
        #[serde(default)]
        endpoints: Vec<TailnetEndpoint>,
    },
    CommandResult {
        command_id: String,
        result: RemoteComputerCommandResult,
    },
}

/// Server-to-client WebSocket frames delivered over `/api/v1/tailnet/derp`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DerpServerFrame {
    Welcome {
        derp_region: String,
        server_time_ms: i64,
        netmap: TailnetNetMap,
    },
    RecvPacket {
        src_node_key: String,
        packet_b64: String,
    },
    NetMapUpdate {
        netmap: TailnetNetMap,
    },
    KeepAliveAck {
        client_time_ms: i64,
        server_time_ms: i64,
    },
    ServerCommandDispatch {
        command: PendingComputerCommandDto,
    },
    Error {
        code: String,
        message: String,
    },
}

/// End-to-end encrypted NAT discovery and path-probing frames (`TSDC`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disco", rename_all = "snake_case")]
pub enum DiscoPayload {
    Ping {
        tx_id: String,
        sent_at_ms: i64,
        src_node_key: String,
    },
    Pong {
        tx_id: String,
        sent_at_ms: i64,
        observed_addr: String,
        src_node_key: String,
    },
    CallMeMaybe {
        src_node_key: String,
        #[serde(default)]
        my_endpoints: Vec<TailnetEndpoint>,
    },
}

/// End-to-end encrypted PeerAPI RPC frames carried inside `TSWG` packets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rpc", rename_all = "snake_case")]
pub enum PeerApiFrame {
    ExecRequest {
        request_id: String,
        caller_device_id: String,
        caller_node_key: String,
        caller_label: String,
        operation: RemoteComputerOp,
    },
    ExecResponse {
        request_id: String,
        result: RemoteComputerCommandResult,
    },
    ApprovalPending {
        request_id: String,
        command_id: String,
        message: String,
    },
}

/// Live link-level telemetry for a single remote peer in the local Tailnet engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TailnetPeerLiveState {
    pub node_id: String,
    pub device_id: String,
    pub tailnet_ip: String,
    pub lan_ip: String,
    pub relay_mode: TailnetRelayMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<u32>,
    pub online: bool,
}

/// Snapshot of the local node's Tailnet mesh engine state for UI and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TailnetNodeStatus {
    pub running: bool,
    pub device_id: String,
    pub tailnet_ip: String,
    pub lan_ip: String,
    pub machine_public_key: String,
    pub node_public_key: String,
    pub local_udp_port: u16,
    pub derp_connected: bool,
    pub relay_mode: TailnetRelayMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derp_rtt_ms: Option<u32>,
    pub netmap_seq: u64,
    pub peers: Vec<TailnetPeerLiveState>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer_contracts::AuthorizedControllerKind;

    fn sample_peer(node_id: &str, device_id: &str, tailnet_ip: &str) -> TailnetPeerDescriptor {
        TailnetPeerDescriptor {
            node_id: node_id.to_string(),
            device_id: device_id.to_string(),
            display_name: "MacBook Pro".to_string(),
            tailnet_ip: tailnet_ip.to_string(),
            lan_ip: "192.168.1.42".to_string(),
            os_name: "macOS 15.2".to_string(),
            resolution: "2560x1600".to_string(),
            machine_public_key: "aa".repeat(32),
            node_public_key: "bb".repeat(32),
            disco_public_key: "cc".repeat(32),
            endpoints: vec![
                TailnetEndpoint {
                    addr: "192.168.1.42:41641".to_string(),
                    kind: TailnetEndpointKind::Lan,
                },
                TailnetEndpoint {
                    addr: "127.0.0.1:41641".to_string(),
                    kind: TailnetEndpointKind::Loopback,
                },
            ],
            derp_region: "primary".to_string(),
            derp_connected: true,
            online: true,
            relay_mode: TailnetRelayMode::DirectUdp,
            latency_ms: Some(4),
            run_policy: ComputerRunPolicy::AlwaysAllowAuthorized,
            authorized_controllers: vec![AuthorizedControllerEntry {
                controller_id: "account_all_devices".to_string(),
                kind: AuthorizedControllerKind::Device,
                display_name: "All My Devices".to_string(),
                added_at_ms: 1_700_000_000_000,
            }],
            last_seen_ms: 1_700_000_001_000,
        }
    }

    #[test]
    fn test_tailnet_relay_mode_roundtrip_parse_and_labels() {
        let cases = [
            (
                TailnetRelayMode::DirectUdp,
                "\"direct_udp\"",
                "direct_udp",
                "P2P 直连",
                "P2P Direct",
            ),
            (
                TailnetRelayMode::DerpRelay,
                "\"derp_relay\"",
                "derp_relay",
                "DERP 加密中继",
                "DERP Encrypted Relay",
            ),
            (
                TailnetRelayMode::HttpFallback,
                "\"http_fallback\"",
                "http_fallback",
                "HTTP 保活",
                "HTTP Keepalive",
            ),
            (
                TailnetRelayMode::Offline,
                "\"offline\"",
                "offline",
                "离线",
                "Offline",
            ),
        ];

        assert_eq!(TailnetRelayMode::default(), TailnetRelayMode::HttpFallback);

        for (mode, expected_json, expected_str, expected_zh, expected_en) in cases {
            let json = serde_json::to_string(&mode).expect("serialize relay mode");
            assert_eq!(json, expected_json);
            let parsed: TailnetRelayMode =
                serde_json::from_str(&json).expect("deserialize relay mode");
            assert_eq!(parsed, mode);
            assert_eq!(mode.as_str(), expected_str);
            assert_eq!(TailnetRelayMode::parse(expected_str), mode);
            assert_eq!(mode.label_zh(), expected_zh);
            assert_eq!(mode.label_en(), expected_en);
        }

        assert_eq!(
            TailnetRelayMode::parse("  DIRECT_UDP "),
            TailnetRelayMode::DirectUdp
        );
        assert_eq!(
            TailnetRelayMode::parse("unknown_mode"),
            TailnetRelayMode::HttpFallback
        );
    }

    #[test]
    fn test_endpoint_kind_and_endpoint_roundtrip() {
        assert_eq!(TailnetEndpointKind::default(), TailnetEndpointKind::Lan);

        let kinds = [
            (TailnetEndpointKind::Lan, "\"lan\"", "lan"),
            (
                TailnetEndpointKind::StunReflexive,
                "\"stun_reflexive\"",
                "stun_reflexive",
            ),
            (TailnetEndpointKind::Loopback, "\"loopback\"", "loopback"),
        ];
        for (kind, expected_json, expected_str) in kinds {
            let json = serde_json::to_string(&kind).expect("serialize endpoint kind");
            assert_eq!(json, expected_json);
            assert_eq!(kind.as_str(), expected_str);
            let parsed: TailnetEndpointKind =
                serde_json::from_str(&json).expect("deserialize endpoint kind");
            assert_eq!(parsed, kind);
        }

        let endpoint = TailnetEndpoint {
            addr: "203.0.113.5:51820".to_string(),
            kind: TailnetEndpointKind::StunReflexive,
        };
        let encoded = serde_json::to_string(&endpoint).expect("serialize endpoint");
        let decoded: TailnetEndpoint =
            serde_json::from_str(&encoded).expect("deserialize endpoint");
        assert_eq!(decoded, endpoint);

        let partial: TailnetEndpoint =
            serde_json::from_str(r#"{"addr":"10.0.0.2:41641"}"#).expect("partial endpoint");
        assert_eq!(partial.kind, TailnetEndpointKind::Lan);
    }

    #[test]
    fn test_register_request_canonical_signing_message_and_roundtrip() {
        let mut req = TailnetRegisterRequest {
            device_id: "  dev-ts-0123456789abcdef  ".to_string(),
            display_name: "Studio Mac".to_string(),
            lan_ip: "192.168.1.10".to_string(),
            os_name: "macOS".to_string(),
            resolution: "3840x2160".to_string(),
            machine_public_key: " 11".repeat(1) + &"11".repeat(31) + " ",
            node_public_key: " 22".to_string() + &"22".repeat(31) + "\n",
            disco_public_key: " 33".to_string() + &"33".repeat(31) + "\t",
            endpoints: vec![TailnetEndpoint {
                addr: "192.168.1.10:41641".to_string(),
                kind: TailnetEndpointKind::Lan,
            }],
            timestamp_ms: 1_730_000_000_123,
            signature_hex: None,
        };

        let expected_msg = format!(
            "gpui-tailnet-reg-v1|42|dev-ts-0123456789abcdef|{}|{}|{}|1730000000123",
            "11".repeat(32),
            "22".repeat(32),
            "33".repeat(32)
        );
        assert_eq!(req.canonical_signing_message(42), expected_msg);

        let val_no_sig = serde_json::to_value(&req).expect("serialize without signature");
        assert!(val_no_sig.get("signature_hex").is_none());

        req.signature_hex = Some("deadbeef".to_string());
        let json = serde_json::to_string(&req).expect("serialize with signature");
        let parsed: TailnetRegisterRequest =
            serde_json::from_str(&json).expect("deserialize register request");
        assert_eq!(parsed, req);
    }

    #[test]
    fn test_netmap_and_stun_roundtrips() {
        let self_peer = sample_peer("local:7:dev-a", "dev-a", "100.64.0.10");
        let remote_peer = sample_peer("local:7:dev-b", "dev-b", "100.64.0.11");
        let netmap = TailnetNetMap {
            owner_user_id: 7,
            seq: 19,
            generated_at_ms: 1_730_000_005_000,
            derp_region: "primary".to_string(),
            derp_ws_path: "/api/v1/tailnet/derp".to_string(),
            stun_http_path: "/api/v1/tailnet/stun".to_string(),
            self_peer: Some(self_peer),
            peers: vec![remote_peer],
        };
        let encoded = serde_json::to_string(&netmap).expect("serialize netmap");
        let decoded: TailnetNetMap = serde_json::from_str(&encoded).expect("deserialize netmap");
        assert_eq!(decoded, netmap);

        let partial_netmap: TailnetNetMap =
            serde_json::from_str(r#"{"owner_user_id":9,"seq":2}"#).expect("partial netmap");
        assert_eq!(partial_netmap.owner_user_id, 9);
        assert_eq!(partial_netmap.seq, 2);
        assert!(partial_netmap.self_peer.is_none());
        assert!(partial_netmap.peers.is_empty());

        let stun = TailnetStunResponse {
            observed_ip: "198.51.100.99".to_string(),
            observed_port: 54321,
            server_time_ms: 1_730_000_006_000,
        };
        let stun_json = serde_json::to_string(&stun).expect("serialize stun");
        let stun_parsed: TailnetStunResponse =
            serde_json::from_str(&stun_json).expect("deserialize stun");
        assert_eq!(stun_parsed, stun);
    }

    #[test]
    fn test_derp_client_and_server_frames_roundtrip() {
        let client_frames = vec![
            (
                DerpClientFrame::Hello {
                    device_id: "dev-ts-1".to_string(),
                    node_public_key: "aa".repeat(32),
                    disco_public_key: "bb".repeat(32),
                    endpoints: vec![TailnetEndpoint {
                        addr: "192.168.1.2:40000".to_string(),
                        kind: TailnetEndpointKind::Lan,
                    }],
                },
                "hello",
            ),
            (
                DerpClientFrame::SendPacket {
                    dst_node_key: "cc".repeat(32),
                    packet_b64: "VFNXRw==".to_string(),
                },
                "send_packet",
            ),
            (
                DerpClientFrame::KeepAlive {
                    client_time_ms: 1_730_000_010_000,
                    endpoints: Vec::new(),
                },
                "keep_alive",
            ),
            (
                DerpClientFrame::CommandResult {
                    command_id: "cmd-123".to_string(),
                    result: RemoteComputerCommandResult {
                        command_id: "cmd-123".to_string(),
                        node_id: "local:7:dev-ts-1".to_string(),
                        ok: true,
                        exit_code: 0,
                        stdout: "ok".to_string(),
                        stderr: String::new(),
                        entries: vec!["a.txt".to_string()],
                        frame_data_url: None,
                        executed_by: "tailnet-derp".to_string(),
                        duration_ms: 12,
                    },
                },
                "command_result",
            ),
        ];

        for (frame, expected_type) in client_frames {
            let val = serde_json::to_value(&frame).expect("serialize DerpClientFrame");
            assert_eq!(
                val.get("type").and_then(|v| v.as_str()),
                Some(expected_type)
            );
            let parsed: DerpClientFrame =
                serde_json::from_value(val).expect("deserialize DerpClientFrame");
            assert_eq!(parsed, frame);
        }

        let netmap = TailnetNetMap {
            owner_user_id: 7,
            seq: 1,
            generated_at_ms: 1_730_000_000_000,
            derp_region: "primary".to_string(),
            derp_ws_path: "/api/v1/tailnet/derp".to_string(),
            stun_http_path: "/api/v1/tailnet/stun".to_string(),
            self_peer: None,
            peers: Vec::new(),
        };
        let server_frames = vec![
            (
                DerpServerFrame::Welcome {
                    derp_region: "primary".to_string(),
                    server_time_ms: 1_730_000_000_001,
                    netmap: netmap.clone(),
                },
                "welcome",
            ),
            (
                DerpServerFrame::RecvPacket {
                    src_node_key: "dd".repeat(32),
                    packet_b64: "VFNXRw==".to_string(),
                },
                "recv_packet",
            ),
            (
                DerpServerFrame::NetMapUpdate { netmap },
                "net_map_update",
            ),
            (
                DerpServerFrame::KeepAliveAck {
                    client_time_ms: 1_730_000_010_000,
                    server_time_ms: 1_730_000_010_005,
                },
                "keep_alive_ack",
            ),
            (
                DerpServerFrame::ServerCommandDispatch {
                    command: PendingComputerCommandDto {
                        command_id: "cmd-99".to_string(),
                        node_id: "local:7:dev-ts-1".to_string(),
                        target_device_id: "dev-ts-1".to_string(),
                        caller_device_id: None,
                        caller_bot_id: Some(777_001),
                        caller_label: "OpsBot".to_string(),
                        requires_user_approval: false,
                        approved: true,
                        operation: RemoteComputerOp::CaptureScreen,
                        created_at_ms: 1_730_000_010_100,
                    },
                },
                "server_command_dispatch",
            ),
            (
                DerpServerFrame::Error {
                    code: "bad_frame".to_string(),
                    message: "invalid payload".to_string(),
                },
                "error",
            ),
        ];

        for (frame, expected_type) in server_frames {
            let val = serde_json::to_value(&frame).expect("serialize DerpServerFrame");
            assert_eq!(
                val.get("type").and_then(|v| v.as_str()),
                Some(expected_type)
            );
            let parsed: DerpServerFrame =
                serde_json::from_value(val).expect("deserialize DerpServerFrame");
            assert_eq!(parsed, frame);
        }
    }

    #[test]
    fn test_disco_and_peer_api_frames_and_live_status_roundtrip() {
        let discos = vec![
            (
                DiscoPayload::Ping {
                    tx_id: "tx-1".to_string(),
                    sent_at_ms: 1000,
                    src_node_key: "aa".repeat(32),
                },
                "ping",
            ),
            (
                DiscoPayload::Pong {
                    tx_id: "tx-1".to_string(),
                    sent_at_ms: 1000,
                    observed_addr: "192.168.1.50:55555".to_string(),
                    src_node_key: "bb".repeat(32),
                },
                "pong",
            ),
            (
                DiscoPayload::CallMeMaybe {
                    src_node_key: "aa".repeat(32),
                    my_endpoints: vec![TailnetEndpoint {
                        addr: "192.168.1.42:41641".to_string(),
                        kind: TailnetEndpointKind::Lan,
                    }],
                },
                "call_me_maybe",
            ),
        ];

        for (payload, expected_tag) in discos {
            let val = serde_json::to_value(&payload).expect("serialize DiscoPayload");
            assert_eq!(
                val.get("disco").and_then(|v| v.as_str()),
                Some(expected_tag)
            );
            let parsed: DiscoPayload =
                serde_json::from_value(val).expect("deserialize DiscoPayload");
            assert_eq!(parsed, payload);
        }

        let rpcs = vec![
            (
                PeerApiFrame::ExecRequest {
                    request_id: "req-1".to_string(),
                    caller_device_id: "dev-ts-a".to_string(),
                    caller_node_key: "aa".repeat(32),
                    caller_label: "My Laptop".to_string(),
                    operation: RemoteComputerOp::ShellExec {
                        command: "uname -a".to_string(),
                        cwd: Some("/tmp".to_string()),
                        timeout_ms: Some(3000),
                    },
                },
                "exec_request",
            ),
            (
                PeerApiFrame::ExecResponse {
                    request_id: "req-1".to_string(),
                    result: RemoteComputerCommandResult {
                        command_id: "p2p:req-1".to_string(),
                        node_id: "local:7:dev-ts-b".to_string(),
                        ok: true,
                        exit_code: 0,
                        stdout: "Darwin".to_string(),
                        stderr: String::new(),
                        entries: Vec::new(),
                        frame_data_url: None,
                        executed_by: "tailnet-p2p".to_string(),
                        duration_ms: 3,
                    },
                },
                "exec_response",
            ),
            (
                PeerApiFrame::ApprovalPending {
                    request_id: "req-2".to_string(),
                    command_id: "p2p:abc".to_string(),
                    message: "waiting for user approval".to_string(),
                },
                "approval_pending",
            ),
        ];

        for (rpc, expected_tag) in rpcs {
            let val = serde_json::to_value(&rpc).expect("serialize PeerApiFrame");
            assert_eq!(val.get("rpc").and_then(|v| v.as_str()), Some(expected_tag));
            let parsed: PeerApiFrame =
                serde_json::from_value(val).expect("deserialize PeerApiFrame");
            assert_eq!(parsed, rpc);
        }

        let status = TailnetNodeStatus {
            running: true,
            device_id: "dev-ts-a".to_string(),
            tailnet_ip: "100.64.0.10".to_string(),
            lan_ip: "192.168.1.42".to_string(),
            machine_public_key: "11".repeat(32),
            node_public_key: "22".repeat(32),
            local_udp_port: 41641,
            derp_connected: true,
            relay_mode: TailnetRelayMode::DirectUdp,
            derp_rtt_ms: Some(18),
            netmap_seq: 42,
            peers: vec![TailnetPeerLiveState {
                node_id: "local:7:dev-ts-b".to_string(),
                device_id: "dev-ts-b".to_string(),
                tailnet_ip: "100.64.0.11".to_string(),
                lan_ip: "192.168.1.43".to_string(),
                relay_mode: TailnetRelayMode::DirectUdp,
                active_endpoint: Some("192.168.1.43:41641".to_string()),
                rtt_ms: Some(2),
                online: true,
            }],
        };

        let status_json = serde_json::to_string(&status).expect("serialize TailnetNodeStatus");
        let parsed_status: TailnetNodeStatus =
            serde_json::from_str(&status_json).expect("deserialize TailnetNodeStatus");
        assert_eq!(parsed_status, status);

        let default_status: TailnetNodeStatus =
            serde_json::from_str("{}").expect("default TailnetNodeStatus");
        assert!(!default_status.running);
        assert_eq!(default_status.relay_mode, TailnetRelayMode::HttpFallback);
        assert!(default_status.derp_rtt_ms.is_none());
    }
}
