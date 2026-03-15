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
