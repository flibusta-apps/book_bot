use tokio::sync::mpsc;

pub struct ClosableSender<T> {
    origin: std::sync::Arc<std::sync::RwLock<Option<mpsc::UnboundedSender<T>>>>,
}

impl<T> Clone for ClosableSender<T> {
    fn clone(&self) -> Self {
        Self {
            origin: self.origin.clone(),
        }
    }
}

impl<T> ClosableSender<T> {
    pub fn new(sender: mpsc::UnboundedSender<T>) -> Self {
        Self {
            origin: std::sync::Arc::new(std::sync::RwLock::new(Some(sender))),
        }
    }

    pub fn get(&self) -> Option<mpsc::UnboundedSender<T>> {
        self.origin.read().unwrap().clone()
    }

    pub fn close(&mut self) {
        self.origin.write().unwrap().take();
    }
}
