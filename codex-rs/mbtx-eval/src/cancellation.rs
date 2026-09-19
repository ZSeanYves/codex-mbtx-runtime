//! Keep interruption observable across model, preparation and validation phases.
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub(crate) struct Cancellation {
    receiver: watch::Receiver<bool>,
    listener: JoinHandle<()>,
}

impl Cancellation {
    pub(crate) fn listen() -> std::io::Result<Self> {
        // Register synchronously so a signal before the task is polled is kept.
        #[cfg(unix)]
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        let (sender, receiver) = watch::channel(false);
        let listener = tokio::spawn(async move {
            #[cfg(unix)]
            signal.recv().await;
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
            sender.send_replace(true);
        });
        Ok(Self { receiver, listener })
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }

    pub(crate) async fn cancelled(&self) {
        let mut receiver = self.receiver.clone();
        let _ = receiver.wait_for(|cancelled| *cancelled).await;
    }
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        self.listener.abort();
    }
}
