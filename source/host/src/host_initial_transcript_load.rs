#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitialTranscriptLoadOptions { pub limit: usize, pub allow_empty_on_corruption: bool }

pub trait InitialTranscriptLoader {
    fn load(&mut self, agent_id:&str, limit:usize)->Result<Vec<String>,String>;
}

pub fn load_initial_transcript_resiliently<L:InitialTranscriptLoader>(loader:&mut L, agent_id:&str, options:&InitialTranscriptLoadOptions)->Result<Vec<String>,String>{
    if agent_id.trim().is_empty(){return Err("agent_id is required".into());}
    match loader.load(agent_id,options.limit.max(1)){
        Ok(rows)=>Ok(rows),
        Err(_) if options.allow_empty_on_corruption=>Ok(Vec::new()),
        Err(error)=>Err(error),
    }
}
