use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// Preserve the legacy custom policy: INFO-and-higher spans, ERROR events,
/// and no tracing breadcrumbs or structured Sentry logs.
pub fn sentry_span_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    sentry_tracing::SpanLayer::new().with_filter(LevelFilter::INFO)
}

pub fn sentry_error_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    sentry_tracing::ErrorLayer::new().with_filter(LevelFilter::ERROR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn tracing_errors_reach_sentry_without_breadcrumbs() {
        let events = sentry::test::with_captured_events(|| {
            let subscriber = tracing_subscriber::registry()
                .with(sentry_span_layer())
                .with(sentry_error_layer());
            tracing::subscriber::with_default(subscriber, || {
                let _span = tracing::info_span!("request").entered();
                tracing::trace!("ignored trace");
                tracing::debug!("ignored debug");
                tracing::info!("ignored info");
                tracing::warn!("ignored warning");
                tracing::error!(answer = 42, "reported error");
                tracing::error!("second error");
            });
        });

        assert_eq!(events.len(), 2, "only ERROR events should be transported");
        assert_eq!(events[0].level, sentry::Level::Error);
        assert_eq!(events[0].message.as_deref(), Some("reported error"));
        assert_eq!(events[1].message.as_deref(), Some("second error"));
        assert!(events.iter().all(|event| event.breadcrumbs.is_empty()));
        assert!(events[0].contexts.contains_key("trace"));
    }

    #[test]
    fn tracing_spans_keep_legacy_level_filter() {
        let options = sentry::ClientOptions::default().traces_sample_rate(1.0);
        let envelopes = sentry::test::with_captured_envelopes_options(
            || {
                let subscriber = tracing_subscriber::registry()
                    .with(sentry_span_layer())
                    .with(sentry_error_layer());
                tracing::subscriber::with_default(subscriber, || {
                    let _root = tracing::info_span!("request").entered();
                    {
                        let _child = tracing::warn_span!("included child").entered();
                    }
                    {
                        let _child = tracing::debug_span!("excluded child").entered();
                    }
                });
            },
            options,
        );
        let transactions: Vec<_> = envelopes
            .iter()
            .flat_map(|envelope| envelope.items())
            .filter_map(|item| match item {
                sentry::protocol::EnvelopeItem::Transaction(transaction) => Some(transaction),
                _ => None,
            })
            .collect();
        assert_eq!(transactions.len(), 1);
        assert_eq!(transactions[0].name.as_deref(), Some("request"));
        assert_eq!(transactions[0].spans.len(), 1);
        assert_eq!(
            transactions[0].spans[0].description.as_deref(),
            Some("included child")
        );
    }
}
