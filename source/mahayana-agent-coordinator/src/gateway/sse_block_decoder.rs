#[derive(Default)]
pub struct SseBlockDecoder {
    buffered: String,
}

impl SseBlockDecoder {
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buffered.push_str(chunk);
        let mut blocks=Vec::new();
        while let Some(index)=self.buffered.find("\n\n") {
            blocks.push(self.buffered[..index].to_string());
            self.buffered.drain(..index+2);
        }
        blocks
    }

    pub fn buffered(&self) -> &str { &self.buffered }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delimiter_can_span_chunks() {
        let mut decoder=SseBlockDecoder::default();
        assert!(decoder.push("event: x\n").is_empty());
        assert_eq!(decoder.push("\ndata: y\n\n"), vec!["event: x".to_string(), "data: y".to_string()]);
    }
}
