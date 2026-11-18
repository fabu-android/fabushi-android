//! TDRP protocol module re-exporting coordinator modules from inherited `mahayana-node-agent-coordinator`.

pub use mahayana_node_agent_coordinator::tdrp::{
    calls_group_calls, chat_admin_moderation, export_payments_bots, folders_filters, forum_topics,
    polls_todo, settings_cloud_passkeys, stories_gifts_stars,
};
pub use mahayana_node_agent_coordinator::tdrp::TdrpCoordinatorState;
