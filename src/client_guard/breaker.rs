use std::time::Duration;

use failsafe::backoff;
use failsafe::failure_policy::{self, ConsecutiveFailures};

// Consecutive parse failures from one client before its breaker trips.
const CONSECUTIVE_FAILURE_THRESHOLD: u32 = 5;

pub(crate) type ClientCircuitBreaker =
    failsafe::StateMachine<ConsecutiveFailures<backoff::EqualJittered>, ()>;

pub(super) fn new_client_breaker() -> ClientCircuitBreaker {
    failsafe::Config::new()
        .failure_policy(failure_policy::consecutive_failures(
            CONSECUTIVE_FAILURE_THRESHOLD,
            // `equal_jittered` computes seconds as `exp/2 + rand(0, exp/2 + 1)` using integer
            // (truncating) division; a 1s start makes `exp` on the first trip equal to 1, so
            // `exp/2` truncates to 0 and the jitter range collapses to `rand(0, 1)` — always 0.
            // That means a 1s start yields a guaranteed *zero*-second first backoff (the breaker
            // would immediately flip to half-open with no fast-fail effect). A 2s start keeps
            // `exp/2 >= 1` on the first trip, so the backoff is never zero.
            backoff::equal_jittered(Duration::from_secs(2), Duration::from_secs(30)),
        ))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use failsafe::futures::CircuitBreaker as _;
    use failsafe::Error;

    #[tokio::test]
    async fn stays_closed_while_healthy() {
        let breaker = new_client_breaker();

        for _ in 0..10 {
            let result = breaker.call(async { Ok::<_, &str>(()) }).await;
            assert!(result.is_ok());
        }
        assert!(breaker.is_call_permitted());
    }

    #[tokio::test]
    async fn trips_open_after_consecutive_failure_threshold() {
        let breaker = new_client_breaker();

        for _ in 0..CONSECUTIVE_FAILURE_THRESHOLD {
            let result = breaker.call(async { Err::<(), _>("malformed packet") }).await;
            assert!(matches!(result, Err(Error::Inner("malformed packet"))));
        }

        assert!(
            !breaker.is_call_permitted(),
            "breaker should be open after {CONSECUTIVE_FAILURE_THRESHOLD} consecutive failures"
        );

        let result = breaker.call(async { Ok::<(), &str>(()) }).await;
        assert!(
            matches!(result, Err(Error::Rejected)),
            "an open breaker must fast-reject instead of running the inner future"
        );
    }

    #[tokio::test]
    async fn a_success_in_between_does_not_count_towards_the_consecutive_threshold() {
        let breaker = new_client_breaker();

        for _ in 0..CONSECUTIVE_FAILURE_THRESHOLD - 1 {
            let _ = breaker.call(async { Err::<(), _>("malformed packet") }).await;
        }
        // Interrupt the failure streak — the breaker's policy is "consecutive", so this
        // should reset the counter and keep the breaker closed.
        let _ = breaker.call(async { Ok::<_, &str>(()) }).await;

        assert!(breaker.is_call_permitted());
    }
}
