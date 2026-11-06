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

use super::user_settings::try_claim_donate_notification;

/// Atomically claim before sending; a failed send keeps the server-side claim.
async fn process_donation_notification<ClaimFut, SendFut>(
    claim: impl FnOnce() -> ClaimFut,
    send: impl FnOnce() -> SendFut,
) -> anyhow::Result<()>
where
    ClaimFut: Future<Output = anyhow::Result<bool>>,
    SendFut: Future<Output = anyhow::Result<()>>,
{
    if claim().await? {
        if let Err(err) = send().await {
            tracing::warn!("Donation notification send failed after claim: {err:?}");
        }
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
            move || try_claim_donate_notification(chat_id, is_private),
            move || async move {
                if let MaybeInaccessibleMessage::Regular(message) = message {
                    support_command_handler(*message.clone(), bot).await?;
                }
                Ok(())
            },
        ))
        .await
        .map_err(|err| anyhow::anyhow!("{err:?}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unclaimed_notification_is_not_sent() {
        process_donation_notification(|| async { Ok(false) }, || async { panic!("not claimed") })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn failed_send_keeps_claim_and_is_logged() {
        process_donation_notification(
            || async { Ok(true) },
            || async { anyhow::bail!("send failed") },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn claim_failure_propagates_without_sending() {
        assert!(process_donation_notification(
            || async { anyhow::bail!("claim failed") },
            || async { panic!("not claimed") }
        )
        .await
        .is_err());
    }
}
