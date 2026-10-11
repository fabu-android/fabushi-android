#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct RetrySchedule { pub base_ms:u64, pub max_ms:u64 }

impl RetrySchedule {
    pub fn delay_ms(&self,attempt:u32)->u64{
        let shift=attempt.min(20);
        self.base_ms.saturating_mul(1u64<<shift).min(self.max_ms.max(self.base_ms))
    }
    pub fn deadline(start_ms:u64,timeout_ms:u64)->u64{start_ms.saturating_add(timeout_ms)}
}

#[cfg(test)]
mod tests { use super::*; #[test] fn backoff_bounded(){let s=RetrySchedule{base_ms:100,max_ms:1000};assert_eq!(s.delay_ms(0),100);assert_eq!(s.delay_ms(4),1000);} }
