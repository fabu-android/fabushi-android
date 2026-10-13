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
    fn ordinary_agent_uses_first_run_kickstart() {
        assert!(kickstart_prompt_for_purpose(None).contains("[first run]"));
        assert!(kickstart_prompt_for_purpose(Some("plugin-auth")).contains("[first run]"));
    }
}
