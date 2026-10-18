use capability_layer::breaker::{BreakerState, CircuitBreaker, ProtectionEngine, TokenBucket};
use std::time::Duration;

#[test]
fn test_token_bucket_rate_limiting() {
    let mut bucket = TokenBucket::new(3.0, 10.0);

    assert!(bucket.try_consume(1.0));
    assert!(bucket.try_consume(1.0));
    assert!(bucket.try_consume(1.0));
    assert!(!bucket.try_consume(1.0));
}

#[test]
fn test_circuit_breaker_transitions() {
    let mut breaker = CircuitBreaker::new(3, Duration::from_millis(50));
    assert_eq!(breaker.state, BreakerState::Closed);

    breaker.record_failure();
    assert_eq!(breaker.state, BreakerState::Closed);
    breaker.record_failure();
    assert_eq!(breaker.state, BreakerState::Closed);
    breaker.record_failure();
    assert_eq!(breaker.state, BreakerState::Open);

    assert!(!breaker.allow_request());

    std::thread::sleep(Duration::from_millis(60));
    assert!(breaker.allow_request());
    assert_eq!(breaker.state, BreakerState::HalfOpen);

    breaker.record_success();
    assert_eq!(breaker.state, BreakerState::Closed);
    assert_eq!(breaker.consecutive_failures, 0);
}

#[test]
fn test_exempt_capabilities_never_blocked() {
    let mut engine = ProtectionEngine::default();

    for _ in 0..10 {
        engine.record_outcome("package.install", false);
    }
    assert!(engine.check_breaker("package.install").is_err());

    assert!(engine.check_breaker("ping").is_ok());
    assert!(engine.check_breaker("service.status").is_ok());
    assert!(engine.check_breaker("file.read").is_ok());
}
