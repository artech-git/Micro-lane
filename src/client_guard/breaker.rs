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
            backoff::equal_jittered(Duration::from_secs(1), Duration::from_secs(30)),
        ))
        .build()
}
