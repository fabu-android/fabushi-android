use super::action::{ActionCategory, ActionContext, ActionError, PlatformAction, TypedPlatformAction};
use futures::future::BoxFuture;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct ActionRegistry {
    actions: Arc<RwLock<HashMap<String, Arc<dyn PlatformAction>>>>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self {
            actions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register(&self, action: Arc<dyn PlatformAction>) -> Result<(), ActionError> {
        let mut map = self
            .actions
            .write()
            .map_err(|_| ActionError::ExecutionFailed("Registry lock poisoned".into()))?;
        map.insert(action.name().to_string(), action);
        Ok(())
    }

    pub fn register_typed<T: TypedPlatformAction>(&self, action: T) -> Result<(), ActionError> {
        self.register(Arc::new(action))
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn PlatformAction>> {
        let map = self.actions.read().ok()?;
        map.get(name).cloned()
    }

    pub fn lookup(&self, name: &str) -> Option<Arc<dyn PlatformAction>> {
        self.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn count(&self) -> usize {
        self.actions.read().map(|m| m.len()).unwrap_or(0)
    }

    pub fn list(&self) -> Vec<Arc<dyn PlatformAction>> {
        let map = match self.actions.read() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        let mut list: Vec<_> = map.values().cloned().collect();
        list.sort_by(|a, b| a.name().cmp(b.name()));
        list
    }

    pub fn list_by_category(&self, category: ActionCategory) -> Vec<Arc<dyn PlatformAction>> {
        self.list()
            .into_iter()
            .filter(|a| a.category() == category)
            .collect()
    }

    pub fn execute<'a>(
        &'a self,
        action_name: &str,
        ctx: ActionContext,
        input: Value,
    ) -> BoxFuture<'a, Result<Value, ActionError>> {
        let action = match self.get(action_name) {
            Some(a) => a,
            None => {
                let name = action_name.to_string();
                return Box::pin(async move {
                    Err(ActionError::NotFound(format!(
                        "Action '{name}' not registered"
                    )))
                });
            }
        };
        if action.risk_level().requires_explicit_confirmation() && !ctx.approved {
            let name = action.name().to_string();
            return Box::pin(async move {
                Err(ActionError::Forbidden(format!(
                    "Action '{name}' requires explicit approval confirmation"
                )))
            });
        }
        Box::pin(async move { action.execute(ctx, input).await })
    }
}
