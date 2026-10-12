use std::sync::{mpsc::{self,Receiver,Sender},Arc,Mutex};
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct TranscriptMutation{pub agent_id:String,pub sequence:u64,pub kind:String}
#[derive(Clone,Default)]
pub struct TranscriptMutationBus{subscribers:Arc<Mutex<Vec<Sender<TranscriptMutation>>>>}
impl TranscriptMutationBus{
    pub fn subscribe(&self)->Receiver<TranscriptMutation>{let(tx,rx)=mpsc::channel();self.subscribers.lock().unwrap().push(tx);rx}
    pub fn publish(&self,event:TranscriptMutation){self.subscribers.lock().unwrap().retain(|tx|tx.send(event.clone()).is_ok());}
}
