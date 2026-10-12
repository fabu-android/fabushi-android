use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AndroidProcessMetricsObservation {
    pub timestamp_ms: u64,
    pub cpu_time_ms: u64,
    pub pss_kb: u64,
    pub java_heap_bytes: u64,
    pub native_heap_bytes: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidProcessMetricsSample {
    pub sample_start_ms: u64,
    pub sample_end_ms: u64,
    pub num_subsamples: u64,
    pub sample_seqno: u64,
    pub avg_pss_kb: f64,
    pub peak_pss_kb: u64,
    pub avg_java_heap_bytes: f64,
    pub peak_java_heap_bytes: u64,
    pub avg_native_heap_bytes: f64,
    pub peak_native_heap_bytes: u64,
    pub cpu_time_ms_delta: u64,
    pub avg_cpu_pct: f64,
    pub peak_cpu_pct: f64,
}

/// Host-owned bounded aggregate for Android process health.
///
/// Android supplies only measurements for its own sandboxed app process. The Host owns aggregation,
/// ordering and reset semantics so Activity recreation cannot create another metrics truth and no
/// arbitrary system-process scan is needed on Android.
#[derive(Debug, Default)]
pub struct AndroidProcessMetricsCollector {
    sample_seqno: u64,
    window_start_ms: Option<u64>,
    window_end_ms: u64,
    num_subsamples: u64,
    pss_sum_kb: u128,
    peak_pss_kb: u64,
    java_heap_sum_bytes: u128,
    peak_java_heap_bytes: u64,
    native_heap_sum_bytes: u128,
    peak_native_heap_bytes: u64,
    cpu_time_ms_delta: u64,
    cpu_pct_sum: f64,
    peak_cpu_pct: f64,
    cpu_pct_count: u64,
    last_timestamp_ms: Option<u64>,
    last_cpu_time_ms: Option<u64>,
}

impl AndroidProcessMetricsCollector {
    pub fn observe(&mut self, observation: AndroidProcessMetricsObservation) -> Result<(), String> {
        if let Some(last) = self.last_timestamp_ms {
            if observation.timestamp_ms <= last {
                return Err("process metrics timestamp must be strictly increasing".into());
            }
        }

        self.window_start_ms.get_or_insert(observation.timestamp_ms);
        self.window_end_ms = observation.timestamp_ms;
        self.num_subsamples = self.num_subsamples.saturating_add(1);
        self.pss_sum_kb = self.pss_sum_kb.saturating_add(observation.pss_kb as u128);
        self.peak_pss_kb = self.peak_pss_kb.max(observation.pss_kb);
        self.java_heap_sum_bytes = self
            .java_heap_sum_bytes
            .saturating_add(observation.java_heap_bytes as u128);
        self.peak_java_heap_bytes = self.peak_java_heap_bytes.max(observation.java_heap_bytes);
        self.native_heap_sum_bytes = self
            .native_heap_sum_bytes
            .saturating_add(observation.native_heap_bytes as u128);
        self.peak_native_heap_bytes = self.peak_native_heap_bytes.max(observation.native_heap_bytes);

        if let (Some(last_at), Some(last_cpu)) = (self.last_timestamp_ms, self.last_cpu_time_ms) {
            let elapsed_ms = observation.timestamp_ms - last_at;
            let cpu_delta = observation.cpu_time_ms.saturating_sub(last_cpu);
            self.cpu_time_ms_delta = self.cpu_time_ms_delta.saturating_add(cpu_delta);
            if elapsed_ms > 0 {
                let cpu_pct = (cpu_delta as f64 / elapsed_ms as f64 * 100.0).max(0.0);
                if cpu_pct.is_finite() {
                    self.cpu_pct_sum += cpu_pct;
                    self.cpu_pct_count = self.cpu_pct_count.saturating_add(1);
                    self.peak_cpu_pct = self.peak_cpu_pct.max(cpu_pct);
                }
            }
        }

        self.last_timestamp_ms = Some(observation.timestamp_ms);
        self.last_cpu_time_ms = Some(observation.cpu_time_ms);
        Ok(())
    }

    pub fn snapshot(&self) -> Option<AndroidProcessMetricsSample> {
        let count = self.num_subsamples;
        if count == 0 {
            return None;
        }
        Some(AndroidProcessMetricsSample {
            sample_start_ms: self.window_start_ms.unwrap_or(self.window_end_ms),
            sample_end_ms: self.window_end_ms,
            num_subsamples: count,
            sample_seqno: self.sample_seqno,
            avg_pss_kb: self.pss_sum_kb as f64 / count as f64,
            peak_pss_kb: self.peak_pss_kb,
            avg_java_heap_bytes: self.java_heap_sum_bytes as f64 / count as f64,
            peak_java_heap_bytes: self.peak_java_heap_bytes,
            avg_native_heap_bytes: self.native_heap_sum_bytes as f64 / count as f64,
            peak_native_heap_bytes: self.peak_native_heap_bytes,
            cpu_time_ms_delta: self.cpu_time_ms_delta,
            avg_cpu_pct: if self.cpu_pct_count == 0 {
                0.0
            } else {
                self.cpu_pct_sum / self.cpu_pct_count as f64
            },
            peak_cpu_pct: self.peak_cpu_pct,
        })
    }

    pub fn take_sample(&mut self) -> Option<AndroidProcessMetricsSample> {
        let sample = self.snapshot()?;
        self.sample_seqno = self.sample_seqno.saturating_add(1);
        self.window_start_ms = self.last_timestamp_ms;
        self.window_end_ms = self.last_timestamp_ms.unwrap_or_default();
        self.num_subsamples = 0;
        self.pss_sum_kb = 0;
        self.peak_pss_kb = 0;
        self.java_heap_sum_bytes = 0;
        self.peak_java_heap_bytes = 0;
        self.native_heap_sum_bytes = 0;
        self.peak_native_heap_bytes = 0;
        self.cpu_time_ms_delta = 0;
        self.cpu_pct_sum = 0.0;
        self.peak_cpu_pct = 0.0;
        self.cpu_pct_count = 0;
        Some(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(at: u64, cpu: u64, pss: u64) -> AndroidProcessMetricsObservation {
        AndroidProcessMetricsObservation {
            timestamp_ms: at,
            cpu_time_ms: cpu,
            pss_kb: pss,
            java_heap_bytes: pss * 1024,
            native_heap_bytes: pss * 2048,
        }
    }

    #[test]
    fn aggregates_bounded_process_health_without_retaining_raw_samples() {
        let mut collector = AndroidProcessMetricsCollector::default();
        collector.observe(observation(1_000, 100, 20)).unwrap();
        collector.observe(observation(2_000, 350, 40)).unwrap();
        let sample = collector.snapshot().unwrap();
        assert_eq!(sample.num_subsamples, 2);
        assert_eq!(sample.avg_pss_kb, 30.0);
        assert_eq!(sample.peak_pss_kb, 40);
        assert_eq!(sample.cpu_time_ms_delta, 250);
        assert_eq!(sample.avg_cpu_pct, 25.0);
        assert_eq!(sample.peak_cpu_pct, 25.0);
    }

    #[test]
    fn rejects_stale_observations_and_rolls_windows_without_losing_cpu_baseline() {
        let mut collector = AndroidProcessMetricsCollector::default();
        collector.observe(observation(1_000, 100, 10)).unwrap();
        assert!(collector.observe(observation(1_000, 120, 11)).is_err());
        collector.observe(observation(2_000, 200, 20)).unwrap();
        let first = collector.take_sample().unwrap();
        assert_eq!(first.sample_seqno, 0);
        assert_eq!(first.num_subsamples, 2);

        collector.observe(observation(3_000, 500, 30)).unwrap();
        let second = collector.take_sample().unwrap();
        assert_eq!(second.sample_seqno, 1);
        assert_eq!(second.num_subsamples, 1);
        assert_eq!(second.cpu_time_ms_delta, 300);
        assert_eq!(second.avg_cpu_pct, 30.0);
    }
}
