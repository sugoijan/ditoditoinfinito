use crate::{Error, Result};
use std::{future::Future, time::Duration};
use tokio::{sync::watch, time::Instant};
#[derive(Clone)]
pub struct NetworkCancel(watch::Sender<bool>);
impl Default for NetworkCancel {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}
impl NetworkCancel {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }
    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }
    async fn wait(&self) {
        let mut r = self.0.subscribe();
        if *r.borrow() {
            return;
        }
        let _ = r.changed().await;
    }
    pub(crate) fn check(&self, deadline: Instant) -> Result<()> {
        if self.is_cancelled() {
            return Err(Error::new("request-cancelled", "Acquisition cancelled"));
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "request-timeout",
                "Acquisition deadline exceeded",
            ));
        }
        Ok(())
    }
    pub(crate) async fn until<T>(
        &self,
        deadline: Instant,
        future: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        self.check(deadline)?;
        tokio::select! {biased;
            _=self.wait()=>Err(Error::new("request-cancelled","Acquisition cancelled")),
            _=tokio::time::sleep_until(deadline)=>Err(Error::new("request-timeout","Acquisition deadline exceeded")),
            result=future=>result,
        }
    }
    pub(crate) async fn sleep(&self, deadline: Instant, duration: Duration) -> Result<()> {
        self.until(deadline, async {
            tokio::time::sleep(duration).await;
            Ok(())
        })
        .await
    }
}
