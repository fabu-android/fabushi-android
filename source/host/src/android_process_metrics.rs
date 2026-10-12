use crate::process_metrics::{ProcessMetricsSampler, ProcessScanRow};
use serde::Serialize;

const MAX_MEMORY_BYTES: u64 = 16 * 1024 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidProcessMetricsSample {
    pub session_id: String,
    pub sample_seqno: u64,
    pub observed_at_ms: u64,
    pub cpu_time_ms_sample: u64,
    pub memory_mb: f64,
    pub session_peak_memory_mb: f64,
}

#[derive(Default)]
pub struct AndroidProcessMetricsCollector {
    sampler: Option<ProcessMetricsSampler>,
    previous_cpu_time_ms: Option<u64>,
    last_observed_at_ms: Option<u64>,
}

impl AndroidProcessMetricsCollector {
    pub fn observe(
        &mut self,
        observed_at_ms: u64,
        cpu_time_ms_cumulative: u64,
        memory_bytes: u64,
    ) -> Result<AndroidProcessMetricsSample, String> {
        if observed_at_ms == 0 {
            return Err("process metrics observedAtMs must be positive".into());
        }
        if memory_bytes > MAX_MEMORY_BYTES {
            return Err("process metrics memoryBytes exceeds the bounded Android range".into());
        }
        if self
            .last_observed_at_ms
            .is_some_and(|previous| observed_at_ms <= previous)
        {
            return Err("process metrics sample is stale or duplicate".into());
        }
        if self
            .previous_cpu_time_ms
            .is_some_and(|previous| cpu_time_ms_cumulative < previous)
        {
            return Err("process metrics cumulative CPU time moved backwards".into());
        }

        let sampler = self.sampler.get_or_insert_with(|| {
            ProcessMetricsSampler::new(format!("android-process-{observed_at_ms}"))
        });
        let memory_mb = bytes_to_mb(memory_bytes);
        sampler.record_subsample(
            observed_at_ms,
            &[ProcessScanRow {
                pid: std::process::id(),
                ppid: 0,
                // Android owns one app process for Coordinator/Host/JNI. Never ingest a
                // caller-supplied process label or command line into telemetry.
                name: "Fabushi".into(),
                cpu_time_ms_cumulative: cpu_time_ms_cumulative as f64,
                memory_mb,
            }],
        );
        let sample = sampler
            .create_sample()
            .ok_or("process metrics sampler did not produce a sample")?;
        let row = sample
            .rows
            .into_iter()
            .next()
            .ok_or("process metrics sampler omitted the Android app process")?;

        self.previous_cpu_time_ms = Some(cpu_time_ms_cumulative);
        self.last_observed_at_ms = Some(observed_at_ms);

        Ok(AndroidProcessMetricsSample {
            session_id: sample.session_id,
            sample_seqno: sample.sample_seqno,
            observed_at_ms,
            cpu_time_ms_sample: row.cpu_time_ms_sample.round().max(0.0) as u64,
            memory_mb: row.sample_avg_mem_mb,
            session_peak_memory_mb: row.session_peak_mem_mb,
        })
    }
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_metrics_reuse_canonical_sampler_sequence_and_peak() {
        let mut collector = AndroidProcessMetricsCollector::default();
        let first = collector.observe(1_000, 250, 64 * 1024 * 1024).unwrap();
        let second = collector.observe(11_000, 1_250, 96 * 1024 * 1024).unwrap();
        let third = collector.observe(21_000, 1_750, 80 * 1024 * 1024).unwrap();

        assert_eq!(first.sample_seqno, 0);
        assert_eq!(second.sample_seqno, 1);
        assert_eq!(third.sample_seqno, 2);
        assert_eq!(first.session_id, second.session_id);
        assert_eq!(second.cpu_time_ms_sample, 1_000);
        assert_eq!(third.cpu_time_ms_sample, 500);
        assert_eq!(third.memory_mb, 80.0);
        assert_eq!(third.session_peak_memory_mb, 96.0);
    }

    #[test]
    fn process_metrics_reject_invalid_without_advancing_canonical_sampler() {
        let mut collector = AndroidProcessMetricsCollector::default();
        collector.observe(10, 100, 1).unwrap();
        assert!(collector.observe(10, 101, 1).is_err());
        assert!(collector.observe(11, 99, 1).is_err());
        assert!(collector
            .observe(12, 101, MAX_MEMORY_BYTES + 1)
            .is_err());

        let next = collector.observe(20, 150, 2).unwrap();
        assert_eq!(next.sample_seqno, 1);
        assert_eq!(next.cpu_time_ms_sample, 50);
    }
}
