use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub capacity: f64,
    pub refill_rate_per_sec: f64,
    pub current_tokens: f64,
    pub last_update: Instant,
}

impl TokenBucket {
    pub fn new(capacity: f64, refill_rate_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_rate_per_sec,
            current_tokens: capacity,
            last_update: Instant::now(),
        }
    }

    pub fn try_consume(&mut self, tokens: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.last_update = now;

        self.current_tokens =
            (self.current_tokens + elapsed * self.refill_rate_per_sec).min(self.capacity);

        if self.current_tokens >= tokens {
            self.current_tokens -= tokens;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub state: BreakerState,
    pub consecutive_failures: u32,
    pub failure_threshold: u32,
    pub cool_down_duration: Duration,
    pub opened_at: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, cool_down_duration: Duration) -> Self {
        Self {
            state: BreakerState::Closed,
            consecutive_failures: 0,
            failure_threshold,
            cool_down_duration,
            opened_at: None,
        }
    }

    pub fn allow_request(&mut self) -> bool {
        match self.state {
            BreakerState::Closed => true,
            BreakerState::Open => {
                if let Some(opened) = self.opened_at {
                    if opened.elapsed() >= self.cool_down_duration {
                        self.state = BreakerState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            BreakerState::HalfOpen => true,
        }
    }

    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = BreakerState::Closed;
        self.opened_at = None;
    }

    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.failure_threshold {
            self.state = BreakerState::Open;
            self.opened_at = Some(Instant::now());
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProtectionEngine {
    pub rate_limiters: HashMap<u32, TokenBucket>,
    pub capability_breakers: HashMap<String, CircuitBreaker>,
    pub default_capacity: f64,
    pub default_refill_rate: f64,
    pub default_failure_threshold: u32,
    pub default_cool_down: Duration,
}

impl Default for ProtectionEngine {
    fn default() -> Self {
        Self {
            rate_limiters: HashMap::new(),
            capability_breakers: HashMap::new(),
            default_capacity: 100.0,
            default_refill_rate: 20.0,
            default_failure_threshold: 5,
            default_cool_down: Duration::from_secs(30),
        }
    }
}

impl ProtectionEngine {
    pub fn check_rate_limit(&mut self, uid: u32) -> bool {
        let cap = self.default_capacity;
        let refill = self.default_refill_rate;
        let bucket = self
            .rate_limiters
            .entry(uid)
            .or_insert_with(|| TokenBucket::new(cap, refill));
        bucket.try_consume(1.0)
    }

    pub fn is_exempt(capability: &str) -> bool {
        matches!(
            capability,
            "ping" | "service.status" | "file.read" | "killswitch"
        )
    }

    pub fn check_breaker(&mut self, capability: &str) -> Result<(), &'static str> {
        if Self::is_exempt(capability) {
            return Ok(());
        }

        let thresh = self.default_failure_threshold;
        let cool = self.default_cool_down;
        let breaker = self
            .capability_breakers
            .entry(capability.to_string())
            .or_insert_with(|| CircuitBreaker::new(thresh, cool));

        if breaker.allow_request() {
            Ok(())
        } else {
            Err("circuit breaker open: capability temporarily disabled for cool-down")
        }
    }

    pub fn record_outcome(&mut self, capability: &str, success: bool) {
        if Self::is_exempt(capability) {
            return;
        }

        let thresh = self.default_failure_threshold;
        let cool = self.default_cool_down;
        let breaker = self
            .capability_breakers
            .entry(capability.to_string())
            .or_insert_with(|| CircuitBreaker::new(thresh, cool));

        if success {
            breaker.record_success();
        } else {
            breaker.record_failure();
        }
    }
}
