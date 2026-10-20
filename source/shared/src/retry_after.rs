use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn parse_retry_after_header_ms(raw: Option<&str>, now_ms: u64) -> Option<u64> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() { return None; }

    if let Ok(seconds) = trimmed.parse::<f64>() {
        if !seconds.is_finite() { return None; }
        if seconds <= 0.0 { return Some(0); }
        return Some((seconds * 1_000.0).round().max(0.0) as u64);
    }

    let when = httpdate::parse_http_date(trimmed).ok()?;
    let when_ms = when.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
    Some(when_ms.saturating_sub(now_ms))
}

pub fn parse_retry_after_header_ms_now(raw: Option<&str>) -> Option<u64> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64;
    parse_retry_after_header_ms(raw, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_seconds_and_http_dates() {
        assert_eq!(parse_retry_after_header_ms(Some("1.5"), 0), Some(1_500));
        assert_eq!(parse_retry_after_header_ms(Some("-1"), 0), Some(0));
        assert_eq!(
            parse_retry_after_header_ms(Some("Thu, 01 Jan 1970 00:00:10 GMT"), 5_000),
            Some(5_000)
        );
        assert_eq!(parse_retry_after_header_ms(Some("bad"), 0), None);
        assert_eq!(parse_retry_after_header_ms(None, 0), None);
    }
}
