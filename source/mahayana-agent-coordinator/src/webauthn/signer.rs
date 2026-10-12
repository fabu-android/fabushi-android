pub trait ChallengeSigner {
    fn sign(&mut self, relying_party_id: &str, challenge: &[u8]) -> Result<Vec<u8>, String>;
}

pub fn sign_challenge<S: ChallengeSigner>(signer: &mut S, relying_party_id: &str, challenge: &[u8]) -> Result<Vec<u8>, String> {
    if relying_party_id.trim().is_empty() { return Err("relying party id is empty".into()); }
    if challenge.len() < 16 { return Err("challenge is too short".into()); }
    signer.sign(relying_party_id, challenge)
}
