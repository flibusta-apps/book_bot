use tokio::sync::mpsc;

pub struct ClosableSender<T> {
    origin: std::sync::Arc<std::sync::RwLock<Option<mpsc::Sender<T>>>>,
}

impl<T> Clone for ClosableSender<T> {
    fn clone(&self) -> Self {
        Self {
            origin: self.origin.clone(),
        }
    }
}

impl<T> ClosableSender<T> {
    pub fn new(sender: mpsc::Sender<T>) -> Self {
        Self {
            origin: std::sync::Arc::new(std::sync::RwLock::new(Some(sender))),
        }
    }

    pub fn get(&self) -> Option<mpsc::Sender<T>> {
        self.origin
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn close(&mut self) {
        self.origin
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_returns_sender_until_closed() {
        let (tx, mut rx) = mpsc::channel::<i32>(1);
        let mut closable = ClosableSender::new(tx);

        let sender = closable.get().expect("sender should be available");
        sender.try_send(42).unwrap();
        assert_eq!(rx.recv().await, Some(42));

        closable.close();
        assert!(closable.get().is_none());
    }

    #[test]
    fn try_send_fails_when_full() {
        let (tx, _rx) = mpsc::channel::<i32>(1);
        tx.try_send(1).unwrap();

        assert!(matches!(
            tx.try_send(2),
            Err(mpsc::error::TrySendError::Full(2))
        ));
    }
}
