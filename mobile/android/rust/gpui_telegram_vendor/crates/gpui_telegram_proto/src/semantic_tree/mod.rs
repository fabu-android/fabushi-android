use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SemanticWindowNode {
    pub layout: String,
    pub width_px: f32,
    pub height_px: f32,
    pub active_panel: String,
    pub is_authenticated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SemanticNavRailNode {
    pub active_destination: String,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SemanticSidebarNode {
    pub dialog_count: usize,
    pub selected_chat_id: Option<u64>,
    pub search_query: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SemanticStageNode {
    pub active_chat_id: Option<u64>,
    pub chat_title: String,
    pub message_count: usize,
    pub is_group_chat: bool,
    pub pinned_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SemanticComposerNode {
    pub text: String,
    pub is_editing: bool,
    pub has_reply_target: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SemanticInspectorNode {
    pub is_open: bool,
    pub profile_name: String,
    pub shared_media_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SemanticModalNode {
    pub modal_id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticWorkspaceTree {
    pub timestamp_ms: i64,
    pub window_layout: String,
    pub selected_chat_id: Option<u64>,
    pub composer_text: String,
    pub active_modal: Option<String>,
    pub is_call_active: bool,
    #[serde(default)]
    pub window: SemanticWindowNode,
    #[serde(default)]
    pub nav_rail: SemanticNavRailNode,
    #[serde(default)]
    pub sidebar: SemanticSidebarNode,
    #[serde(default)]
    pub stage: SemanticStageNode,
    #[serde(default)]
    pub composer: SemanticComposerNode,
    #[serde(default)]
    pub inspector: SemanticInspectorNode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_modal_node: Option<SemanticModalNode>,
    pub state_hash: String,
}

fn hash_str(hasher: &mut Sha256, s: &str) {
    hasher.update((s.len() as u64).to_le_bytes());
    hasher.update(s.as_bytes());
}

fn hash_opt_str(hasher: &mut Sha256, s: Option<&str>) {
    match s {
        None => hasher.update([0u8]),
        Some(v) => {
            hasher.update([1u8]);
            hash_str(hasher, v);
        }
    }
}

fn hash_opt_u64(hasher: &mut Sha256, v: Option<u64>) {
    match v {
        None => hasher.update([0u8]),
        Some(n) => {
            hasher.update([1u8]);
            hasher.update(n.to_le_bytes());
        }
    }
}

fn finalize_hex(hasher: Sha256) -> String {
    let digest = hasher.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

impl SemanticWorkspaceTree {
    pub fn compute_hash_parts(
        layout: &str,
        chat_id: Option<u64>,
        text: &str,
        modal: Option<&str>,
        call: bool,
    ) -> String {
        let mut hasher = Sha256::new();
        hash_str(&mut hasher, layout);
        hash_opt_u64(&mut hasher, chat_id);
        hash_str(&mut hasher, text);
        hash_opt_str(&mut hasher, modal);
        hasher.update([u8::from(call)]);
        finalize_hex(hasher)
    }

    pub fn compute_full_state_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hash_str(&mut hasher, &self.window_layout);
        hash_opt_u64(&mut hasher, self.selected_chat_id);
        hash_str(&mut hasher, &self.composer_text);
        hash_opt_str(&mut hasher, self.active_modal.as_deref());
        hasher.update([u8::from(self.is_call_active)]);

        // Window node
        hash_str(&mut hasher, &self.window.layout);
        hasher.update(self.window.width_px.to_bits().to_le_bytes());
        hasher.update(self.window.height_px.to_bits().to_le_bytes());
        hash_str(&mut hasher, &self.window.active_panel);
        hasher.update([u8::from(self.window.is_authenticated)]);

        // Nav rail node
        hash_str(&mut hasher, &self.nav_rail.active_destination);
        hasher.update([u8::from(self.nav_rail.connected)]);

        // Sidebar node
        hasher.update((self.sidebar.dialog_count as u64).to_le_bytes());
        hash_opt_u64(&mut hasher, self.sidebar.selected_chat_id);
        hash_str(&mut hasher, &self.sidebar.search_query);

        // Stage node
        hash_opt_u64(&mut hasher, self.stage.active_chat_id);
        hash_str(&mut hasher, &self.stage.chat_title);
        hasher.update((self.stage.message_count as u64).to_le_bytes());
        hasher.update([u8::from(self.stage.is_group_chat)]);
        hash_opt_str(&mut hasher, self.stage.pinned_message.as_deref());

        // Composer node
        hash_str(&mut hasher, &self.composer.text);
        hasher.update([u8::from(self.composer.is_editing)]);
        hasher.update([u8::from(self.composer.has_reply_target)]);

        // Inspector node
        hasher.update([u8::from(self.inspector.is_open)]);
        hash_str(&mut hasher, &self.inspector.profile_name);
        hasher.update((self.inspector.shared_media_count as u64).to_le_bytes());

        // Active modal node
        match &self.active_modal_node {
            None => hasher.update([0u8]),
            Some(m) => {
                hasher.update([1u8]);
                hash_str(&mut hasher, &m.modal_id);
                hash_str(&mut hasher, &m.title);
            }
        }

        finalize_hex(hasher)
    }

    pub fn recompute_state_hash(&mut self) {
        self.state_hash = self.compute_full_state_hash();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", content = "params")]
pub enum SemanticAction {
    SelectChat { chat_id: u64 },
    ComposeText { text: String },
    SendMessage,
    OpenModal { modal_id: String },
    CloseModal,
    ToggleCall { peer_id: u64, is_video: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionExecutionStatus {
    Succeeded,
    Failed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticActionReceipt {
    pub action_id: String,
    pub action_type: String,
    pub success: bool,
    pub status: ActionExecutionStatus,
    pub error: Option<String>,
    pub timestamp_ms: i64,
    pub mutation_summary: serde_json::Value,
    pub resulting_state_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_tree_hash_deterministic() {
        let h1 = SemanticWorkspaceTree::compute_hash_parts("Normal", Some(42), "hi", None, false);
        let h2 = SemanticWorkspaceTree::compute_hash_parts("Normal", Some(42), "hi", None, false);
        let h3 = SemanticWorkspaceTree::compute_hash_parts("Normal", Some(43), "hi", None, false);
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64);

        // No delimiter or sentinel collisions
        assert_ne!(
            SemanticWorkspaceTree::compute_hash_parts("Normal", Some(1), "foo|bar", Some("baz"), false),
            SemanticWorkspaceTree::compute_hash_parts("Normal", Some(1), "foo", Some("bar|baz"), false)
        );
        assert_ne!(
            SemanticWorkspaceTree::compute_hash_parts("Normal", None, "hi", None, false),
            SemanticWorkspaceTree::compute_hash_parts("Normal", Some(0), "hi", None, false)
        );
        assert_ne!(
            SemanticWorkspaceTree::compute_hash_parts("Normal", Some(1), "hi", None, false),
            SemanticWorkspaceTree::compute_hash_parts("Normal", Some(1), "hi", Some("none"), false)
        );

        let mut t1 = SemanticWorkspaceTree {
            timestamp_ms: 1,
            window_layout: "Normal".into(),
            selected_chat_id: Some(1),
            composer_text: String::new(),
            active_modal: None,
            is_call_active: false,
            window: SemanticWindowNode::default(),
            nav_rail: SemanticNavRailNode::default(),
            sidebar: SemanticSidebarNode::default(),
            stage: SemanticStageNode::default(),
            composer: SemanticComposerNode::default(),
            inspector: SemanticInspectorNode::default(),
            active_modal_node: None,
            state_hash: String::new(),
        };
        t1.recompute_state_hash();
        let mut t2 = t1.clone();
        t2.stage.message_count = 1;
        t2.recompute_state_hash();
        assert_ne!(t1.state_hash, t2.state_hash);

        t2.inspector.is_open = true;
        t2.recompute_state_hash();
        assert_ne!(t1.state_hash, t2.state_hash);
    }
}
