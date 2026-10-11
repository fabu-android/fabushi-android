#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoSpilloverUiDefaults {
    pub auto_title: &'static str,
    pub auto_description: &'static str,
    pub api_title: &'static str,
    pub api_description: &'static str,
    pub auto_beyond_limit_description: &'static str,
    pub auto_usage_bar_label: &'static str,
    pub api_usage_bar_label: &'static str,
}

pub const AUTO_SPILLOVER_UI_DEFAULTS: AutoSpilloverUiDefaults = AutoSpilloverUiDefaults {
    auto_title: "Cursor Models",
    auto_description: "Includes Cursor Grok 4.5 and Composer 2.5",
    api_title: "Other Models",
    api_description: "Consumed by named models.",
    auto_beyond_limit_description:
        "Additional usage beyond limits consumes Other Models quota or on-demand spend.",
    auto_usage_bar_label: "your included total usage",
    api_usage_bar_label: "your included API usage",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_nonempty_and_distinct() {
        assert!(!AUTO_SPILLOVER_UI_DEFAULTS.auto_title.is_empty());
        assert_ne!(
            AUTO_SPILLOVER_UI_DEFAULTS.auto_usage_bar_label,
            AUTO_SPILLOVER_UI_DEFAULTS.api_usage_bar_label
        );
    }
}
