use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardActionType {
    Callback,
    Url,
    MiniApp,
    ToggleTodo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardActionButton {
    pub id: String,
    pub label: String,
    pub action_type: CardActionType,
    pub payload: String,
}

impl CardActionButton {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        action_type: CardActionType,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            action_type,
            payload: payload.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub title: String,
    pub completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
}

impl TodoItem {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            completed: false,
            assignee: None,
            completed_at_ms: None,
        }
    }

    pub fn with_assignee(mut self, assignee: impl Into<String>) -> Self {
        self.assignee = Some(assignee.into());
        self
    }

    pub fn with_completed(mut self, completed: bool, completed_at_ms: Option<i64>) -> Self {
        self.completed = completed;
        self.completed_at_ms = completed_at_ms;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoCard {
    pub id: String,
    #[serde(default)]
    pub list_id: String,
    pub title: String,
    pub items: Vec<TodoItem>,
    pub completed_count: u32,
    #[serde(default)]
    pub action_buttons: Vec<CardActionButton>,
}

impl TodoCard {
    pub fn new(id: impl Into<String>, title: impl Into<String>, items: Vec<TodoItem>) -> Self {
        let id_str = id.into();
        let completed_count = items.iter().filter(|i| i.completed).count() as u32;
        Self {
            id: id_str.clone(),
            list_id: id_str,
            title: title.into(),
            items,
            completed_count,
            action_buttons: Vec::new(),
        }
    }

    pub fn recalculate_completed(&mut self) {
        self.completed_count = self.items.iter().filter(|i| i.completed).count() as u32;
    }

    pub fn completion_percentage(&self) -> f32 {
        if self.items.is_empty() {
            0.0
        } else {
            (self.completed_count as f32 / self.items.len() as f32) * 100.0
        }
    }

    pub fn toggle_item(&mut self, item_id: &str) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == item_id) {
            item.completed = !item.completed;
            if item.completed {
                item.completed_at_ms = Some(chrono::Utc::now().timestamp_millis());
            } else {
                item.completed_at_ms = None;
            }
            self.recalculate_completed();
            true
        } else {
            false
        }
    }

    pub fn add_item(&mut self, item: TodoItem) {
        if item.completed {
            self.completed_count += 1;
        }
        self.items.push(item);
    }

    pub fn remove_item(&mut self, item_id: &str) -> bool {
        if let Some(pos) = self.items.iter().position(|i| i.id == item_id) {
            let removed = self.items.remove(pos);
            if removed.completed {
                self.completed_count = self.completed_count.saturating_sub(1);
            }
            true
        } else {
            false
        }
    }

    pub fn add_action_button(&mut self, button: CardActionButton) {
        self.action_buttons.push(button);
    }
}
