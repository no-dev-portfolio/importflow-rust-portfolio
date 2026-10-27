use crate::{
    domain::{Fault, validate_csv},
    store::Store,
};
use std::time::Duration;
use tokio::sync::watch;

pub async fn tick(store: &Store) -> Result<bool, Fault> {
    let Some(job) = store.claim().await? else {
        return Ok(false);
    };
    let csv = job.csv.clone();
    let result = tokio::task::spawn_blocking(move || validate_csv(&csv))
        .await
        .map_err(|_| Fault::Invalid("検証処理が中断されました。".into()))?;
    store
        .finish(&job, result, job.attempts <= job.simulate_failures)
        .await?;
    Ok(true)
}
pub async fn run(store: Store, mut stop: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(Duration::from_millis(300));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
         _=stop.changed()=>break,
         _=interval.tick()=>if let Err(error)=tick(&store).await{tracing::error!(%error,"worker_failed");}
        }
    }
}
