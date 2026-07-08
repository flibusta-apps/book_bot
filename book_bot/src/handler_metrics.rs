use std::time::Instant;

/// A guard that records handler metrics when dropped.
/// This ensures metrics are recorded even if the handler returns early.
pub struct HandlerMetricsGuard {
    handler_name: &'static str,
    start: Instant,
    success: bool,
}

impl HandlerMetricsGuard {
    pub fn new(handler_name: &'static str) -> Self {
        Self {
            handler_name,
            start: Instant::now(),
            success: true,
        }
    }

    pub fn set_error(&mut self) {
        self.success = false;
    }
}

impl Drop for HandlerMetricsGuard {
    fn drop(&mut self) {
        let status = if self.success { "success" } else { "error" };
        let elapsed = self.start.elapsed().as_secs_f64();

        metrics::counter!("handler_requests_total", "handler" => self.handler_name, "status" => status).increment(1);
        metrics::histogram!("handler_duration_seconds", "handler" => self.handler_name, "status" => status).record(elapsed);
    }
}

#[cfg(test)]
mod tests {
    use book_bot_macros::log_handler;
    use metrics::{
        Counter, CounterFn, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct ErrorCountingRecorder {
        error_increments: Arc<Mutex<u64>>,
    }

    struct ErrorCounter(Arc<Mutex<u64>>);

    impl CounterFn for ErrorCounter {
        fn increment(&self, value: u64) {
            *self.0.lock().unwrap() += value;
        }

        fn absolute(&self, value: u64) {
            *self.0.lock().unwrap() = value;
        }
    }

    impl Recorder for ErrorCountingRecorder {
        fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
        }
        fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
        fn describe_histogram(
            &self,
            _key: KeyName,
            _unit: Option<Unit>,
            _description: SharedString,
        ) {
        }

        fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
            let is_error_counter = key.name() == "handler_requests_total"
                && key
                    .labels()
                    .any(|l| l.key() == "status" && l.value() == "error");

            if is_error_counter {
                Counter::from_arc(Arc::new(ErrorCounter(self.error_increments.clone())))
            } else {
                Counter::noop()
            }
        }

        fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge {
            Gauge::noop()
        }

        fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
            Histogram::noop()
        }
    }

    fn fails() -> anyhow::Result<()> {
        Err(anyhow::anyhow!("boom"))
    }

    #[log_handler("macro_error_propagation_test")]
    async fn fails_via_question_mark() -> anyhow::Result<()> {
        fails()?;
        Ok(())
    }

    #[test]
    fn question_mark_error_increments_error_metric() {
        let recorder = ErrorCountingRecorder::default();
        let error_increments = recorder.error_increments.clone();

        metrics::with_local_recorder(&recorder, || {
            let result = futures::executor::block_on(fails_via_question_mark());
            assert!(result.is_err());
        });

        assert_eq!(*error_increments.lock().unwrap(), 1);
    }
}
