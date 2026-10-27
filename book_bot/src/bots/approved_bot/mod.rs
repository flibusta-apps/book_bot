pub mod modules;
pub mod services;
mod tools;

use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use moka::future::Cache;
use tokio::sync::Semaphore;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::BotCommand,
};

use crate::{
    bots::approved_bot::services::user_settings::{
        default_lang_codes, get_user_settings, save_user_settings,
    },
    bots_manager::USER_ACTIVITY_CACHE,
};

use self::{
    modules::{
        annotations::get_annotations_handler, book::get_book_handler,
        download::get_download_handler, help::get_help_handler, random::get_random_handler,
        search::get_search_handler, settings::get_settings_handler, support::get_support_handler,
        update_history::get_update_log_handler,
    },
    services::user_settings::update_user_activity,
};

use super::{
    ignore_channel_messages, ignore_chat_join_request, ignore_chat_member_update,
    ignore_user_edited_message, registration::get_manager_handler, BotCommands, BotHandler,
};

/// Max concurrent background activity updates.
static ACTIVITY_SEMAPHORE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(16)));

/// Users with an update in flight, so one user never spawns several tasks.
static ACTIVITY_IN_FLIGHT: LazyLock<Mutex<HashSet<UserId>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Short negative cache so a failing service isn't hammered per message.
static ACTIVITY_FAILURE_CACHE: LazyLock<Cache<UserId, ()>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(60))
        .max_capacity(4096)
        .build()
});

struct InFlightGuard(UserId);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        ACTIVITY_IN_FLIGHT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

async fn run_activity_update(me: &teloxide::types::Me, user: &teloxide::types::User) -> bool {
    if update_user_activity(user.id).await.is_ok() {
        return true;
    }

    // Only create a default profile after a confirmed 404; on any service
    // error do nothing so existing settings are never overwritten.
    match get_user_settings(user.id).await {
        Ok(None) => {
            if save_user_settings(user, me, default_lang_codes(), None, Default::default())
                .await
                .is_err()
            {
                return false;
            }
            update_user_activity(user.id).await.is_ok()
        }
        Ok(Some(_)) | Err(_) => false,
    }
}

async fn _update_activity(me: teloxide::types::Me, user: teloxide::types::User) -> Option<()> {
    if USER_ACTIVITY_CACHE.contains_key(&user.id) || ACTIVITY_FAILURE_CACHE.contains_key(&user.id) {
        return None;
    }

    {
        let mut in_flight = ACTIVITY_IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        if !in_flight.insert(user.id) {
            return None;
        }
    }
    let guard = InFlightGuard(user.id);

    let Ok(permit) = ACTIVITY_SEMAPHORE.clone().try_acquire_owned() else {
        // Too many updates in flight; skip, a later message will retry.
        return None;
    };

    tokio::spawn(async move {
        let _guard = guard;
        let _permit = permit;

        if run_activity_update(&me, &user).await {
            USER_ACTIVITY_CACHE.insert(user.id, ()).await;
        } else {
            ACTIVITY_FAILURE_CACHE.insert(user.id, ()).await;
        }
    });

    None
}

fn update_user_activity_handler() -> BotHandler {
    dptree::entry()
        .branch(Update::filter_callback_query().inspect_async(
            |cq: CallbackQuery, bot: CacheMe<Throttle<Bot>>| async move {
                if let Ok(me) = bot.get_me().await {
                    _update_activity(me, cq.from).await;
                }
            },
        ))
        .branch(Update::filter_message().inspect_async(
            |message: Message, bot: CacheMe<Throttle<Bot>>| async move {
                if let Some(user) = message.from {
                    if let Ok(me) = bot.get_me().await {
                        _update_activity(me, user).await;
                    }
                }
            },
        ))
}

pub fn get_approved_handler() -> (BotHandler, BotCommands) {
    (
        dptree::entry()
            .branch(ignore_channel_messages())
            .branch(ignore_chat_member_update())
            .branch(ignore_user_edited_message())
            .branch(ignore_chat_join_request())
            .branch(update_user_activity_handler())
            .branch(get_help_handler())
            .branch(get_settings_handler())
            .branch(get_support_handler())
            .branch(get_random_handler())
            .branch(get_download_handler())
            .branch(get_annotations_handler())
            .branch(get_book_handler())
            .branch(get_update_log_handler())
            .branch(get_manager_handler())
            .branch(get_search_handler()),
        Some(vec![
            BotCommand::new("random", "🎲 Попытать удачу"),
            BotCommand::new("update_log", "🔄 Обновления каталога"),
            BotCommand::new("settings", "⚙️ Настройки"),
            BotCommand::new("donate", "☕️ Поддержать разработчика"),
        ]),
    )
}
