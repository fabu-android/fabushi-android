pub const SEND_DISPATCH_MAX_PLAUSIBLE_MS: u64 = 120_000;
pub const TTFT_MAX_PLAUSIBLE_MS: u64 = 1_800_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSkewReason {
    Negative,
    TooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SanitizedCrossClockDuration {
    Millis(u64),
    Skew(ClockSkewReason),
}

pub fn sanitize_cross_clock_duration_ms(
    raw_delta_ms: f64,
    ceiling_ms: f64,
) -> SanitizedCrossClockDuration {
    if !raw_delta_ms.is_finite() || raw_delta_ms > ceiling_ms {
        return SanitizedCrossClockDuration::Skew(ClockSkewReason::TooLarge);
    }
    if raw_delta_ms < 0.0 {
        return SanitizedCrossClockDuration::Skew(ClockSkewReason::Negative);
    }
    SanitizedCrossClockDuration::Millis(raw_delta_ms.round() as u64)
}

pub fn bucket_clock_skew_delta_ms(raw: f64) -> &'static str {
    if !raw.is_finite() {
        return "nonfinite";
    }
    if raw < 0.0 {
        let magnitude = -raw;
        return if magnitude <= 1_000.0 {
            "neg_le_1s"
        } else if magnitude <= 60_000.0 {
            "neg_le_1m"
        } else {
            "neg_gt_1m"
        };
    }
    if raw <= 60_000.0 {
        "le_1m"
    } else if raw <= 300_000.0 {
        "le_5m"
    } else if raw <= 900_000.0 {
        "le_15m"
    } else {
        "gt_15m"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_valid_cross_clock_duration() {
        assert_eq!(
            sanitize_cross_clock_duration_ms(1_234.6, 2_000.0),
            SanitizedCrossClockDuration::Millis(1_235)
        );
    }

    #[test]
    fn rejects_negative_nonfinite_and_over_ceiling_durations() {
        assert_eq!(
            sanitize_cross_clock_duration_ms(-0.1, 1_000.0),
            SanitizedCrossClockDuration::Skew(ClockSkewReason::Negative)
        );
        assert_eq!(
            sanitize_cross_clock_duration_ms(f64::INFINITY, 1_000.0),
            SanitizedCrossClockDuration::Skew(ClockSkewReason::TooLarge)
        );
        assert_eq!(
            sanitize_cross_clock_duration_ms(1_001.0, 1_000.0),
            SanitizedCrossClockDuration::Skew(ClockSkewReason::TooLarge)
        );
    }

    #[test]
    fn buckets_clock_skew_delta() {
        assert_eq!(bucket_clock_skew_delta_ms(-500.0), "neg_le_1s");
        assert_eq!(bucket_clock_skew_delta_ms(-60_001.0), "neg_gt_1m");
        assert_eq!(bucket_clock_skew_delta_ms(60_000.0), "le_1m");
        assert_eq!(bucket_clock_skew_delta_ms(900_001.0), "gt_15m");
        assert_eq!(bucket_clock_skew_delta_ms(f64::NAN), "nonfinite");
    }
}
