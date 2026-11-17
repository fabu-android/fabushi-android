//! Telegram 5-bit 100-bar (63-byte) voice waveform codec, PCM RMS normalizer,
//! and <60ms 3-frame Voice Activity Detection (VAD) barge-in detector.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const WAVEFORM_BAR_COUNT: usize = 100;
pub const WAVEFORM_BITS_PER_BAR: usize = 5;
pub const WAVEFORM_MAX_BAR_VALUE: u8 = 31;
pub const WAVEFORM_ENCODED_LEN: usize = 63; // (100 * 5 + 7) / 8 = 63 bytes

/// Packs `WAVEFORM_BAR_COUNT` (or arbitrary count) 5-bit values (0..=31) LSB-first.
/// For 100 bars this produces the canonical 63-byte Telegram voice waveform payload.
pub fn encode_waveform_5bit(bars: &[u8]) -> Vec<u8> {
    let normalized = resample_bars_to_count(bars, WAVEFORM_BAR_COUNT);
    encode_waveform_5bit_raw(&normalized)
}

/// Packs exact `bars` slice into 5-bit LSB-first bitstream without resampling.
pub fn encode_waveform_5bit_raw(bars: &[u8]) -> Vec<u8> {
    let total_bits = bars.len() * WAVEFORM_BITS_PER_BAR;
    let total_bytes = (total_bits + 7) / 8;
    let mut out = vec![0u8; total_bytes];

    for (i, &bar) in bars.iter().enumerate() {
        let clamped = bar.min(WAVEFORM_MAX_BAR_VALUE);
        let bit_pos = i * WAVEFORM_BITS_PER_BAR;
        for bit in 0..WAVEFORM_BITS_PER_BAR {
            if ((clamped >> bit) & 1) != 0 {
                let target_bit = bit_pos + bit;
                let byte_idx = target_bit / 8;
                let bit_offset = target_bit % 8;
                out[byte_idx] |= 1 << bit_offset;
            }
        }
    }
    out
}

/// Unpacks `bar_count` 5-bit values (0..=31) from a Telegram packed bitstream.
pub fn decode_waveform_5bit(encoded: &[u8], bar_count: usize) -> Vec<u8> {
    let mut bars = Vec::with_capacity(bar_count);
    for i in 0..bar_count {
        let bit_pos = i * WAVEFORM_BITS_PER_BAR;
        let mut val = 0u8;
        for bit in 0..WAVEFORM_BITS_PER_BAR {
            let target_bit = bit_pos + bit;
            let byte_idx = target_bit / 8;
            let bit_offset = target_bit % 8;
            if byte_idx < encoded.len() && ((encoded[byte_idx] >> bit_offset) & 1) != 0 {
                val |= 1 << bit;
            }
        }
        bars.push(val);
    }
    bars
}

/// Resamples an arbitrary bar array into `target_count` 5-bit bars (0..=31).
pub fn resample_bars_to_count(bars: &[u8], target_count: usize) -> Vec<u8> {
    if target_count == 0 {
        return Vec::new();
    }
    if bars.is_empty() {
        return vec![0u8; target_count];
    }
    if bars.len() == target_count {
        return bars.iter().map(|&b| b.min(WAVEFORM_MAX_BAR_VALUE)).collect();
    }
    let mut out = Vec::with_capacity(target_count);
    for i in 0..target_count {
        let start = i * bars.len() / target_count;
        let end = ((i + 1) * bars.len() / target_count).max(start + 1).min(bars.len());
        let peak = bars[start..end].iter().copied().max().unwrap_or(0);
        out.push(peak.min(WAVEFORM_MAX_BAR_VALUE));
    }
    out
}

/// Extracts `bar_count` peak-normalized 5-bit bars (0..=31) from mono 16-bit PCM samples.
pub fn extract_bars_from_pcm_i16(samples: &[i16], bar_count: usize) -> Vec<u8> {
    if bar_count == 0 {
        return Vec::new();
    }
    if samples.is_empty() {
        return vec![0u8; bar_count];
    }

    let mut rms_buckets = Vec::with_capacity(bar_count);
    let mut global_peak = 0.0_f64;

    for i in 0..bar_count {
        let start = i * samples.len() / bar_count;
        let end = ((i + 1) * samples.len() / bar_count)
            .max(start + 1)
            .min(samples.len());
        let slice = &samples[start..end];
        let sum_sq: f64 = slice
            .iter()
            .map(|&s| {
                let v = (s as f64) / 32768.0;
                v * v
            })
            .sum();
        let rms = (sum_sq / (slice.len() as f64)).sqrt();
        if rms > global_peak {
            global_peak = rms;
        }
        rms_buckets.push(rms);
    }

    if global_peak <= 1e-6 {
        return vec![0u8; bar_count];
    }

    rms_buckets
        .into_iter()
        .map(|rms| {
            let scaled = ((rms / global_peak) * (WAVEFORM_MAX_BAR_VALUE as f64)).round() as u8;
            scaled.min(WAVEFORM_MAX_BAR_VALUE)
        })
        .collect()
}

/// Decodes little-endian PCM16 bytes into `Vec<i16>`.
pub fn pcm_bytes_to_i16(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// Sub-60ms 3-frame Voice Activity Detection (VAD) & Talk-Mode Barge-In State Machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VadBargeInDetector {
    pub sample_rate_hz: u32,
    pub frame_duration_ms: u32,
    pub required_speech_frames: usize,
    pub rms_threshold: f64,
    pub consecutive_speech_frames: usize,
    pub consecutive_silence_frames: usize,
    pub speech_active: bool,
    pub total_frames_processed: u64,
    pub total_barge_ins: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VadFeedReport {
    pub frames_evaluated: usize,
    pub speech_detected: bool,
    pub barge_in_triggered: bool,
    pub tts_interrupted: bool,
    pub consecutive_speech_frames: usize,
    pub detection_latency_ms: u32,
    pub peak_rms: f64,
}

impl Default for VadBargeInDetector {
    fn default() -> Self {
        Self::new(16_000, 18) // 3 * 18ms = 54ms (< 60ms barge-in SLA)
    }
}

impl VadBargeInDetector {
    pub fn new(sample_rate_hz: u32, frame_duration_ms: u32) -> Self {
        Self {
            sample_rate_hz: sample_rate_hz.max(8_000),
            frame_duration_ms: frame_duration_ms.clamp(10, 20),
            required_speech_frames: 3,
            rms_threshold: 0.025,
            consecutive_speech_frames: 0,
            consecutive_silence_frames: 0,
            speech_active: false,
            total_frames_processed: 0,
            total_barge_ins: 0,
        }
    }

    pub fn reset(&mut self) {
        self.consecutive_speech_frames = 0;
        self.consecutive_silence_frames = 0;
        self.speech_active = false;
    }

    pub fn feed_pcm(&mut self, samples: &[i16], tts_playing: bool) -> VadFeedReport {
        if samples.is_empty() {
            return VadFeedReport {
                frames_evaluated: 0,
                speech_detected: self.speech_active,
                barge_in_triggered: false,
                tts_interrupted: false,
                consecutive_speech_frames: self.consecutive_speech_frames,
                detection_latency_ms: (self.consecutive_speech_frames as u32)
                    * self.frame_duration_ms,
                peak_rms: 0.0,
            };
        }

        let samples_per_frame =
            ((self.sample_rate_hz as usize) * (self.frame_duration_ms as usize) / 1000).max(1);

        let mut frames_evaluated = 0usize;
        let mut barge_in_triggered = false;
        let mut peak_rms = 0.0_f64;

        for chunk in samples.chunks(samples_per_frame) {
            frames_evaluated += 1;
            self.total_frames_processed += 1;

            let sum_sq: f64 = chunk
                .iter()
                .map(|&s| {
                    let v = (s as f64) / 32768.0;
                    v * v
                })
                .sum();
            let rms = (sum_sq / (chunk.len() as f64)).sqrt();
            if rms > peak_rms {
                peak_rms = rms;
            }

            if rms >= self.rms_threshold {
                self.consecutive_speech_frames += 1;
                self.consecutive_silence_frames = 0;
                if self.consecutive_speech_frames >= self.required_speech_frames && !self.speech_active {
                    self.speech_active = true;
                    if tts_playing {
                        barge_in_triggered = true;
                        self.total_barge_ins += 1;
                    }
                } else if self.speech_active && tts_playing && !barge_in_triggered && self.consecutive_speech_frames == self.required_speech_frames {
                    barge_in_triggered = true;
                    self.total_barge_ins += 1;
                }
            } else {
                self.consecutive_silence_frames += 1;
                self.consecutive_speech_frames = 0;
                if self.consecutive_silence_frames >= 5 {
                    self.speech_active = false;
                }
            }
        }

        let latency_frames = self
            .consecutive_speech_frames
            .min(self.required_speech_frames)
            .max(if barge_in_triggered {
                self.required_speech_frames
            } else {
                0
            });
        let detection_latency_ms = (latency_frames as u32) * self.frame_duration_ms;

        VadFeedReport {
            frames_evaluated,
            speech_detected: self.speech_active || self.consecutive_speech_frames >= self.required_speech_frames,
            barge_in_triggered,
            tts_interrupted: barge_in_triggered && tts_playing,
            consecutive_speech_frames: self.consecutive_speech_frames,
            detection_latency_ms,
            peak_rms,
        }
    }
}

pub fn rpc_encode_waveform(params: &Value) -> Result<Value, String> {
    let bars: Vec<u8> = if let Some(arr) = params.get("bars").and_then(Value::as_array) {
        arr.iter()
            .map(|v| v.as_u64().unwrap_or(0).min(31) as u8)
            .collect()
    } else if let Some(pcm_arr) = params.get("pcmSamples").and_then(Value::as_array) {
        let samples: Vec<i16> = pcm_arr
            .iter()
            .map(|v| v.as_i64().unwrap_or(0).clamp(i16::MIN as i64, i16::MAX as i64) as i16)
            .collect();
        extract_bars_from_pcm_i16(&samples, WAVEFORM_BAR_COUNT)
    } else if let Some(b64) = params.get("pcmBase64").and_then(Value::as_str) {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("Invalid pcmBase64: {e}"))?;
        let samples = pcm_bytes_to_i16(&raw);
        extract_bars_from_pcm_i16(&samples, WAVEFORM_BAR_COUNT)
    } else {
        return Err("encodeWaveform requires 'bars', 'pcmSamples', or 'pcmBase64'".into());
    };

    let normalized_100 = resample_bars_to_count(&bars, WAVEFORM_BAR_COUNT);
    let encoded = encode_waveform_5bit_raw(&normalized_100);
    let encoded_b64 = base64::engine::general_purpose::STANDARD.encode(&encoded);
    let encoded_hex: String = encoded.iter().map(|b| format!("{b:02x}")).collect();

    Ok(json!({
        "encodedBase64": encoded_b64,
        "encodedHex": encoded_hex,
        "byteLength": encoded.len(),
        "barCount": normalized_100.len(),
        "bars": normalized_100,
    }))
}

pub fn rpc_decode_waveform(params: &Value) -> Result<Value, String> {
    let bar_count = params
        .get("barCount")
        .and_then(Value::as_u64)
        .unwrap_or(WAVEFORM_BAR_COUNT as u64) as usize;

    let bytes = if let Some(b64) = params.get("encodedBase64").and_then(Value::as_str) {
        base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("Invalid encodedBase64: {e}"))?
    } else if let Some(arr) = params.get("encodedBytes").and_then(Value::as_array) {
        arr.iter()
            .map(|v| v.as_u64().unwrap_or(0).min(255) as u8)
            .collect()
    } else {
        return Err("decodeWaveform requires 'encodedBase64' or 'encodedBytes'".into());
    };

    let bars = decode_waveform_5bit(&bytes, bar_count);
    let peak = bars.iter().copied().max().unwrap_or(0);
    Ok(json!({
        "bars": bars,
        "barCount": bars.len(),
        "peak": peak,
    }))
}
