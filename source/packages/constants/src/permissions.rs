use std::collections::BTreeSet;

pub struct TeamPermission;
impl TeamPermission {
    pub const READ_SPEND: &'static str = "team.spend.read";
    pub const READ_TEAM_MEMBER_SPEND: &'static str = "team.member_spend.read";
    pub const READ_PRIVACY_MODE: &'static str = "team.privacy_mode.read";
    pub const READ_SSO_CONFIGURATION: &'static str = "team.sso.read";
    pub const READ_TEAM_RULES: &'static str = "team.rules.read";
    pub const MANAGE_TEAM_RULES: &'static str = "team.rules.manage";
    pub const READ_TEAM_HOOKS: &'static str = "team.hooks.read";
    pub const MANAGE_TEAM_HOOKS: &'static str = "team.hooks.manage";
    pub const READ_COMMIT_METRICS: &'static str = "team.commit_metrics.read";
    pub const MANAGE_DIRECTORY_GROUPS: &'static str = "team.directory_groups.manage";
    pub const MANAGE_PROTECTED_GIT_SCOPES: &'static str = "team.protected_git_scopes.manage";
    pub const READ_BACKGROUND_COMPOSERS: &'static str = "team.background_composers.read";
    pub const LIST_TEAM_BACKGROUND_COMPOSERS: &'static str = "team.background_composers.list";
    pub const MANAGE_TEAM_BACKGROUND_COMPOSERS: &'static str = "team.background_composers.manage";
    pub const MANAGE_FULL_SELF_DRIVING: &'static str = "team.full_self_driving.manage";
    pub const MANAGE_PUBLIC_PROFILE_SETTINGS: &'static str = "team.public_profile_settings.manage";
    pub const MANAGE_MEMBER_SPEND_LIMITS: &'static str = "team.member_spend_limits.manage";
    pub const MANAGE_TEAM_MCP_SERVERS: &'static str = "team.mcp_servers.manage";
    pub const MANAGE_SCIM_CONFIGURATION: &'static str = "team.scim_configuration.manage";
    pub const MANAGE_BILLING_GROUPS: &'static str = "team.billing_groups.manage";
    pub const READ_GROUPS: &'static str = "team.groups.read";
    pub const MANAGE_GROUPS: &'static str = "team.groups.manage";
    pub const READ_TEAM_MEMBERS: &'static str = "team.members.read";
    pub const MANAGE_TEAM_MEMBERS: &'static str = "team.members.manage";
    pub const READ_TEAM_MEMBERSHIP: &'static str = "team.membership.read";
    pub const READ_TEAM_INVITES: &'static str = "team.invites.read";
    pub const MANAGE_TEAM_INVITES: &'static str = "team.invites.manage";
    pub const MANAGE_TEAM_API_KEYS: &'static str = "team.api_keys.manage";
    pub const READ_TEAM_REPOS: &'static str = "team.repos.read";
    pub const MANAGE_TEAM_REPOS: &'static str = "team.repos.manage";
    pub const READ_TEAM_SETTINGS: &'static str = "team.settings.read";
    pub const MANAGE_TEAM_SETTINGS: &'static str = "team.settings.manage";
    pub const READ_TEAM_BILLING: &'static str = "team.billing.read";
    pub const MANAGE_TEAM_BILLING: &'static str = "team.billing.manage";
    pub const READ_AUDIT_LOGS: &'static str = "team.audit_logs.read";
    pub const READ_TEAM_ANALYTICS: &'static str = "team.analytics.read";
    pub const MANAGE_TEAM_ANALYTICS: &'static str = "team.analytics.manage";
    pub const MANAGE_TEAM_PRIVACY: &'static str = "team.privacy.manage";
    pub const READ_TEAM_COMMANDS: &'static str = "team.commands.read";
    pub const MANAGE_TEAM_COMMANDS: &'static str = "team.commands.manage";
    pub const READ_BUGBOT: &'static str = "team.bugbot.read";
    pub const MANAGE_BUGBOT: &'static str = "team.bugbot.manage";
    pub const READ_TEAM_SHARING_SETTINGS: &'static str = "team.sharing_settings.read";
    pub const MANAGE_TEAM_SHARING_SETTINGS: &'static str = "team.sharing_settings.manage";
    pub const MANAGE_TEAM_BACKGROUND_AGENT_SETTINGS: &'static str = "team.background_agent_settings.manage";
    pub const READ_TEAM_PLUGINS: &'static str = "team.plugins.read";
    pub const MANAGE_TEAM_PLUGINS: &'static str = "team.plugins.manage";
    pub const READ_TEAM_INTEGRATIONS: &'static str = "team.integrations.read";
    pub const MANAGE_TEAM_INTEGRATIONS: &'static str = "team.integrations.manage";
}

pub struct OrganizationPermission;
impl OrganizationPermission {
    pub const READ_MEMBERS: &'static str = "organization.members.read";
    pub const WRITE_MEMBERS: &'static str = "organization.members.write";
    pub const MANAGE_MEMBERSHIPS: &'static str = "organization.memberships.manage";
    pub const MANAGE_ORGANIZATION: &'static str = "organization.manage";
    pub const MANAGE_TEAMS: &'static str = "organization.teams.manage";
    pub const READ_GROUPS: &'static str = "organization.groups.read";
    pub const MANAGE_GROUPS: &'static str = "organization.groups.manage";
    pub const MANAGE_API_KEYS: &'static str = "organization.api_keys.manage";
    pub const MANAGE_IDENTITY_PROVIDERS: &'static str = "organization.identity_providers.manage";
    pub const READ_SPEND: &'static str = "organization.spend.read";
    pub const READ_AUDIT_LOGS: &'static str = "organization.audit_logs.read";
    pub const READ_BILLING: &'static str = "organization.billing.read";
}

pub struct AgentStorePermission;
impl AgentStorePermission {
    pub const READ_AGENT_STORE: &'static str = "agent_store.read";
    pub const WRITE_AGENT_STORE: &'static str = "agent_store.write";
    pub const SHARE_AGENT_STORE: &'static str = "agent_store.share";
}

pub struct AgentStoreSharePermission;
impl AgentStoreSharePermission {
    pub const READ_AGENT_STORE_SHARE: &'static str = "agent_store_share.read";
}

pub struct KeyringPermission;
impl KeyringPermission {
    pub const MANAGE_KEYRING: &'static str = "keyring.manage";
    pub const ATTACH_KEYRING: &'static str = "keyring.attach";
}

pub struct EnvironmentPermission;
impl EnvironmentPermission {
    pub const MANAGE_ENVIRONMENT_SECURITY: &'static str = "environment.security.manage";
    pub const MANAGE_ENVIRONMENT_WORKLOAD: &'static str = "environment.workload.manage";
    pub const USE_ENVIRONMENT: &'static str = "environment.use";
}

pub const TEAM_PERMISSIONS: &[&str] = &[
    "team.spend.read", "team.member_spend.read", "team.privacy_mode.read", "team.sso.read",
    "team.rules.read", "team.rules.manage", "team.hooks.read", "team.hooks.manage",
    "team.commit_metrics.read", "team.directory_groups.manage", "team.protected_git_scopes.manage",
    "team.background_composers.read", "team.background_composers.list", "team.background_composers.manage",
    "team.full_self_driving.manage", "team.public_profile_settings.manage", "team.member_spend_limits.manage",
    "team.mcp_servers.manage", "team.scim_configuration.manage", "team.billing_groups.manage",
    "team.groups.read", "team.groups.manage", "team.members.read", "team.members.manage",
    "team.membership.read", "team.invites.read", "team.invites.manage", "team.api_keys.manage",
    "team.repos.read", "team.repos.manage", "team.settings.read", "team.settings.manage",
    "team.billing.read", "team.billing.manage", "team.audit_logs.read", "team.analytics.read",
    "team.analytics.manage", "team.privacy.manage", "team.commands.read", "team.commands.manage",
    "team.bugbot.read", "team.bugbot.manage", "team.sharing_settings.read", "team.sharing_settings.manage",
    "team.background_agent_settings.manage", "team.plugins.read", "team.plugins.manage",
    "team.integrations.read", "team.integrations.manage",
];

pub const ORGANIZATION_PERMISSIONS: &[&str] = &[
    "organization.members.read", "organization.members.write", "organization.memberships.manage",
    "organization.manage", "organization.teams.manage", "organization.groups.read",
    "organization.groups.manage", "organization.api_keys.manage",
    "organization.identity_providers.manage", "organization.spend.read",
    "organization.audit_logs.read", "organization.billing.read",
];

pub const AGENT_STORE_PERMISSIONS: &[&str] = &[
    "agent_store.read", "agent_store.write", "agent_store.share",
];
pub const AGENT_STORE_SHARE_PERMISSIONS: &[&str] = &["agent_store_share.read"];
pub const KEYRING_PERMISSIONS: &[&str] = &["keyring.manage", "keyring.attach"];
pub const ENVIRONMENT_PERMISSIONS: &[&str] = &[
    "environment.security.manage", "environment.workload.manage", "environment.use",
];

pub fn is_known_permission(permission: &str) -> bool {
    [
        TEAM_PERMISSIONS,
        ORGANIZATION_PERMISSIONS,
        AGENT_STORE_PERMISSIONS,
        AGENT_STORE_SHARE_PERMISSIONS,
        KEYRING_PERMISSIONS,
        ENVIRONMENT_PERMISSIONS,
    ]
    .into_iter()
    .any(|group| group.contains(&permission))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_registry_has_no_duplicates_and_fails_closed() {
        let mut all = BTreeSet::new();
        for group in [
            TEAM_PERMISSIONS,
            ORGANIZATION_PERMISSIONS,
            AGENT_STORE_PERMISSIONS,
            AGENT_STORE_SHARE_PERMISSIONS,
            KEYRING_PERMISSIONS,
            ENVIRONMENT_PERMISSIONS,
        ] {
            for permission in group {
                assert!(all.insert(*permission), "duplicate permission: {permission}");
            }
        }
        assert_eq!(TeamPermission::READ_SPEND, "team.spend.read");
        assert_eq!(OrganizationPermission::READ_MEMBERS, "organization.members.read");
        assert_eq!(AgentStorePermission::READ_AGENT_STORE, "agent_store.read");
        assert_eq!(AgentStoreSharePermission::READ_AGENT_STORE_SHARE, "agent_store_share.read");
        assert_eq!(KeyringPermission::MANAGE_KEYRING, "keyring.manage");
        assert_eq!(EnvironmentPermission::USE_ENVIRONMENT, "environment.use");
        assert!(is_known_permission("agent_store.read"));
        assert!(is_known_permission("environment.use"));
        assert!(!is_known_permission("environment.root"));
    }
}
