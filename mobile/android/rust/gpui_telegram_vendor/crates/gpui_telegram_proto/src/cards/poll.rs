use crate::message::MessageEntity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PollOption {
    pub option_id: u8,
    pub text: String,
    pub vote_count: u32,
    #[serde(default)]
    pub is_chosen: bool,
    #[serde(default)]
    pub percentage: f32,
}

impl PollOption {
    pub fn new(option_id: u8, text: impl Into<String>) -> Self {
        Self {
            option_id,
            text: text.into(),
            vote_count: 0,
            is_chosen: false,
            percentage: 0.0,
        }
    }
}

/// Legacy alias struct for backwards compatibility
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollAnswer {
    pub text: String,
    pub option_id: u8,
    pub voters: u32,
    pub chosen: bool,
}

impl From<&PollOption> for PollAnswer {
    fn from(opt: &PollOption) -> Self {
        Self {
            text: opt.text.clone(),
            option_id: opt.option_id,
            voters: opt.vote_count,
            chosen: opt.is_chosen,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PollCard {
    pub id: String,
    #[serde(default)]
    pub poll_id: u64,
    pub question: String,
    pub options: Vec<PollOption>,
    #[serde(default)]
    pub answers: Vec<PollAnswer>,
    pub total_voters: u32,
    #[serde(default)]
    pub total_votes: u32,
    pub is_closed: bool,
    pub is_anonymous: bool,
    #[serde(default)]
    pub allows_multiple_answers: bool,
    #[serde(default)]
    pub is_quiz: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correct_option_id: Option<u8>,
    #[serde(default, alias = "explanation", skip_serializing_if = "Option::is_none")]
    pub solution: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub solution_entities: Vec<MessageEntity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_period: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_date: Option<i64>,
}

impl PollCard {
    pub fn new(
        id: impl Into<String>,
        question: impl Into<String>,
        options: Vec<PollOption>,
        is_anonymous: bool,
    ) -> Self {
        let id_str = id.into();
        let parsed_poll_id = id_str.parse::<u64>().unwrap_or(0);
        let total_voters: u32 = options.iter().map(|o| o.vote_count).sum();
        let answers = options.iter().map(PollAnswer::from).collect();

        let mut card = Self {
            id: id_str,
            poll_id: parsed_poll_id,
            question: question.into(),
            options,
            answers,
            total_voters,
            total_votes: total_voters,
            is_closed: false,
            is_anonymous,
            allows_multiple_answers: false,
            is_quiz: false,
            correct_option_id: None,
            solution: None,
            solution_entities: Vec::new(),
            close_period: None,
            close_date: None,
        };
        card.recalculate_percentages();
        card
    }

    pub fn recalculate_percentages(&mut self) {
        let total: u32 = self.options.iter().map(|o| o.vote_count).sum();
        self.total_voters = total;
        self.total_votes = total;
        if total > 0 {
            for opt in &mut self.options {
                opt.percentage = (opt.vote_count as f32 / total as f32) * 100.0;
            }
        } else {
            for opt in &mut self.options {
                opt.percentage = 0.0;
            }
        }
        self.answers = self.options.iter().map(PollAnswer::from).collect();
    }

    pub fn vote_percentage(&self, option_id: u8) -> f32 {
        self.options
            .iter()
            .find(|o| o.option_id == option_id)
            .map(|o| o.percentage)
            .unwrap_or(0.0)
    }

    pub fn cast_vote(&mut self, option_id: u8) -> bool {
        if self.is_closed {
            return false;
        }

        if !self.allows_multiple_answers {
            // Unset other chosen options
            for opt in &mut self.options {
                if opt.is_chosen && opt.option_id != option_id {
                    opt.is_chosen = false;
                    opt.vote_count = opt.vote_count.saturating_sub(1);
                }
            }
        }

        if let Some(opt) = self.options.iter_mut().find(|o| o.option_id == option_id) {
            if opt.is_chosen {
                opt.is_chosen = false;
                opt.vote_count = opt.vote_count.saturating_sub(1);
            } else {
                opt.is_chosen = true;
                opt.vote_count += 1;
            }
            self.recalculate_percentages();
            true
        } else {
            false
        }
    }

    pub fn close(&mut self) {
        self.is_closed = true;
    }
}
