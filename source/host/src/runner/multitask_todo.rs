use super::{AndroidRoutedToolBridge};
use crate::sha256::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const TODO_WRITE_TOOL_NAME: &str = "TodoWrite";

const TODO_DESCRIPTION: &str = "Your task queue: the durable list of everything the user has asked for, across parallel streams of work. Keep pending, in_progress, completed, and cancelled states current. This root-turn bookkeeping tool is unavailable to generated subagents.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultitaskTodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl MultitaskTodoStatus {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultitaskTodoItem {
    pub id: String,
    pub content: String,
    pub status: MultitaskTodoStatus,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DurableTodoCall {
    args: Value,
    result: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DurableTodoScope {
    todos: Vec<MultitaskTodoItem>,
    #[serde(default)]
    calls: BTreeMap<String, DurableTodoCall>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DurableTodoDocument {
    #[serde(default)]
    scopes: BTreeMap<String, DurableTodoScope>,
}

pub struct DurableMultitaskTodoStore {
    path: PathBuf,
    state: DurableTodoDocument,
}

impl DurableMultitaskTodoStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let state = if path.exists() {
            let raw = fs::read(&path)
                .map_err(|error| format!("failed to read durable multitask todos: {error}"))?;
            serde_json::from_slice(&raw)
                .map_err(|error| format!("failed to decode durable multitask todos: {error}"))?
        } else {
            DurableTodoDocument::default()
        };
        Ok(Self { path, state })
    }

    pub fn snapshot(
        &self,
        account_fence: &str,
        conversation_id: &str,
    ) -> Vec<MultitaskTodoItem> {
        self.state
            .scopes
            .get(&scope_id(account_fence, conversation_id))
            .map(|scope| scope.todos.clone())
            .unwrap_or_default()
    }

    fn apply(
        &mut self,
        account_fence: &str,
        conversation_id: &str,
        args: &Value,
        tool_call_id: &str,
    ) -> Result<Value, String> {
        let tool_call_id = required(tool_call_id, "TodoWrite tool_call_id")?;
        let scope_id = scope_id(
            required(account_fence, "TodoWrite account fence")?,
            required(conversation_id, "TodoWrite conversation identity")?,
        );

        if let Some(call) = self
            .state
            .scopes
            .get(&scope_id)
            .and_then(|scope| scope.calls.get(tool_call_id))
        {
            if call.args == *args {
                return Ok(call.result.clone());
            }
            return Err("TodoWrite tool_call_id was reused with mismatched arguments".into());
        }

        let object = args
            .as_object()
            .ok_or("TodoWrite arguments must be an object")?;
        let merge = parse_merge(object.get("merge"))?;
        let values = object
            .get("todos")
            .and_then(Value::as_array)
            .ok_or("TodoWrite requires a todos array")?;
        let patches = values
            .iter()
            .map(parse_patch)
            .collect::<Result<Vec<_>, _>>()?;
        let now = now_ms();

        let scope = self.state.scopes.entry(scope_id).or_default();
        let mut todos = scope.todos.clone();
        if merge {
            for patch in patches {
                if let Some(existing) = todos.iter_mut().find(|todo| todo.id == patch.id) {
                    if let Some(content) = patch.content {
                        existing.content = content;
                    }
                    if existing.content.is_empty() {
                        return Err("Invalid argument: must provide 'content' for new todos items".into());
                    }
                    existing.status = patch.status;
                    existing.updated_at_ms = now;
                } else {
                    let content = patch.content.unwrap_or_default();
                    if content.is_empty() {
                        return Err("Invalid argument: must provide 'content' for new todos items".into());
                    }
                    todos.push(MultitaskTodoItem {
                        id: patch.id,
                        content,
                        status: patch.status,
                        created_at_ms: now,
                        updated_at_ms: now,
                    });
                }
            }
        } else {
            todos = patches
                .into_iter()
                .map(|patch| {
                    let content = patch.content.unwrap_or_default();
                    if content.is_empty() {
                        return Err("Invalid argument: must provide 'content' for new todos items".to_string());
                    }
                    Ok(MultitaskTodoItem {
                        id: patch.id,
                        content,
                        status: patch.status,
                        created_at_ms: now,
                        updated_at_ms: now,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
        }

        let result = Value::String(render_success(&todos));
        scope.todos = todos;
        scope.calls.insert(
            tool_call_id.to_string(),
            DurableTodoCall {
                args: args.clone(),
                result: result.clone(),
            },
        );
        self.persist()?;
        Ok(result)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create durable multitask todo directory: {error}"))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(&self.state)
                .map_err(|error| format!("failed to encode durable multitask todos: {error}"))?,
        )
        .map_err(|error| format!("failed to write durable multitask todos: {error}"))?;
        fs::rename(&tmp, &self.path)
            .map_err(|error| format!("failed to commit durable multitask todos: {error}"))
    }
}

#[derive(Debug)]
struct TodoPatch {
    id: String,
    content: Option<String>,
    status: MultitaskTodoStatus,
}

fn parse_patch(value: &Value) -> Result<TodoPatch, String> {
    let object = value
        .as_object()
        .ok_or("TodoWrite todo entries must be objects")?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or("TodoWrite todo.id must be a string")?
        .to_string();
    let content = match object.get("content") {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) | None => None,
        Some(_) => return Err("TodoWrite todo.content must be a string".into()),
    };
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .and_then(MultitaskTodoStatus::parse)
        .ok_or("TodoWrite todo.status must be pending, in_progress, completed, or cancelled")?;
    Ok(TodoPatch { id, content, status })
}

fn parse_merge(value: Option<&Value>) -> Result<bool, String> {
    match value {
        None => Ok(true),
        Some(Value::Bool(value)) => Ok(*value),
        Some(Value::String(value)) if value.eq_ignore_ascii_case("true") || value == "1" => Ok(true),
        Some(Value::String(value)) if value.eq_ignore_ascii_case("false") || value == "0" => Ok(false),
        Some(Value::Number(value)) if value.as_i64() == Some(1) => Ok(true),
        Some(Value::Number(value)) if value.as_i64() == Some(0) => Ok(false),
        _ => Err("TodoWrite merge must be a boolean".into()),
    }
}

fn tool_definition() -> Value {
    json!({
        "type":"function",
        "name":TODO_WRITE_TOOL_NAME,
        "description":TODO_DESCRIPTION,
        "parameters":{
            "type":"object",
            "required":["todos","merge"],
            "additionalProperties":false,
            "properties":{
                "todos":{
                    "type":"array",
                    "minItems":2,
                    "description":"Array of TODO items to update or create",
                    "items":{
                        "type":"object",
                        "required":["id","content","status"],
                        "additionalProperties":false,
                        "properties":{
                            "id":{"type":"string"},
                            "content":{"type":"string"},
                            "status":{"type":"string","enum":["pending","in_progress","completed","cancelled"]}
                        }
                    }
                },
                "merge":{"type":"boolean"}
            }
        }
    })
}

fn render_success(todos: &[MultitaskTodoItem]) -> String {
    let mut message = String::from(
        "Successfully updated TODOs. Make sure to follow and update your TODO list as you make progress. Cancel and add new TODO tasks as needed when the user makes a correction or follow-up request.",
    );
    if todos.iter().any(|todo| todo.status == MultitaskTodoStatus::Pending)
        && todos.iter().all(|todo| todo.status != MultitaskTodoStatus::InProgress)
    {
        message.push_str(" No TODOs are marked in-progress, make sure to mark them before starting the next.");
    }
    if todos.iter().filter(|todo| todo.status.is_finished()).count() > 20 {
        message.push_str("\n\n<system_reminder>You have many finished todos. Consider cleaning up old ones.</system_reminder>");
    }
    message.push_str("\n\nHere are the latest contents of your todo list:");
    for todo in todos {
        message.push_str(&format!(
            "\n- **{}**: {} (id: {})",
            todo.status.as_str().to_ascii_uppercase(),
            todo.content,
            todo.id,
        ));
    }
    message
}

fn scope_id(account_fence: &str, conversation_id: &str) -> String {
    sha256_hex(format!("{account_fence}\n{conversation_id}").as_bytes())
}

fn required<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(value)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub struct MultitaskTodoRoutedTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    store: Arc<Mutex<DurableMultitaskTodoStore>>,
    account_fence: String,
    conversation_id: String,
}

impl MultitaskTodoRoutedTools {
    fn new(
        delegate: Arc<dyn AndroidRoutedToolBridge>,
        store: Arc<Mutex<DurableMultitaskTodoStore>>,
        account_fence: &str,
        conversation_id: &str,
    ) -> Self {
        Self {
            delegate,
            store,
            account_fence: account_fence.to_string(),
            conversation_id: conversation_id.to_string(),
        }
    }
}

impl AndroidRoutedToolBridge for MultitaskTodoRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        tools.retain(|tool| tool.get("name").and_then(Value::as_str) != Some(TODO_WRITE_TOOL_NAME));
        tools.insert(0, tool_definition());
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if name != TODO_WRITE_TOOL_NAME {
            return self.delegate.call_tool(name, args, tool_call_id);
        }
        self.store
            .lock()
            .map_err(|_| "durable multitask todo lock poisoned".to_string())?
            .apply(
                &self.account_fence,
                &self.conversation_id,
                &args,
                tool_call_id,
            )
    }
}

pub fn with_multitask_todo_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    store: Arc<Mutex<DurableMultitaskTodoStore>>,
    account_fence: &str,
    conversation_id: &str,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(MultitaskTodoRoutedTools::new(
        delegate,
        store,
        account_fence,
        conversation_id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyTools;

    impl AndroidRoutedToolBridge for EmptyTools {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(Vec::new())
        }

        fn call_tool(&self, name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            Err(format!("unexpected delegated tool: {name}"))
        }
    }

    fn args(content: &str) -> Value {
        json!({
            "todos":[
                {"id":"a","content":content,"status":"in_progress"},
                {"id":"b","content":"second","status":"pending"}
            ],
            "merge":false
        })
    }

    #[test]
    fn root_tool_persists_merge_replace_and_exact_duplicate_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todos.json");
        let store = Arc::new(Mutex::new(DurableMultitaskTodoStore::open(&path).unwrap()));
        let tools = with_multitask_todo_tools(
            Arc::new(EmptyTools),
            Arc::clone(&store),
            "acct:a",
            "agent:a",
        );
        assert_eq!(tools.list_tools().unwrap()[0]["name"], TODO_WRITE_TOOL_NAME);
        let first = tools.call_tool(TODO_WRITE_TOOL_NAME, args("first"), "call-1").unwrap();
        let duplicate = tools.call_tool(TODO_WRITE_TOOL_NAME, args("first"), "call-1").unwrap();
        assert_eq!(first, duplicate);
        assert!(tools.call_tool(TODO_WRITE_TOOL_NAME, args("changed"), "call-1").is_err());

        tools.call_tool(
            TODO_WRITE_TOOL_NAME,
            json!({"todos":[{"id":"a","content":"first","status":"completed"}],"merge":true}),
            "call-2",
        ).unwrap();
        let snapshot = store.lock().unwrap().snapshot("acct:a", "agent:a");
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].status, MultitaskTodoStatus::Completed);
    }

    #[test]
    fn process_reopen_preserves_todos_and_duplicate_result() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todos.json");
        let original_result = {
            let store = Arc::new(Mutex::new(DurableMultitaskTodoStore::open(&path).unwrap()));
            let tools = with_multitask_todo_tools(
                Arc::new(EmptyTools),
                store,
                "acct:a",
                "agent:a",
            );
            tools.call_tool(TODO_WRITE_TOOL_NAME, args("persisted"), "stable-call").unwrap()
        };
        let reopened = Arc::new(Mutex::new(DurableMultitaskTodoStore::open(&path).unwrap()));
        assert_eq!(reopened.lock().unwrap().snapshot("acct:a", "agent:a").len(), 2);
        let tools = with_multitask_todo_tools(
            Arc::new(EmptyTools),
            reopened,
            "acct:a",
            "agent:a",
        );
        assert_eq!(
            tools.call_tool(TODO_WRITE_TOOL_NAME, args("persisted"), "stable-call").unwrap(),
            original_result
        );
    }

    #[test]
    fn account_and_conversation_scopes_do_not_bleed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todos.json");
        let store = Arc::new(Mutex::new(DurableMultitaskTodoStore::open(&path).unwrap()));
        let tools = with_multitask_todo_tools(
            Arc::new(EmptyTools),
            Arc::clone(&store),
            "acct:a",
            "agent:a",
        );
        tools.call_tool(TODO_WRITE_TOOL_NAME, args("private"), "call-a").unwrap();
        let state = store.lock().unwrap();
        assert_eq!(state.snapshot("acct:a", "agent:a").len(), 2);
        assert!(state.snapshot("acct:b", "agent:a").is_empty());
        assert!(state.snapshot("acct:a", "agent:b").is_empty());
    }

    #[test]
    fn validation_fails_closed_before_persisting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todos.json");
        let store = Arc::new(Mutex::new(DurableMultitaskTodoStore::open(&path).unwrap()));
        let tools = with_multitask_todo_tools(
            Arc::new(EmptyTools),
            Arc::clone(&store),
            "acct:a",
            "agent:a",
        );
        assert!(tools.call_tool(
            TODO_WRITE_TOOL_NAME,
            json!({"todos":[{"id":"a","content":"x","status":"unknown"}],"merge":false}),
            "invalid",
        ).is_err());
        assert!(store.lock().unwrap().snapshot("acct:a", "agent:a").is_empty());
    }
}
