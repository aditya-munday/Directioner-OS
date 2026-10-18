use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RateLimiter {
    capacity: u32,
    refill_rate_per_sec: u32,
    records: HashMap<u32, (u32, i64)>,
}

impl RateLimiter {
    pub fn new(capacity: u32, refill_rate_per_sec: u32) -> Self {
        RateLimiter {
            capacity,
            refill_rate_per_sec,
            records: HashMap::new(),
        }
    }

    pub fn check_and_consume(&mut self, uid: u32, now_secs: i64) -> bool {
        let (tokens, last_refill) = self.records.entry(uid).or_insert((self.capacity, now_secs));
        let elapsed = (now_secs - *last_refill).max(0) as u32;
        let added = elapsed.saturating_mul(self.refill_rate_per_sec);
        *tokens = (*tokens + added).min(self.capacity);
        *last_refill = now_secs;

        if *tokens > 0 {
            *tokens -= 1;
            true
        } else {
            false
        }
    }

    pub fn reset(&mut self, uid: u32) {
        self.records.remove(&uid);
    }
}
