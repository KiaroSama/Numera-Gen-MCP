use std::{future::Future, time::Duration};
use tokio_util::sync::CancellationToken;

/// Computes bounded exponential backoff in milliseconds.
pub fn backoff_ms(attempt: u32, base_ms: u64, cap_ms: u64) -> u64 {
    base_ms.saturating_mul(1u64 << attempt.min(6)).min(cap_ms)
}

/// Poll immediately; cancellation stops the next poll, not a completed result.
pub async fn wait<T, E, F, Fut>(
    cancel: &CancellationToken,
    mut poll: F,
    cancelled: impl Fn() -> E,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>, E>>,
{
    let mut attempt = 0u32;
    loop {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        if let Some(result) = poll().await? {
            return Ok(result);
        }
        let delay = Duration::from_millis(backoff_ms(attempt, 250, 2_000));
        attempt = attempt.saturating_add(1);
        tokio::select! {
            _ = cancel.cancelled() => return Err(cancelled()),
            _ = tokio::time::sleep(delay) => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::wait;
    use std::cell::Cell;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn pre_cancel_preserves_caller_error_without_polling() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let calls = Cell::new(0);
        let error = wait(
            &cancel,
            || {
                calls.set(calls.get() + 1);
                std::future::ready(Ok::<_, &str>(Some(1)))
            },
            || "fixture cancellation",
        )
        .await
        .unwrap_err();
        assert_eq!(error, "fixture cancellation");
        assert_eq!(calls.get(), 0);
    }

    #[tokio::test]
    async fn a_completed_poll_wins_over_later_cancellation() {
        let cancel = CancellationToken::new();
        let result = wait(
            &cancel,
            || {
                cancel.cancel();
                std::future::ready(Ok::<_, &str>(Some(42)))
            },
            || "cancelled",
        )
        .await
        .unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn empty_poll_cancellation_and_poll_errors_return_without_retry() {
        let cancel = CancellationToken::new();
        let calls = Cell::new(0);
        let error = wait(
            &cancel,
            || {
                calls.set(calls.get() + 1);
                cancel.cancel();
                std::future::ready(Ok::<Option<()>, &str>(None))
            },
            || "cancelled",
        )
        .await
        .unwrap_err();
        assert_eq!(error, "cancelled");
        assert_eq!(calls.get(), 1);
        let error = wait(
            &CancellationToken::new(),
            || std::future::ready(Err::<Option<()>, &str>("poll failed")),
            || "cancelled",
        )
        .await
        .unwrap_err();
        assert_eq!(error, "poll failed");
    }
}
