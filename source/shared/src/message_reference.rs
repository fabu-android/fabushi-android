pub const MESSAGE_ADDRESS_EXACT: &str =
    r"^t(?:\d+u(?:a\d+)?|(?:\d+|b)[as]\d+)$";

pub fn is_message_address(value: &str) -> bool {
    let Some(rest) = value.strip_prefix('t') else { return false; };
    if rest.is_empty() { return false; }

    if let Some((thread, suffix)) = rest.split_once('u') {
        if !all_digits(thread) { return false; }
        return suffix.is_empty()
            || suffix.strip_prefix('a').is_some_and(all_digits);
    }

    let bytes = rest.as_bytes();
    for (index, ch) in bytes.iter().enumerate() {
        if *ch == b'a' || *ch == b's' {
            let left = &rest[..index];
            let right = &rest[index + 1..];
            let left_ok = left == "b" || all_digits(left);
            return left_ok && all_digits(right);
        }
    }
    false
}

fn all_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_all_message_address_shapes() {
        for value in ["t1u", "t12ua3", "t1a2", "t9s10", "tba7", "tbs8"] {
            assert!(is_message_address(value), "{value}");
        }
        for value in ["", "t", "tbu", "t1ua", "t1x2", "x1a2", "tbz1"] {
            assert!(!is_message_address(value), "{value}");
        }
    }
}
