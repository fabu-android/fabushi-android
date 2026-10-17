pub const ASK_QUESTION_AUTO_ANSWER_MARKER: &str = "ask_question_auto_answer";
pub const ASK_QUESTION_AUTO_ANSWER_REASON_PREFIX: &str =
    "No response was received within the time limit";
pub const ASK_QUESTION_AUTO_ANSWER_REASON_BODY: &str =
    "No response was received within the time limit. Proceed with the recommended option(s) you offered for each question, or your best judgment based on the information already available.";
pub const ASK_QUESTION_AUTO_ANSWER_REASON: &str =
    "ask_question_auto_answer:timeout|No response was received within the time limit. Proceed with the recommended option(s) you offered for each question, or your best judgment based on the information already available.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskQuestionAutoAnswerKind {
    Timeout,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AskQuestionAutoAnswerIdentity {
    pub kind: AskQuestionAutoAnswerKind,
}

impl AskQuestionAutoAnswerIdentity {
    fn kind_name(self) -> &'static str {
        match self.kind {
            AskQuestionAutoAnswerKind::Timeout => "timeout",
            AskQuestionAutoAnswerKind::Other => "other",
        }
    }
}

pub fn create_ask_question_auto_answer_identity(
    kind: AskQuestionAutoAnswerKind,
) -> AskQuestionAutoAnswerIdentity {
    AskQuestionAutoAnswerIdentity { kind }
}

pub fn format_ask_question_auto_answer_reason(
    identity: AskQuestionAutoAnswerIdentity,
) -> String {
    format!(
        "{}:{}|{}",
        ASK_QUESTION_AUTO_ANSWER_MARKER,
        identity.kind_name(),
        ASK_QUESTION_AUTO_ANSWER_REASON_BODY
    )
}

pub fn parse_ask_question_auto_answer_identity(
    reason: Option<&str>,
) -> Option<AskQuestionAutoAnswerIdentity> {
    let value = reason.unwrap_or_default().trim();
    let suffix = value.strip_prefix(ASK_QUESTION_AUTO_ANSWER_MARKER)?.strip_prefix(':')?;
    let kind = suffix.split_once('|')?.0;
    let kind = match kind {
        "timeout" => AskQuestionAutoAnswerKind::Timeout,
        "other" => AskQuestionAutoAnswerKind::Other,
        _ => return None,
    };
    Some(AskQuestionAutoAnswerIdentity { kind })
}

pub fn is_ask_question_auto_answer_reason(reason: Option<&str>) -> bool {
    parse_ask_question_auto_answer_identity(reason).is_some()
        || reason
            .unwrap_or_default()
            .trim()
            .starts_with(ASK_QUESTION_AUTO_ANSWER_REASON_PREFIX)
}

pub fn default_ask_question_auto_answer_reason() -> String {
    ASK_QUESTION_AUTO_ANSWER_REASON.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_identity_round_trips_and_legacy_prefix_remains_recognized() {
        let reason = default_ask_question_auto_answer_reason();
        assert_eq!(reason, ASK_QUESTION_AUTO_ANSWER_REASON);
        assert_eq!(
            parse_ask_question_auto_answer_identity(Some(&reason)),
            Some(AskQuestionAutoAnswerIdentity { kind: AskQuestionAutoAnswerKind::Timeout })
        );
        assert!(is_ask_question_auto_answer_reason(Some(
            "No response was received within the time limit."
        )));
        assert!(!is_ask_question_auto_answer_reason(Some("user cancelled")));
    }
}
