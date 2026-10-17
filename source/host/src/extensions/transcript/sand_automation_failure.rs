pub fn is_background_automation_trigger(trigger: &str) -> bool {
    matches!(trigger, "schedule" | "event")
}

fn is_ascii_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn uuid_len_at(bytes: &[u8], start: usize) -> Option<usize> {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut index = start;
    for (group_index, group_len) in GROUPS.into_iter().enumerate() {
        let end = index.checked_add(group_len)?;
        if end > bytes.len() || !bytes[index..end].iter().all(|byte| is_ascii_hex(*byte)) {
            return None;
        }
        index = end;
        if group_index + 1 != GROUPS.len() {
            if bytes.get(index) != Some(&b'-') {
                return None;
            }
            index += 1;
        }
    }
    Some(index - start)
}

pub fn normalize_automation_error_kind(detail: Option<&str>) -> String {
    let text = detail.unwrap_or("").trim().to_ascii_lowercase();
    if text.is_empty() {
        return "unknown".into();
    }

    let bytes = text.as_bytes();
    let mut normalized = String::with_capacity(bytes.len());
    let mut index = 0usize;
    let mut inside_parentheses = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if inside_parentheses {
            if byte == b')' {
                inside_parentheses = false;
                normalized.push(' ');
            }
            index += 1;
            continue;
        }
        if byte == b'(' {
            inside_parentheses = true;
            normalized.push(' ');
            index += 1;
            continue;
        }
        if let Some(length) = uuid_len_at(bytes, index) {
            normalized.push(' ');
            index += length;
            continue;
        }
        if byte == b'0'
            && bytes.get(index + 1) == Some(&b'x')
            && bytes
                .get(index + 2)
                .is_some_and(|next| is_ascii_hex(*next))
        {
            normalized.push(' ');
            index += 2;
            while bytes.get(index).is_some_and(|next| is_ascii_hex(*next)) {
                index += 1;
            }
            continue;
        }
        if byte.is_ascii_digit() || !byte.is_ascii_lowercase() {
            normalized.push(' ');
        } else {
            normalized.push(char::from(byte));
        }
        index += 1;
    }

    let result = normalized
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    if result.is_empty() {
        "unknown".into()
    } else {
        result
    }
}

pub fn should_notify_automation_failure(occurrence: i64) -> bool {
    occurrence <= 1 || (occurrence > 0 && (occurrence & (occurrence - 1)) == 0)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_trigger_contract_is_exact() {
        assert!(is_background_automation_trigger("schedule"));
        assert!(is_background_automation_trigger("event"));
        assert!(!is_background_automation_trigger("manual"));
        assert!(!is_background_automation_trigger(""));
    }

    #[test]
    fn error_kind_strips_dynamic_ids_numbers_hex_and_parenthetical_detail() {
        let normalized = normalize_automation_error_kind(Some(
            "Runner 42 failed (attempt 7) for 550e8400-e29b-41d4-a716-446655440000 at 0xDEADBEEF: Network Timeout",
        ));
        assert_eq!(normalized, "runner failed for at network timeout");
        assert_eq!(normalize_automation_error_kind(Some("   ")), "unknown");
    }

    #[test]
    fn failure_notification_uses_power_of_two_backoff() {
        let notified = (1..=10)
            .filter(|value| should_notify_automation_failure(*value))
            .collect::<Vec<_>>();
        assert_eq!(notified, vec![1, 2, 4, 8]);
        assert!(should_notify_automation_failure(0));
        assert!(should_notify_automation_failure(-1));
    }
}
