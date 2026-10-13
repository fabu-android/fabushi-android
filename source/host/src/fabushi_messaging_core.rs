//! Responsibility-level direct port from Desktop native/mahayana-messaging.
//! This module intentionally owns only the recipient authorization and forward-privacy
//! policies currently consumed by the Android Host. It is not evidence that the entire
//! Desktop messaging crate has been migrated.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationKind {
    Direct,
    Group,
    Channel,
    SavedMessages,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipientContentKind {
    Standard,
    Secret,
    Service,
}

impl Default for RecipientContentKind {
    fn default() -> Self { Self::Standard }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecipientSearchRequirements {
    pub require_media: bool,
    pub require_polls: bool,
    pub require_send_other: bool,
    pub require_inline: bool,
    pub require_game: bool,
    pub source_protected_content: bool,
    pub content_kind: RecipientContentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipientAuthorizationInput {
    pub conversation_kind: ConversationKind,
    pub sender_is_participant: bool,
    pub can_send_messages: bool,
    pub can_send_media: bool,
    pub can_send_polls: bool,
    pub can_send_other: bool,
    pub can_send_inline: bool,
    pub can_send_games: bool,
    pub community_forbidden: bool,
    pub community_restrict_messages: bool,
    pub community_restrict_media: bool,
    pub community_restrict_polls: bool,
    pub community_restrict_other: bool,
    pub community_restrict_inline: bool,
    pub community_restrict_games: bool,
    pub channel_posting_allowed: bool,
}

impl RecipientAuthorizationInput {
    pub fn direct_default(sender_is_participant: bool) -> Self {
        Self {
            conversation_kind: ConversationKind::Direct,
            sender_is_participant,
            can_send_messages: true,
            can_send_media: true,
            can_send_polls: true,
            can_send_other: true,
            can_send_inline: true,
            can_send_games: true,
            community_forbidden: false,
            community_restrict_messages: false,
            community_restrict_media: false,
            community_restrict_polls: false,
            community_restrict_other: false,
            community_restrict_inline: false,
            community_restrict_games: false,
            channel_posting_allowed: true,
        }
    }
}

pub fn recipient_search_authorized(
    input: RecipientAuthorizationInput,
    requirements: RecipientSearchRequirements,
) -> bool {
    if requirements.source_protected_content
        || !input.sender_is_participant
        || !input.can_send_messages
        || input.community_forbidden
        || input.community_restrict_messages
        || (requirements.require_media && (!input.can_send_media || input.community_restrict_media))
        || (requirements.require_polls && (!input.can_send_polls || input.community_restrict_polls))
        || (requirements.require_send_other && (!input.can_send_other || input.community_restrict_other))
        || (requirements.require_inline && (!input.can_send_inline || input.community_restrict_inline))
        || (requirements.require_game && (!input.can_send_games || input.community_restrict_games))
    {
        return false;
    }
    if matches!(input.conversation_kind, ConversationKind::Channel)
        && (!input.channel_posting_allowed || requirements.require_game)
    {
        return false;
    }
    match (requirements.content_kind, input.conversation_kind) {
        (RecipientContentKind::Standard, ConversationKind::Secret) => false,
        (RecipientContentKind::Secret, ConversationKind::Secret) => true,
        (RecipientContentKind::Secret, _) => false,
        (RecipientContentKind::Service, _) | (RecipientContentKind::Standard, _) => true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ForwardPrivacy {
    pub drop_sender_names: bool,
    pub drop_captions: bool,
}

impl ForwardPrivacy {
    pub fn normalized(self) -> Self {
        Self {
            drop_sender_names: self.drop_sender_names || self.drop_captions,
            drop_captions: self.drop_captions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_and_non_participant_destinations_fail_closed() {
        let direct = RecipientAuthorizationInput::direct_default(true);
        assert!(recipient_search_authorized(direct, RecipientSearchRequirements::default()));
        assert!(!recipient_search_authorized(
            direct,
            RecipientSearchRequirements { source_protected_content: true, ..Default::default() },
        ));
        assert!(!recipient_search_authorized(
            RecipientAuthorizationInput::direct_default(false),
            RecipientSearchRequirements::default(),
        ));
    }

    #[test]
    fn caption_privacy_also_drops_sender_identity() {
        assert_eq!(
            ForwardPrivacy { drop_sender_names: false, drop_captions: true }.normalized(),
            ForwardPrivacy { drop_sender_names: true, drop_captions: true },
        );
    }
}
