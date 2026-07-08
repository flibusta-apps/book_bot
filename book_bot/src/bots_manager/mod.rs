pub mod axum_server;
pub mod bot_manager_client;
pub mod closable_sender;
pub mod custom_error_handler;
pub mod error_classification;
pub mod internal;
pub mod utils;

use std::sync::LazyLock;
use teloxide::adaptors::throttle::Limits;
use teloxide::stop::{StopFlag, StopToken};
use tokio::task::JoinSet;
use tracing::log;

use std::sync::Arc;
use tokio::sync::watch;

use tokio::sync::Semaphore;
use tokio::time::{interval, sleep, Duration};

use teloxide::prelude::*;

use moka::future::Cache;

use crate::bots_manager::bot_manager_client::delete_bot;
use crate::bots_manager::error_classification::is_expected_telegram_error;
use crate::config;

use self::axum_server::start_axum_server;
use self::bot_manager_client::get_bots;
pub use self::bot_manager_client::{BotCache, BotData};
use self::closable_sender::ClosableSender;
use self::internal::set_webhook;

pub static USER_ACTIVITY_CACHE: LazyLock<Cache<UserId, ()>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(30 * 60))
        .max_capacity(4096)
        .build()
});

pub static CHAT_DONATION_NOTIFICATIONS_CACHE: LazyLock<Cache<ChatId, ()>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(24 * 60 * 60))
        .max_capacity(4096)
        .build()
});

pub static WEBHOOK_CHECK_ERRORS_COUNT: LazyLock<Cache<u32, u32>> =
    LazyLock::new(|| Cache::builder().build());

type StopTokenWithSender = (
    StopToken,
    StopFlag,
    ClosableSender<Result<Update, std::convert::Infallible>>,
    Arc<tokio::task::JoinHandle<()>>,
);

pub static BOTS_ROUTES: LazyLock<Cache<String, StopTokenWithSender>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(60 * 60))
        .eviction_listener(|token: Arc<String>, value: StopTokenWithSender, cause| {
            log::info!(
                "Stop Bot(token={}), cause={cause:?}!",
                crate::bots_manager::utils::mask_token(&token)
            );

            let (stop_token, _stop_flag, mut sender, _dispatcher_handle) = value;

            stop_token.stop();
            sender.close();
        })
        .build()
});

pub static BOTS_DATA: LazyLock<Cache<String, BotData>> = LazyLock::new(|| Cache::builder().build());
pub static INITED_BOTS_IDS: LazyLock<Cache<u32, ()>> = LazyLock::new(|| Cache::builder().build());
pub static COMMANDS_SET_BOT_IDS: LazyLock<Cache<u32, ()>> =
    LazyLock::new(|| Cache::builder().build());

async fn record_webhook_check_success(bot_id: u32) {
    WEBHOOK_CHECK_ERRORS_COUNT.insert(bot_id, 0).await;
}

async fn record_webhook_check_failure(bot_id: u32) -> u32 {
    let error_count = WEBHOOK_CHECK_ERRORS_COUNT.get(&bot_id).await.unwrap_or(0) + 1;
    WEBHOOK_CHECK_ERRORS_COUNT.insert(bot_id, error_count).await;
    error_count
}

async fn webhook_check_breaker_tripped(bot_id: u32) -> bool {
    WEBHOOK_CHECK_ERRORS_COUNT.get(&bot_id).await.unwrap_or(0) >= 3
}

enum WebhookAction {
    NoAction,
    ReSet { url_missing: bool },
}

fn decide_webhook_action(webhook_info: &teloxide::types::WebhookInfo) -> WebhookAction {
    if webhook_info.pending_update_count == 0 {
        return WebhookAction::NoAction;
    }

    if webhook_info.url.is_none() {
        return WebhookAction::ReSet { url_missing: true };
    }

    if let Some(ref err_msg) = webhook_info.last_error_message {
        if is_expected_telegram_error(err_msg) {
            log::warn!("Webhook last error (expected): {err_msg}");
        } else {
            log::error!("Webhook last error: {err_msg}");
        }
        return WebhookAction::ReSet { url_missing: false };
    }

    WebhookAction::NoAction
}

fn record_manager_fetch_failure() {
    metrics::counter!("bots_manager_fetch_failures_total").increment(1);
}

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

async fn wait_for_handles(
    handles: Vec<Arc<tokio::task::JoinHandle<()>>>,
    timeout: Duration,
) -> usize {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = handles.iter().filter(|h| !h.is_finished()).count();
        if remaining == 0 || tokio::time::Instant::now() >= deadline {
            return remaining;
        }
        sleep(Duration::from_millis(100)).await;
    }
}

pub struct BotsManager;

impl BotsManager {
    async fn check_bots_data(bots: &[BotData]) {
        let fresh_tokens: std::collections::HashSet<&str> = bots
            .iter()
            .map(|bot_data| bot_data.token.as_str())
            .collect();

        for bot_data in bots.iter() {
            BOTS_DATA
                .insert(bot_data.token.clone(), bot_data.clone())
                .await;
        }

        let stale_tokens: Vec<String> = BOTS_DATA
            .iter()
            .filter(|(token, _)| !fresh_tokens.contains(token.as_str()))
            .map(|(token, _)| token.as_str().to_string())
            .collect();

        for token in stale_tokens {
            BOTS_DATA.invalidate(&token).await;
            BOTS_ROUTES.remove(&token).await;
        }
    }

    async fn check_uninited(bots_data: &[BotData]) {
        let semaphore = Arc::new(Semaphore::const_new(5));
        let mut set_webhook_tasks = JoinSet::new();

        for bot_data in bots_data.iter() {
            if INITED_BOTS_IDS.contains_key(&bot_data.id) {
                continue;
            }

            let bot_data: BotData = bot_data.clone();

            let semaphore = semaphore.clone();
            set_webhook_tasks.spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                let webhook_status = set_webhook(&bot_data).await;

                if webhook_status {
                    INITED_BOTS_IDS.insert(bot_data.id, ()).await;
                }

                drop(_permit);
            });
        }

        while let Some(res) = set_webhook_tasks.join_next().await {
            if let Err(join_err) = res {
                log::error!("set_webhook task panicked: {join_err:?}");
            }
        }
    }

    async fn check(only_bot_data: bool) {
        let bots_data = get_bots().await;

        let bots_data = match bots_data {
            Ok(v) => v,
            Err(err) => {
                log::error!("Failed to fetch bots from the manager API: {err:?}");
                record_manager_fetch_failure();
                return;
            }
        };

        let _ = BotsManager::check_bots_data(&bots_data).await;

        if !only_bot_data {
            let _ = BotsManager::check_uninited(&bots_data).await;
        }
    }

    pub async fn stop_all() {
        let handles: Vec<Arc<tokio::task::JoinHandle<()>>> = BOTS_ROUTES
            .iter()
            .map(|(_, (stop_token, _, _, handle))| {
                stop_token.stop();
                handle
            })
            .collect();

        BOTS_ROUTES.invalidate_all();

        let total = handles.len();
        let remaining = wait_for_handles(handles, SHUTDOWN_TIMEOUT).await;

        if remaining == 0 {
            log::info!("All {total} bot dispatcher(s) shut down cleanly");
        } else {
            log::warn!(
                "Timed out after {}s waiting for {remaining}/{total} bot dispatcher(s) to shut down",
                SHUTDOWN_TIMEOUT.as_secs()
            );
        }
    }

    pub async fn check_pending_updates() {
        for (token, bot_data) in BOTS_DATA.iter() {
            if webhook_check_breaker_tripped(bot_data.id).await {
                continue;
            }

            let bot = Bot::new(token.clone().as_str())
                .set_api_url(crate::config::CONFIG.telegram_bot_api.clone())
                .throttle(Limits::default());

            let result = bot.get_webhook_info().send().await;

            match result {
                Ok(webhook_info) => {
                    record_webhook_check_success(bot_data.id).await;

                    match decide_webhook_action(&webhook_info) {
                        WebhookAction::NoAction => continue,
                        WebhookAction::ReSet { url_missing } => {
                            if url_missing {
                                log::warn!(
                                    "Webhook URL missing for Bot(id={}) despite pending updates; re-setting",
                                    bot_data.id
                                );
                            }

                            if !set_webhook(&bot_data).await {
                                log::error!("Failed to re-set webhook for Bot(id={})", bot_data.id);
                            }
                        }
                    }
                }
                Err(err) => {
                    let error_message = err.to_string();

                    if error_message.contains("Invalid bot token") {
                        BOTS_DATA.invalidate(token.as_str()).await;
                        if let Err(d_err) = delete_bot(bot_data.id).await {
                            log::error!("Error deleting bot {}: {:?}", bot_data.id, d_err);
                        };
                        continue;
                    }

                    if is_expected_telegram_error(&error_message) {
                        log::warn!("Error getting webhook info (expected): {error_message}");
                    } else {
                        log::error!("Error getting webhook info: {error_message}");
                    }

                    record_webhook_check_failure(bot_data.id).await;
                }
            }
        }
    }

    async fn wait_for_telegram_api() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client for readiness check");

        let url = config::CONFIG.telegram_bot_api.clone();

        log::info!("Waiting for Telegram Bot API at {url} to become ready...");

        let mut attempt: u32 = 0;

        loop {
            match client.get(url.clone()).send().await {
                Ok(_) => {
                    log::info!("Telegram Bot API is ready");
                    return;
                }
                Err(err) => {
                    attempt += 1;
                    let delay = Duration::from_secs(2)
                        .mul_f64(1.5_f64.powi(attempt as i32 - 1))
                        .min(Duration::from_secs(30));

                    log::warn!(
                        "Telegram Bot API not ready yet (attempt {attempt}): {err}. \
                         Retrying in {}s...",
                        delay.as_secs(),
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    pub async fn start(mut shutdown_rx: watch::Receiver<()>) {
        BotsManager::wait_for_telegram_api().await;

        BotsManager::check(true).await;

        let server_handle = match start_axum_server(shutdown_rx.clone()).await {
            Ok(handle) => handle,
            Err(err) => {
                log::error!("Failed to start webhook server: {err}");
                std::process::exit(1);
            }
        };

        let mut tick_number: i32 = 0;
        let mut ticker = interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    BotsManager::stop_all().await;
                    return;
                }
                _ = ticker.tick() => {}
            }

            if server_handle.is_finished() {
                log::error!("Webhook server task exited unexpectedly; shutting down");
                std::process::exit(1);
            }

            if BotsManager::should_run_bots_data_check(tick_number) {
                BotsManager::check(false).await;
            }

            if BotsManager::should_run_pending_updates_check(tick_number) {
                BotsManager::check_pending_updates().await;
            }

            tick_number = (tick_number + 1) % 1800;
        }
    }

    fn should_run_bots_data_check(tick_number: i32) -> bool {
        tick_number % 30 == 0
    }

    fn should_run_pending_updates_check(tick_number: i32) -> bool {
        tick_number % 1800 == 600
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use teloxide::stop::mk_stop_token;

    #[tokio::test]
    async fn invalidating_a_route_stops_its_token_and_closes_its_sender() {
        let (stop_token, stop_flag) = mk_stop_token();
        let (tx, _rx) = tokio::sync::mpsc::channel::<Result<Update, std::convert::Infallible>>(1);
        let closable = ClosableSender::new(tx);
        let handle = Arc::new(tokio::spawn(async {}));

        let test_token = "test-route-invalidation-token".to_string();
        BOTS_ROUTES
            .insert(
                test_token.clone(),
                (stop_token, stop_flag.clone(), closable.clone(), handle),
            )
            .await;

        BOTS_ROUTES.invalidate(&test_token).await;
        BOTS_ROUTES.run_pending_tasks().await;

        assert!(stop_flag.is_stopped());
        assert!(closable.get().is_none());
    }

    fn fake_route() -> StopTokenWithSender {
        let (stop_token, stop_flag) = mk_stop_token();
        let (tx, _rx) = tokio::sync::mpsc::channel::<Result<Update, std::convert::Infallible>>(1);
        let handle = Arc::new(tokio::spawn(async {}));
        (stop_token, stop_flag, ClosableSender::new(tx), handle)
    }

    #[tokio::test]
    async fn sync_upserts_changes_and_removes_bots_absent_from_the_fresh_list() {
        let kept_token = "sync-test-kept-token".to_string();
        let removed_token = "sync-test-removed-token".to_string();

        BOTS_DATA
            .insert(
                kept_token.clone(),
                BotData {
                    id: 101,
                    token: kept_token.clone(),
                    cache: BotCache::Cache,
                },
            )
            .await;
        BOTS_DATA
            .insert(
                removed_token.clone(),
                BotData {
                    id: 102,
                    token: removed_token.clone(),
                    cache: BotCache::Cache,
                },
            )
            .await;
        BOTS_ROUTES
            .insert(removed_token.clone(), fake_route())
            .await;

        let fresh = vec![BotData {
            id: 101,
            token: kept_token.clone(),
            cache: BotCache::NoCache,
        }];
        BotsManager::check_bots_data(&fresh).await;
        BOTS_DATA.run_pending_tasks().await;
        BOTS_ROUTES.run_pending_tasks().await;

        assert!(!BOTS_DATA.contains_key(&removed_token));
        assert!(!BOTS_ROUTES.contains_key(&removed_token));

        let kept = BOTS_DATA
            .get(&kept_token)
            .await
            .expect("kept bot should remain");
        assert_eq!(kept.cache, BotCache::NoCache);
    }

    #[tokio::test]
    async fn breaker_trips_after_three_failures_and_resets_on_success() {
        let bot_id = 900_001;

        assert!(!webhook_check_breaker_tripped(bot_id).await);

        record_webhook_check_failure(bot_id).await;
        record_webhook_check_failure(bot_id).await;
        assert!(!webhook_check_breaker_tripped(bot_id).await);

        record_webhook_check_failure(bot_id).await;
        assert!(webhook_check_breaker_tripped(bot_id).await);

        record_webhook_check_success(bot_id).await;
        assert!(!webhook_check_breaker_tripped(bot_id).await);
    }

    fn webhook_info(
        url: Option<&str>,
        pending: u32,
        last_error: Option<&str>,
    ) -> teloxide::types::WebhookInfo {
        teloxide::types::WebhookInfo {
            url: url.map(|u| reqwest::Url::parse(u).unwrap()),
            has_custom_certificate: false,
            pending_update_count: pending,
            ip_address: None,
            last_error_date: None,
            last_error_message: last_error.map(String::from),
            last_synchronization_error_date: None,
            max_connections: None,
            allowed_updates: None,
        }
    }

    #[test]
    fn no_action_when_no_pending_updates() {
        let info = webhook_info(Some("https://example.com/token/"), 0, Some("boom"));
        assert!(matches!(
            decide_webhook_action(&info),
            WebhookAction::NoAction
        ));
    }

    #[test]
    fn reset_when_url_missing_despite_pending_updates() {
        let info = webhook_info(None, 5, None);
        assert!(matches!(
            decide_webhook_action(&info),
            WebhookAction::ReSet { url_missing: true }
        ));
    }

    #[test]
    fn reset_when_last_error_present() {
        let info = webhook_info(Some("https://example.com/token/"), 5, Some("boom"));
        assert!(matches!(
            decide_webhook_action(&info),
            WebhookAction::ReSet { url_missing: false }
        ));
    }

    #[test]
    fn no_action_when_pending_but_no_error_and_url_present() {
        let info = webhook_info(Some("https://example.com/token/"), 5, None);
        assert!(matches!(
            decide_webhook_action(&info),
            WebhookAction::NoAction
        ));
    }

    #[test]
    fn manager_fetch_failure_increments_metric() {
        use metrics::{
            Counter, CounterFn, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString,
            Unit,
        };
        use std::sync::Mutex as StdMutex;

        #[derive(Default)]
        struct CountingRecorder {
            count: Arc<StdMutex<u64>>,
        }

        struct SharedCounter(Arc<StdMutex<u64>>);

        impl CounterFn for SharedCounter {
            fn increment(&self, value: u64) {
                *self.0.lock().unwrap() += value;
            }
            fn absolute(&self, value: u64) {
                *self.0.lock().unwrap() = value;
            }
        }

        impl Recorder for CountingRecorder {
            fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
            fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
            fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

            fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
                if key.name() == "bots_manager_fetch_failures_total" {
                    Counter::from_arc(Arc::new(SharedCounter(self.count.clone())))
                } else {
                    Counter::noop()
                }
            }

            fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
                Gauge::noop()
            }

            fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
                Histogram::noop()
            }
        }

        let recorder = CountingRecorder::default();
        let count = recorder.count.clone();

        metrics::with_local_recorder(&recorder, || {
            record_manager_fetch_failure();
            record_manager_fetch_failure();
        });

        assert_eq!(*count.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn drains_all_set_webhook_tasks_including_ones_that_panic() {
        let mut set: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
        set.spawn(async {});
        set.spawn(async { panic!("boom") });
        set.spawn(async {});

        let mut panicked = 0;
        while let Some(res) = set.join_next().await {
            if res.is_err() {
                panicked += 1;
            }
        }

        assert_eq!(panicked, 1);
    }

    #[test]
    fn bots_data_check_runs_every_30_ticks_starting_at_zero() {
        assert!(BotsManager::should_run_bots_data_check(0));
        assert!(!BotsManager::should_run_bots_data_check(1));
        assert!(BotsManager::should_run_bots_data_check(30));
        assert!(BotsManager::should_run_bots_data_check(60));
    }

    #[test]
    fn pending_updates_check_runs_once_per_1800_tick_cycle() {
        assert!(!BotsManager::should_run_pending_updates_check(0));
        assert!(BotsManager::should_run_pending_updates_check(600));
        assert!(!BotsManager::should_run_pending_updates_check(601));
        assert!(!BotsManager::should_run_pending_updates_check(1799));
    }

    #[tokio::test]
    async fn wait_for_handles_returns_zero_once_all_tasks_finish() {
        let handles = vec![
            Arc::new(tokio::spawn(async {})),
            Arc::new(tokio::spawn(async {})),
        ];

        let remaining = wait_for_handles(handles, Duration::from_secs(1)).await;
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn wait_for_handles_gives_up_after_timeout() {
        let handles = vec![Arc::new(tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }))];

        let remaining = wait_for_handles(handles, Duration::from_millis(200)).await;
        assert_eq!(remaining, 1);
    }
}
