use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Maximum consecutive failures before lockout.
const MAX_FAILURES: u32 = 5;
/// Sliding window over which failures are counted.
const WINDOW: Duration = Duration::from_secs(60);
/// Lockout duration after exceeding MAX_FAILURES.
const LOCKOUT: Duration = Duration::from_secs(300);

struct UserRecord {
    failures: u32,
    window_start: Instant,
    locked_until: Option<Instant>,
}

/// Per-user rate limiter for verification attempts.
///
/// After MAX_FAILURES failed verifications within WINDOW seconds the user is
/// locked out for LOCKOUT seconds.  Engine errors (camera failure, timeout)
/// are not counted as failures — only a deliberate face-not-matched response
/// increments the counter.
pub struct RateLimiter {
    records: HashMap<String, UserRecord>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    /// Return `Ok(())` if the user is allowed to attempt verification.
    /// Return `Err(message)` if the user is currently rate-limited.
    pub fn check(&mut self, user: &str) -> Result<(), String> {
        let now = Instant::now();
        let record = self.records.entry(user.to_string()).or_insert(UserRecord {
            failures: 0,
            window_start: now,
            locked_until: None,
        });

        if let Some(locked_until) = record.locked_until {
            if now < locked_until {
                let remaining = locked_until.duration_since(now).as_secs();
                return Err(format!(
                    "too many failed attempts; try again in {remaining}s"
                ));
            }
            // Lockout expired — reset
            *record = UserRecord {
                failures: 0,
                window_start: now,
                locked_until: None,
            };
        } else if now.duration_since(record.window_start) >= WINDOW {
            // Sliding window expired — reset failure counter
            record.failures = 0;
            record.window_start = now;
        }

        Ok(())
    }

    /// Record a failed verification attempt. May trigger a lockout.
    pub fn record_failure(&mut self, user: &str) {
        let now = Instant::now();
        let record = self.records.entry(user.to_string()).or_insert(UserRecord {
            failures: 0,
            window_start: now,
            locked_until: None,
        });

        if now.duration_since(record.window_start) >= WINDOW {
            record.failures = 0;
            record.window_start = now;
        }

        record.failures += 1;
        if record.failures >= MAX_FAILURES {
            record.locked_until = Some(now + LOCKOUT);
            tracing::warn!(
                user,
                failures = record.failures,
                lockout_secs = LOCKOUT.as_secs(),
                "rate limit triggered — locking user"
            );
        } else {
            tracing::debug!(
                user,
                failures = record.failures,
                max = MAX_FAILURES,
                "verify failed — incrementing failure counter"
            );
        }
    }

    /// Record a successful verification — reset the failure counter.
    pub fn record_success(&mut self, user: &str) {
        self.records.remove(user);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_under_limit() {
        let mut rl = RateLimiter::new();
        for _ in 0..4 {
            assert!(rl.check("alice").is_ok());
            rl.record_failure("alice");
        }
        assert!(rl.check("alice").is_ok());
    }

    #[test]
    fn test_locks_after_max_failures() {
        let mut rl = RateLimiter::new();
        for _ in 0..MAX_FAILURES {
            rl.record_failure("alice");
        }
        assert!(rl.check("alice").is_err());
    }

    #[test]
    fn test_success_clears_counter() {
        let mut rl = RateLimiter::new();
        for _ in 0..4 {
            rl.record_failure("alice");
        }
        rl.record_success("alice");
        // Counter reset — should allow again
        assert!(rl.check("alice").is_ok());
    }

    #[test]
    fn test_independent_per_user() {
        let mut rl = RateLimiter::new();
        for _ in 0..MAX_FAILURES {
            rl.record_failure("alice");
        }
        // bob is unaffected
        assert!(rl.check("bob").is_ok());
        assert!(rl.check("alice").is_err());
    }
}
