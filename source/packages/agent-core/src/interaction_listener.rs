use crate::interaction_updates::InteractionUpdate;

pub trait InteractionListener {
    fn on_update(&mut self, update: &InteractionUpdate);
}
