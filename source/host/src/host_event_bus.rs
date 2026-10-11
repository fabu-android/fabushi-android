use std::sync::{mpsc::{self, Receiver, Sender}, Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEvent { pub family: String, pub session_id: String, pub payload_json: String }

#[derive(Clone, Default)]
pub struct HostEventBus { subscribers: Arc<Mutex<Vec<Sender<HostEvent>>>> }

impl HostEventBus {
    pub fn subscribe(&self) -> Receiver<HostEvent> {
        let (sender, receiver)=mpsc::channel();
        self.subscribers.lock().expect("host event subscribers lock poisoned").push(sender);
        receiver
    }
    pub fn emit(&self, event: HostEvent) {
        self.subscribers.lock().expect("host event subscribers lock poisoned").retain(|subscriber| subscriber.send(event.clone()).is_ok());
    }
    pub fn subscriber_count(&self) -> usize { self.subscribers.lock().expect("host event subscribers lock poisoned").len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fanout_is_not_competing_consumption() {
        let bus=HostEventBus::default();
        let a=bus.subscribe(); let b=bus.subscribe();
        let event=HostEvent{family:"transcript".into(),session_id:"s".into(),payload_json:"{}".into()};
        bus.emit(event.clone());
        assert_eq!(a.recv().unwrap(), event);
        assert_eq!(b.recv().unwrap(), event);
    }
}
