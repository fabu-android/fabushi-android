pub const NOT_RECORD: u8 = 0;
pub const RECORD: u8 = 1;
pub const RECORD_AND_SAMPLED: u8 = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SamplingResult {
    pub decision: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AlwaysOffSampler;

impl AlwaysOffSampler {
    pub fn should_sample(&self) -> SamplingResult {
        SamplingResult {
            decision: NOT_RECORD,
        }
    }
}

impl std::fmt::Display for AlwaysOffSampler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AlwaysOffSampler")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParentBasedSampler {
    pub root: AlwaysOffSampler,
}

impl ParentBasedSampler {
    pub fn should_sample(&self) -> SamplingResult {
        self.root.should_sample()
    }
}

impl std::fmt::Display for ParentBasedSampler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "ParentBased{{root={}}}", self.root)
    }
}

pub fn create_send_trace_sampler() -> ParentBasedSampler {
    ParentBasedSampler {
        root: AlwaysOffSampler,
    }
}
