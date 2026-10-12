use serde::Serialize;
const MAX_MEMORY_BYTES: u64 = 16 * 1024 * 1024 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidProcessMetricsSample { pub session_id: String, pub sample_seqno: u64, pub observed_at_ms: u64, pub cpu_time_ms_sample: u64, pub memory_mb: f64, pub session_peak_memory_mb: f64 }
#[derive(Default)]
pub struct AndroidProcessMetricsCollector { session_id: Option<String>, previous_cpu_time_ms: Option<u64>, last_observed_at_ms: Option<u64>, sample_seqno: u64, session_peak_memory_bytes: u64 }
impl AndroidProcessMetricsCollector {
    pub fn observe(&mut self, observed_at_ms: u64, cpu_time_ms_cumulative: u64, memory_bytes: u64) -> Result<AndroidProcessMetricsSample, String> {
        if observed_at_ms == 0 { return Err("process metrics observedAtMs must be positive".into()); }
        if memory_bytes > MAX_MEMORY_BYTES { return Err("process metrics memoryBytes exceeds the bounded Android range".into()); }
        if self.last_observed_at_ms.is_some_and(|previous| observed_at_ms <= previous) { return Err("process metrics sample is stale or duplicate".into()); }
        if self.previous_cpu_time_ms.is_some_and(|previous| cpu_time_ms_cumulative < previous) { return Err("process metrics cumulative CPU time moved backwards".into()); }
        let cpu_time_ms_sample = self.previous_cpu_time_ms.map(|previous| cpu_time_ms_cumulative.saturating_sub(previous)).unwrap_or(0);
        self.previous_cpu_time_ms = Some(cpu_time_ms_cumulative); self.last_observed_at_ms = Some(observed_at_ms); self.session_peak_memory_bytes = self.session_peak_memory_bytes.max(memory_bytes);
        let session_id = self.session_id.get_or_insert_with(|| format!("android-process-{observed_at_ms}")).clone();
        let sample_seqno = self.sample_seqno; self.sample_seqno = self.sample_seqno.saturating_add(1);
        Ok(AndroidProcessMetricsSample { session_id, sample_seqno, observed_at_ms, cpu_time_ms_sample, memory_mb: bytes_to_mb(memory_bytes), session_peak_memory_mb: bytes_to_mb(self.session_peak_memory_bytes) })
    }
}
fn bytes_to_mb(bytes: u64) -> f64 { bytes as f64 / (1024.0 * 1024.0) }
#[cfg(test)] mod tests { use super::*;
    #[test] fn process_metrics_keep_one_session_sequence_and_peak() { let mut c=AndroidProcessMetricsCollector::default(); let a=c.observe(1_000,250,64*1024*1024).unwrap(); let b=c.observe(11_000,1_250,96*1024*1024).unwrap(); let d=c.observe(21_000,1_750,80*1024*1024).unwrap(); assert_eq!(a.sample_seqno,0); assert_eq!(b.sample_seqno,1); assert_eq!(d.sample_seqno,2); assert_eq!(a.session_id,b.session_id); assert_eq!(b.cpu_time_ms_sample,1_000); assert_eq!(d.cpu_time_ms_sample,500); assert_eq!(d.memory_mb,80.0); assert_eq!(d.session_peak_memory_mb,96.0); }
    #[test] fn process_metrics_reject_invalid_without_advancing() { let mut c=AndroidProcessMetricsCollector::default(); c.observe(10,100,1).unwrap(); assert!(c.observe(10,101,1).is_err()); assert!(c.observe(11,99,1).is_err()); assert!(c.observe(12,101,MAX_MEMORY_BYTES+1).is_err()); let n=c.observe(20,150,2).unwrap(); assert_eq!(n.sample_seqno,1); assert_eq!(n.cpu_time_ms_sample,50); }
}
