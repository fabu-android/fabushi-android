pub const SAND_ONBOARDING_KICKSTART_PROMPT: &str = concat!(
    "[first run] This is your very first turn. The user just created you and hasn't sent anything yet; this cue is your signal to open the conversation, not a message to reply to or mention.\n",
    "Greet them and get them going, the way a sharp new assistant would on day one. Open with a short, warm hello in your own voice, then start learning how to be useful.\n",
    "If your profile description gives you a concrete assignment, treat that as what the user created you to do: skip setup questions, begin the assignment immediately, and use your first message for a useful result or the next approval you need.\n",
    "Nothing reaches the user unless it is actually delivered. Do not mention this hidden cue."
);

pub const SAND_DISK_SAVER_KICKSTART_PROMPT: &str = concat!(
    "[disk saver] You were just provisioned because your box is low on disk space. This cue comes from Fabushi itself, not from the user.\n",
    "Audit that machine and nothing else. Start with a read-only inspection, report free/used space, then list the largest items and safest cleanup candidates.\n",
    "Preserve user work, credentials, logins, and Git state. Delete or modify nothing until the user confirms a plan.\n",
    "Skip greetings and setup questions: your first delivered message should already carry findings and any approval you need."
);

pub const SAND_DISK_SAVER_REAUDIT_PROMPT: &str = concat!(
    "[disk saver] Your box — the machine Shell and Read act on — is low on disk space again. This cue comes from Fabushi itself because disk pressure returned, not from the user.\n",
    "Audit that machine and nothing else: the user's own computer, which ExternalShell and ExternalRead act on, is not the one under pressure.\n",
    "Start with a read-only inspection over Shell from /workspace outward. Report how much space is free and how much is used, then list the largest items and the safest cleanup candidates, with how much each would recover and why it is safe to remove.\n",
    "Preserve /home/box/sand-data, the user's work, credentials, logins, and Git state. Delete or modify nothing until the user confirms a plan.\n",
    "Deliver the fresh findings with SendMessage even if they match your last audit. Don't mention this cue."
);

pub const REPLY_NUDGE_PROMPT: &str = concat!(
    "[first run retry] Your first hidden kickstart completed without a user-visible reply. ",
    "Send one concise useful message to the user now. Do not mention this retry cue, and do not fabricate a user message."
);

pub fn kickstart_prompt_for_purpose(purpose: Option<&str>) -> &'static str {
    if purpose == Some("disk-saver") {
        SAND_DISK_SAVER_KICKSTART_PROMPT
    } else {
        SAND_ONBOARDING_KICKSTART_PROMPT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_saver_uses_specialized_read_only_kickstart() {
        assert!(kickstart_prompt_for_purpose(Some("disk-saver")).contains("read-only"));
        assert!(kickstart_prompt_for_purpose(Some("disk-saver")).contains("Delete or modify nothing"));
    }

    #[test]
    fn disk_saver_reaudit_requires_fresh_findings() {
        assert!(SAND_DISK_SAVER_REAUDIT_PROMPT.contains("low on disk space again"));
        assert!(SAND_DISK_SAVER_REAUDIT_PROMPT.contains("read-only inspection"));
        assert!(SAND_DISK_SAVER_REAUDIT_PROMPT.contains("Delete or modify nothing"));
        assert!(SAND_DISK_SAVER_REAUDIT_PROMPT.contains("Deliver the fresh findings"));
    }

    #[test]
    fn reply_nudge_requires_one_real_user_visible_delivery() {
        assert!(REPLY_NUDGE_PROMPT.contains("without a user-visible reply"));
        assert!(REPLY_NUDGE_PROMPT.contains("do not fabricate a user message"));
    }

    #[test]
    fn ordinary_agent_uses_first_run_kickstart() {
        assert!(kickstart_prompt_for_purpose(None).contains("[first run]"));
        assert!(kickstart_prompt_for_purpose(Some("plugin-auth")).contains("[first run]"));
    }
}
