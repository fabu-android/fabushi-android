use futures::future::BoxFuture;

use crate::groups::group_chat::{
    GROUP_MAX_MEMBER_TURNS, GROUP_MAX_MESSAGES_PER_TURN, GROUP_MAX_ROUNDS,
    SHARED_ROOM_HISTORY_LIMIT, GroupDescription, GroupMember, GroupMessage,
    build_group_member_system_prompt, build_group_turn_prompt, is_pass_content,
    messages_since_member_last_spoke, order_round_speakers, resolve_responders,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMemberTurnRequest {
    pub member: GroupMember,
    pub system_prompt: String,
    pub prompt: String,
    pub group: GroupDescription,
    pub peers: Vec<GroupMember>,
    pub new_messages: Vec<GroupMessage>,
    pub shared_room_id: Option<String>,
}

pub trait GroupOrchestratorDeps: Send + Sync {
    fn resolve_members<'a>(&'a self, ids: &'a [String]) -> BoxFuture<'a, Vec<GroupMember>>;
    fn read_history(&self) -> Vec<GroupMessage>;
    fn is_current(&self) -> bool;
    fn run_member_turn<'a>(
        &'a self,
        request: GroupMemberTurnRequest,
    ) -> BoxFuture<'a, Vec<String>>;
    fn post_member_message(&self, member: &GroupMember, content: &str);
    fn finalize_member_turn(&self, _member: &GroupMember) {}
    fn is_shared_room(&self) -> bool {
        false
    }
    fn shared_room_id(&self) -> Option<String> {
        None
    }
}

pub struct GroupChatOrchestrator<D> {
    deps: D,
}

impl<D> GroupChatOrchestrator<D>
where
    D: GroupOrchestratorDeps,
{
    pub fn new(deps: D) -> Self {
        Self { deps }
    }

    pub async fn run(&self, group: &GroupDescription, member_ids: &[String]) {
        let members = self.deps.resolve_members(member_ids).await;
        if members.is_empty() {
            return;
        }

        let member_by_id = members
            .iter()
            .map(|member| (member.id.clone(), member.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let mut total_messages = 0usize;

        for round in 0..GROUP_MAX_ROUNDS {
            if !self.deps.is_current() {
                return;
            }
            let responder_ids = resolve_responders(&members, &self.deps.read_history())
                .into_iter()
                .map(|member| member.id)
                .collect::<Vec<_>>();
            let mut messages_this_round = 0usize;

            for member_id in order_round_speakers(&responder_ids, round as i64) {
                if total_messages >= GROUP_MAX_MEMBER_TURNS || !self.deps.is_current() {
                    return;
                }
                let Some(member) = member_by_id.get(&member_id).cloned() else {
                    continue;
                };

                let sent = self.run_one_turn(group, &member, &members).await;
                let mut hit_cap = false;
                for content in sent {
                    self.deps.post_member_message(&member, &content);
                    total_messages += 1;
                    messages_this_round += 1;
                    if total_messages >= GROUP_MAX_MEMBER_TURNS {
                        hit_cap = true;
                        break;
                    }
                }
                self.deps.finalize_member_turn(&member);
                if hit_cap {
                    return;
                }
            }

            if messages_this_round == 0 {
                return;
            }
        }
    }

    pub async fn run_one_turn(
        &self,
        group: &GroupDescription,
        member: &GroupMember,
        members: &[GroupMember],
    ) -> Vec<String> {
        let peers = members
            .iter()
            .filter(|other| other.id != member.id)
            .cloned()
            .collect::<Vec<_>>();
        let history = self.deps.read_history();
        let new_messages = if self.deps.is_shared_room() {
            let start = history.len().saturating_sub(SHARED_ROOM_HISTORY_LIMIT);
            &history[start..]
        } else {
            messages_since_member_last_spoke(&history, &member.id)
        };
        let sent = self
            .deps
            .run_member_turn(GroupMemberTurnRequest {
                member: member.clone(),
                system_prompt: build_group_member_system_prompt(
                    member,
                    group,
                    &peers,
                    self.deps.is_shared_room(),
                ),
                prompt: build_group_turn_prompt(member, group, &peers, new_messages),
                group: group.clone(),
                peers: peers.clone(),
                new_messages: new_messages.to_vec(),
                shared_room_id: self.deps.shared_room_id(),
            })
            .await;

        let mut spoken = Vec::new();
        for content in sent {
            if is_pass_content(&content) {
                continue;
            }
            let trimmed = content.trim();
            if trimmed.is_empty() {
                continue;
            }
            spoken.push(trimmed.to_string());
            if spoken.len() >= GROUP_MAX_MESSAGES_PER_TURN {
                break;
            }
        }
        spoken
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;
    use std::sync::Mutex;

    struct TestDeps {
        members: Vec<GroupMember>,
        history: Mutex<Vec<GroupMessage>>,
        outputs: Mutex<Vec<Vec<String>>>,
    }

    impl GroupOrchestratorDeps for TestDeps {
        fn resolve_members<'a>(&'a self, ids: &'a [String]) -> BoxFuture<'a, Vec<GroupMember>> {
            let members = self
                .members
                .iter()
                .filter(|member| ids.iter().any(|id| id == &member.id))
                .cloned()
                .collect::<Vec<_>>();
            Box::pin(async move { members })
        }

        fn read_history(&self) -> Vec<GroupMessage> {
            self.history.lock().unwrap().clone()
        }

        fn is_current(&self) -> bool {
            true
        }

        fn run_member_turn<'a>(
            &'a self,
            _request: GroupMemberTurnRequest,
        ) -> BoxFuture<'a, Vec<String>> {
            let output = self.outputs.lock().unwrap().remove(0);
            Box::pin(async move { output })
        }

        fn post_member_message(&self, member: &GroupMember, content: &str) {
            self.history.lock().unwrap().push(GroupMessage {
                speaker: crate::groups::group_chat::GroupSpeaker::Member {
                    id: member.id.clone(),
                    name: member.name.clone(),
                },
                content: content.to_string(),
            });
        }
    }

    fn member(id: &str, name: &str) -> GroupMember {
        GroupMember {
            id: id.into(),
            name: name.into(),
            description: String::new(),
        }
    }

    #[test]
    fn orchestrator_filters_pass_and_caps_member_messages() {
        let deps = TestDeps {
            members: vec![member("a", "A")],
            history: Mutex::new(vec![GroupMessage {
                speaker: crate::groups::group_chat::GroupSpeaker::User { name: None },
                content: "hello".into(),
            }]),
            outputs: Mutex::new(vec![vec![
                "(pass)".into(),
                " first ".into(),
                "second".into(),
                "third".into(),
            ]]),
        };
        let orchestrator = GroupChatOrchestrator::new(deps);
        let group = GroupDescription {
            name: "Room".into(),
            description: String::new(),
        };
        let spoken = futures::executor::block_on(orchestrator.run_one_turn(
            &group,
            &member("a", "A"),
            &[member("a", "A")],
        ));
        assert_eq!(spoken, vec!["first".to_string(), "second".to_string()]);
    }

    #[test]
    fn orchestrator_posts_member_output_into_shared_history() {
        let deps = TestDeps {
            members: vec![member("a", "A")],
            history: Mutex::new(vec![GroupMessage {
                speaker: crate::groups::group_chat::GroupSpeaker::User { name: None },
                content: "@A status?".into(),
            }]),
            outputs: Mutex::new(vec![vec!["done".into()], vec!["(pass)".into()]]),
        };
        let orchestrator = GroupChatOrchestrator::new(deps);
        let group = GroupDescription {
            name: "Room".into(),
            description: "coordination".into(),
        };
        futures::executor::block_on(orchestrator.run(&group, &["a".to_string()]));
        let history = orchestrator.deps.read_history();
        assert!(history.iter().any(|message| {
            matches!(
                &message.speaker,
                crate::groups::group_chat::GroupSpeaker::Member { id, .. } if id == "a"
            ) && message.content == "done"
        }));
    }
}
