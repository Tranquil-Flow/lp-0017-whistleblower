//! Exponential-backoff retry helper for adapter calls.
//!
//! We don't pull in a retry crate — the policy is small enough to own.
//! `Retryable` adapter errors are retried with backoff; `NonRetryable`
//! errors are returned immediately. After `max_attempts`, the last
//! error is returned even if it was Retryable.

use crate::traits::{AdapterError, AdapterErrorKind};
use std::future::Future;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    /// Multiplied by `initial_backoff * 2^(attempt-1)`. So with
    /// initial=200ms and max_attempts=5: 200, 400, 800, 1600 ms between
    /// attempts. Set to 1 to disable jitter.
    pub backoff_multiplier: u32,
    /// Hard ceiling on each backoff interval — prevents pathological
    /// runaway delays in long-lived workers.
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(200),
            backoff_multiplier: 2,
            max_backoff: Duration::from_secs(10),
        }
    }
}

impl RetryPolicy {
    pub const fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff: Duration::ZERO,
            backoff_multiplier: 1,
            max_backoff: Duration::ZERO,
        }
    }

    fn backoff_for(&self, attempt: u32) -> Duration {
        let exp = self.backoff_multiplier.saturating_pow(attempt.saturating_sub(1));
        let raw = self.initial_backoff.saturating_mul(exp);
        std::cmp::min(raw, self.max_backoff)
    }
}

/// Retry an adapter call according to `policy`. Returns the first success
/// or the last error after `max_attempts`. Non-retryable errors short-circuit.
pub async fn with_retry<F, Fut, T>(
    policy: RetryPolicy,
    mut op: F,
) -> Result<T, AdapterError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AdapterError>>,
{
    let mut last_err: Option<AdapterError> = None;
    for attempt in 1..=policy.max_attempts {
        match op().await {
            Ok(value) => return Ok(value),
            Err(e) if e.kind == AdapterErrorKind::NonRetryable => return Err(e),
            Err(e) => {
                last_err = Some(e);
                if attempt < policy.max_attempts {
                    tokio::time::sleep(policy.backoff_for(attempt)).await;
                }
            }
        }
    }
    Err(last_err.expect("loop ran at least once when max_attempts >= 1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn returns_first_success() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_clone = calls.clone();
        let result: Result<i32, AdapterError> = with_retry(RetryPolicy::default(), move || {
            let calls = calls_clone.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(42)
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_then_succeeds() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_clone = calls.clone();
        let result: Result<i32, AdapterError> = with_retry(
            RetryPolicy {
                max_attempts: 4,
                initial_backoff: Duration::from_millis(1),
                backoff_multiplier: 1,
                max_backoff: Duration::from_millis(1),
            },
            move || {
                let calls = calls_clone.clone();
                async move {
                    let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
                    if n < 3 {
                        Err(AdapterError::retryable("not yet"))
                    } else {
                        Ok(7)
                    }
                }
            },
        )
        .await;
        assert_eq!(result.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn non_retryable_short_circuits() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_clone = calls.clone();
        let result: Result<i32, AdapterError> = with_retry(
            RetryPolicy {
                max_attempts: 5,
                initial_backoff: Duration::from_millis(1),
                backoff_multiplier: 1,
                max_backoff: Duration::from_millis(1),
            },
            move || {
                let calls = calls_clone.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>(AdapterError::non_retryable("permanent"))
                }
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "non-retryable should not be retried"
        );
    }

    #[tokio::test]
    async fn returns_last_error_after_exhausting_attempts() {
        let result: Result<i32, AdapterError> = with_retry(
            RetryPolicy {
                max_attempts: 3,
                initial_backoff: Duration::from_millis(1),
                backoff_multiplier: 1,
                max_backoff: Duration::from_millis(1),
            },
            || async { Err(AdapterError::retryable("flaky")) },
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(err.kind, AdapterErrorKind::Retryable);
        assert!(err.message.contains("flaky"));
    }

    #[test]
    fn backoff_caps_at_max() {
        let policy = RetryPolicy {
            max_attempts: 10,
            initial_backoff: Duration::from_millis(100),
            backoff_multiplier: 2,
            max_backoff: Duration::from_secs(5),
        };
        assert_eq!(policy.backoff_for(1), Duration::from_millis(100));
        assert_eq!(policy.backoff_for(2), Duration::from_millis(200));
        assert_eq!(policy.backoff_for(8), Duration::from_secs(5));
        // Even attempt 100 caps at max.
        assert_eq!(policy.backoff_for(100), Duration::from_secs(5));
    }
}
