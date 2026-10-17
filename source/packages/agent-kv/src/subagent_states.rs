use std::{collections::BTreeMap, thread};
use crate::reference::BlobRef;

pub const SUBAGENT_STATE_BLOB_FETCH_CONCURRENCY: usize = 8;

#[derive(Debug)]
pub struct SubagentResolution<T> {
    pub states: BTreeMap<String, T>,
    pub inline_fallback_ids: Vec<String>,
}

enum Outcome<T> {
    Loaded(T),
    Missing,
    Failed(String),
}

pub fn resolve_subagent_state_references<T, L, D>(
    mut inline: BTreeMap<String, T>,
    refs: BTreeMap<String, BlobRef>,
    load: L,
    decode: D,
) -> Result<SubagentResolution<T>, String>
where
    T: Send,
    L: Fn(&BlobRef) -> Result<Option<Vec<u8>>, String> + Sync,
    D: Fn(&[u8]) -> Result<T, String> + Sync,
{
    let entries = refs.iter().map(|(id, reference)| (id.clone(), reference.clone())).collect::<Vec<_>>();
    let mut missing = Vec::new();
    let mut failed = Vec::new();
    let mut fallback = Vec::new();

    for chunk in entries.chunks(SUBAGENT_STATE_BLOB_FETCH_CONCURRENCY) {
        let results = thread::scope(|scope| {
            let handles = chunk.iter().map(|(id, reference)| {
                let id = id.clone();
                let panic_id = id.clone();
                let reference = reference.clone();
                let load_ref = &load;
                let decode_ref = &decode;
                (id, scope.spawn(move || {
                    match load_ref(&reference) {
                        Ok(Some(bytes)) => decode_ref(&bytes)
                            .map(Outcome::Loaded)
                            .unwrap_or_else(|error| Outcome::Failed(format!("invalid referenced subagent state for {panic_id}: {error}"))),
                        Ok(None) => Outcome::Missing,
                        Err(error) => Outcome::Failed(format!("failed to load subagent state for {panic_id}: {error}")),
                    }
                }))
            }).collect::<Vec<_>>();
            handles.into_iter().map(|(id, handle)| {
                let panic_id = id.clone();
                (id, handle.join().unwrap_or_else(|_| Outcome::Failed(format!("loader panicked for {panic_id}"))))
            }).collect::<Vec<_>>()
        });

        for (id, outcome) in results {
            match outcome {
                Outcome::Loaded(state) => { inline.insert(id, state); }
                Outcome::Missing if inline.contains_key(&id) => fallback.push(id),
                Outcome::Missing => missing.push(format!("{}:{}", id, refs[&id].as_str())),
                Outcome::Failed(error) => failed.push(error),
            }
        }
    }

    if !missing.is_empty() {
        return Err(format!("subagent state ref blob not found: {}", missing.join(",")));
    }
    if let Some(error) = failed.into_iter().next() {
        return Err(error);
    }
    Ok(SubagentResolution { states: inline, inline_fallback_ids: fallback })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    use std::time::Duration;

    #[test]
    fn inline_fallback_and_missing_diagnostic() {
        let reference = BlobRef::from_content(b"x");
        let refs = BTreeMap::from([("a".into(), reference.clone())]);
        let result = resolve_subagent_state_references(
            BTreeMap::from([("a".into(), "inline".to_string())]), refs,
            |_| Ok(None), |bytes| Ok(String::from_utf8_lossy(bytes).into_owned()),
        ).unwrap();
        assert_eq!(result.states["a"], "inline");
        assert_eq!(result.inline_fallback_ids, vec!["a"]);
        let error = resolve_subagent_state_references(
            BTreeMap::<String, String>::new(),
            BTreeMap::from([("a".into(), reference.clone())]),
            |_| Ok(None), |bytes| Ok(String::from_utf8_lossy(bytes).into_owned()),
        ).unwrap_err();
        assert!(error.contains(reference.as_str()));
    }

    #[test]
    fn fetches_concurrently_with_bound() {
        let mut refs = BTreeMap::new();
        let mut blobs = BTreeMap::new();
        for i in 0..16 {
            let bytes = format!("{i}").into_bytes();
            let reference = BlobRef::from_content(&bytes);
            refs.insert(i.to_string(), reference.clone());
            blobs.insert(reference, bytes);
        }
        let blobs = Arc::new(blobs);
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let active_loader = active.clone();
        let max_loader = maximum.clone();
        let result = resolve_subagent_state_references(
            BTreeMap::<String, String>::new(), refs,
            move |reference| {
                let count = active_loader.fetch_add(1, Ordering::SeqCst) + 1;
                max_loader.fetch_max(count, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(5));
                let bytes = blobs.get(reference).cloned();
                active_loader.fetch_sub(1, Ordering::SeqCst);
                Ok(bytes)
            },
            |bytes| Ok(String::from_utf8_lossy(bytes).into_owned()),
        ).unwrap();
        assert_eq!(result.states.len(), 16);
        assert!(maximum.load(Ordering::SeqCst) > 1);
        assert!(maximum.load(Ordering::SeqCst) <= SUBAGENT_STATE_BLOB_FETCH_CONCURRENCY);
    }
}
