use crate::{
    interaction_listener::InteractionListener,
    interaction_updates::InteractionUpdate,
    redacted_interaction_updates::redact_update,
};

pub struct RedactedInteractionListener<L> {
    inner: L,
}

impl<L> RedactedInteractionListener<L> {
    pub fn new(inner: L) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> L {
        self.inner
    }
}

impl<L: InteractionListener> InteractionListener for RedactedInteractionListener<L> {
    fn on_update(&mut self, update: &InteractionUpdate) {
        self.inner.on_update(&redact_update(update));
    }
}
