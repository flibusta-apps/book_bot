use std::future::Future;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    types::{ChatId, MaybeInaccessibleMessage},
    Bot,
};

use crate::{
    bots::{approved_bot::modules::support::support_command_handler, BotHandlerInternal},
    bots_manager::CHAT_DONATION_NOTIFICATIONS_CACHE,
};

use super::user_settings::{is_need_donate_notifications, mark_donate_notification_sent};

/// Runs the check -> send -> mark sequence for one chat. `send` only runs
/// when `check` reports a notification is needed, and `mark` only runs
/// after `send` succeeds, so a failed Telegram send is never recorded as
/// "sent" server-side, and a failed check never suppresses a future retry.
/// A successful `Ok(())` — whether or not a notification was actually
/// sent — caches the chat for the caller's TTL, throttling how often the
/// server-side check runs; the server remains the source of truth for
/// the notification schedule.
async fn process_donation_notification<CheckFut, SendFut, MarkFut>(
    check: impl FnOnce() -> CheckFut,
    send: impl FnOnce() -> SendFut,
    mark: impl FnOnce() -> MarkFut,
) -> anyhow::Result<()>
where
    CheckFut: Future<Output = anyhow::Result<bool>>,
    SendFut: Future<Output = anyhow::Result<()>>,
    MarkFut: Future<Output = anyhow::Result<()>>,
{
    if check().await? {
        send().await?;
        mark().await?;
    }
    Ok(())
}

pub async fn send_donation_notification(
    bot: &CacheMe<Throttle<Bot>>,
    message: &MaybeInaccessibleMessage,
) -> BotHandlerInternal {
    let chat_id: ChatId = message.chat().id;
    let is_private = message.chat().is_private();

    CHAT_DONATION_NOTIFICATIONS_CACHE
        .entry(chat_id)
        .or_try_insert_with(process_donation_notification(
            move || is_need_donate_notifications(chat_id, is_private),
            move || async move {
                if let MaybeInaccessibleMessage::Regular(message) = message {
                    support_command_handler(*message.clone(), bot).await?;
                }
                Ok(())
            },
            move || mark_donate_notification_sent(chat_id),
        ))
        .await
        .map_err(|err| anyhow::anyhow!("{err:?}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::process_donation_notification;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StdMutex,
    };

    #[tokio::test]
    async fn mark_runs_only_after_a_successful_send() {
        let order: Arc<StdMutex<Vec<&'static str>>> = Arc::new(StdMutex::new(Vec::new()));

        let check_order = order.clone();
        let send_order = order.clone();
        let mark_order = order.clone();

        let result = process_donation_notification(
            move || async move {
                check_order.lock().unwrap().push("check");
                Ok(true)
            },
            move || async move {
                send_order.lock().unwrap().push("send");
                Ok(())
            },
            move || async move {
                mark_order.lock().unwrap().push("mark");
                Ok(())
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(*order.lock().unwrap(), vec!["check", "send", "mark"]);
    }

    #[tokio::test]
    async fn mark_is_not_called_when_send_fails() {
        let mark_called = Arc::new(AtomicBool::new(false));
        let mark_called_in_closure = mark_called.clone();

        let result = process_donation_notification(
            || async { Ok(true) },
            || async { Err(anyhow::anyhow!("telegram send failed")) },
            move || async move {
                mark_called_in_closure.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .await;

        assert!(result.is_err());
        assert!(!mark_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn send_and_mark_are_skipped_when_notification_not_needed() {
        let send_called = Arc::new(AtomicBool::new(false));
        let send_called_in_closure = send_called.clone();

        let result = process_donation_notification(
            || async { Ok(false) },
            move || async move {
                send_called_in_closure.store(true, Ordering::SeqCst);
                Ok(())
            },
            || async { panic!("mark should not be called") },
        )
        .await;

        assert!(result.is_ok());
        assert!(!send_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn propagates_check_errors_without_sending_or_marking() {
        let send_called = Arc::new(AtomicBool::new(false));
        let send_called_in_closure = send_called.clone();

        let result = process_donation_notification(
            || async { Err(anyhow::anyhow!("user-settings service down")) },
            move || async move {
                send_called_in_closure.store(true, Ordering::SeqCst);
                Ok(())
            },
            || async { panic!("mark should not be called") },
        )
        .await;

        assert!(result.is_err());
        assert!(!send_called.load(Ordering::SeqCst));
    }
}
