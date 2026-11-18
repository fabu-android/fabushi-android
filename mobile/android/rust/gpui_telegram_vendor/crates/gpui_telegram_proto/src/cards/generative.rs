use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GenerativeCardLayout {
    VerticalStack,
    HorizontalRow,
    KeyValueGrid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GenerativeComponent {
    Text {
        text: String,
        bold: bool,
        color: Option<String>,
    },
    Badge {
        label: String,
        color: String,
    },
    Progress {
        value: f32,
        max: f32,
        label: Option<String>,
    },
    Button {
        id: String,
        label: String,
        action_payload: String,
    },
    Divider,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerativeCard {
    pub card_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub layout: GenerativeCardLayout,
    pub components: Vec<GenerativeComponent>,
    pub status: Option<String>,
}

impl GenerativeCard {
    pub fn sanitize_text(raw: &str) -> String {
        raw.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    /// Sanitizes text for display without double-escaping characters that are
    /// already encoded as HTML entities (e.g. from `append_streaming_delta`).
    pub fn sanitize_text_once(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let bytes = raw.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let rest = &raw[i..];
            if rest.starts_with('&') {
                let known_entity = [
                    "&amp;", "&lt;", "&gt;", "&quot;", "&#39;", "&#x27;",
                ]
                .iter()
                .find(|&&ent| rest.starts_with(ent));
                if let Some(&ent) = known_entity {
                    out.push_str(ent);
                    i += ent.len();
                    continue;
                }
                out.push_str("&amp;");
                i += 1;
            } else {
                let ch = rest.chars().next().unwrap();
                match ch {
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    '"' => out.push_str("&quot;"),
                    '\'' => out.push_str("&#39;"),
                    other => out.push(other),
                }
                i += ch.len_utf8();
            }
        }
        out
    }

    pub fn append_streaming_delta(
        &mut self,
        component_idx: usize,
        delta: &str,
    ) -> Result<(), String> {
        let comp = self
            .components
            .get_mut(component_idx)
            .ok_or_else(|| format!("Component index {component_idx} out of bounds"))?;
        match comp {
            GenerativeComponent::Text { text, .. } => {
                text.push_str(&Self::sanitize_text(delta));
                Ok(())
            }
            _ => Err("Target component is not a Text stream target".into()),
        }
    }

    pub fn clamped_progress(&self) -> Vec<f32> {
        let mut results = Vec::new();
        for comp in &self.components {
            if let GenerativeComponent::Progress { value, max, .. } = comp {
                let ratio = if !value.is_finite() || !max.is_finite() || *max <= 0.0 {
                    0.0
                } else {
                    (*value / *max).clamp(0.0, 1.0)
                };
                results.push(ratio);
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generative_card_sanitize_and_clamp() {
        let mut card = GenerativeCard {
            card_id: "g1".into(),
            title: "Build".into(),
            subtitle: None,
            layout: GenerativeCardLayout::VerticalStack,
            components: vec![
                GenerativeComponent::Text {
                    text: String::new(),
                    bold: false,
                    color: None,
                },
                GenerativeComponent::Progress {
                    value: 150.0,
                    max: 100.0,
                    label: None,
                },
                GenerativeComponent::Progress {
                    value: 10.0,
                    max: 0.0,
                    label: None,
                },
            ],
            status: None,
        };
        card.append_streaming_delta(0, "<script>alert(1)</script>")
            .unwrap();
        assert_eq!(card.clamped_progress(), vec![1.0, 0.0]);
        match &card.components[0] {
            GenerativeComponent::Text { text, .. } => {
                assert_eq!(text, "&lt;script&gt;alert(1)&lt;/script&gt;");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_generative_card_single_quote_and_idempotent_render_sanitize() {
        let escaped = GenerativeCard::sanitize_text("' onerror='alert(1)");
        assert!(!escaped.contains('\''));
        assert_eq!(escaped, "&#39; onerror=&#39;alert(1)");

        let mut card = GenerativeCard {
            card_id: "g2".into(),
            title: "Stream".into(),
            subtitle: None,
            layout: GenerativeCardLayout::VerticalStack,
            components: vec![GenerativeComponent::Text {
                text: String::new(),
                bold: false,
                color: None,
            }],
            status: None,
        };
        card.append_streaming_delta(0, "A & <B> 'x'").unwrap();
        let stored = match &card.components[0] {
            GenerativeComponent::Text { text, .. } => text,
            _ => unreachable!(),
        };
        assert_eq!(stored, "A &amp; &lt;B&gt; &#39;x&#39;");
        assert_eq!(
            GenerativeCard::sanitize_text_once(stored),
            "A &amp; &lt;B&gt; &#39;x&#39;"
        );
        assert_eq!(
            GenerativeCard::sanitize_text_once("<script>alert('1')</script> & raw"),
            "&lt;script&gt;alert(&#39;1&#39;)&lt;/script&gt; &amp; raw"
        );
    }
}
