#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextFragment {
    pub source: String,
    pub text: String,
}

pub fn inject_with_budget(fragments: &[ContextFragment], byte_budget: usize) -> String {
    if byte_budget == 0 {
        return String::new();
    }
    let mut out = String::new();
    for fragment in fragments {
        if fragment.text.is_empty() {
            continue;
        }
        let prefix = if fragment.source.is_empty() {
            String::new()
        } else {
            format!("[{}]\n", fragment.source)
        };
        let separator = if out.is_empty() { "" } else { "\n\n" };
        let fixed = separator.len() + prefix.len();
        if out.len().saturating_add(fixed) >= byte_budget {
            break;
        }
        out.push_str(separator);
        out.push_str(&prefix);
        let remaining = byte_budget.saturating_sub(out.len());
        if fragment.text.len() <= remaining {
            out.push_str(&fragment.text);
        } else {
            let mut end = remaining.min(fragment.text.len());
            while end > 0 && !fragment.text.is_char_boundary(end) {
                end -= 1;
            }
            out.push_str(&fragment.text[..end]);
            break;
        }
    }
    out
}
