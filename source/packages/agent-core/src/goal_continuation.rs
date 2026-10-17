#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalContinuation {
    Continue,
    Settle,
}

pub fn decide_goal_continuation(is_terminal: bool, remaining_steps: usize, cancelled: bool) -> GoalContinuation {
    if cancelled || is_terminal || remaining_steps == 0 {
        GoalContinuation::Settle
    } else {
        GoalContinuation::Continue
    }
}
