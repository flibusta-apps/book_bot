use teloxide::{dptree, types::CallbackQuery};

pub fn filter_callback_query<T>() -> crate::bots::BotHandler
where
    T: std::str::FromStr + Send + Sync + 'static,
{
    dptree::entry().chain(dptree::filter_map(move |cq: CallbackQuery| {
        cq.data.and_then(|data| T::from_str(data.as_str()).ok())
    }))
}
