use importflow::{
    api,
    domain::{Actor, SAMPLE, Submission},
    store::Store,
    worker,
};
use sqlx::postgres::PgPoolOptions;
use std::{env, error::Error, time::Duration};
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if env::args().nth(1).as_deref() == Some("health") {
        return importflow::health::check(
            &env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into()),
        )
        .await;
    }
    tracing_subscriber::fmt()
        .json()
        .with_env_filter("info")
        .init();
    if env::var("DEMO_MODE").as_deref() != Ok("true") {
        return Err("Local synthetic demo only: set DEMO_MODE=true".into());
    }
    let pool=PgPoolOptions::new().max_connections(10).acquire_timeout(Duration::from_secs(5)).after_connect(|c,_|Box::pin(async move{sqlx::query("SELECT set_config('statement_timeout','5000',false),set_config('lock_timeout','4000',false)").execute(c).await?;Ok(())})).connect(&env::var("DATABASE_URL")?).await?;
    let store = Store { pool };
    store.init().await?;
    if env::var("DEMO_SEED").as_deref() != Ok("false") {
        let a = Actor {
            tenant: "factory-a".into(),
            role: "manager".into(),
        };
        store
            .submit(
                &a,
                "seed-catalog-001",
                Submission {
                    label: "部品マスター 初回取込".into(),
                    csv: SAMPLE.into(),
                    simulate_failures: 0,
                },
            )
            .await?;
        store
            .submit(
                &a,
                "seed-invalid-001",
                Submission {
                    label: "検証エラーのサンプル".into(),
                    csv: "sku,name,quantity,unit_price\nPART-999,不正数量,-2,100\n".into(),
                    simulate_failures: 0,
                },
            )
            .await?;
    }
    let address = env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    let (tx, rx) = watch::channel(false);
    let workers: Vec<_> = (0..2)
        .map(|_| tokio::spawn(worker::run(store.clone(), rx.clone())))
        .collect();
    tracing::info!(%address,"server_started");
    let result = axum::serve(listener, api::router(store.clone()))
        .with_graceful_shutdown(async {
            #[cfg(unix)]
            {
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("SIGTERM handler");
                tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
        })
        .await;
    let _ = tx.send(true);
    for worker in workers {
        worker.await?;
    }
    store.pool.close().await;
    tracing::info!("shutdown_complete");
    result?;
    Ok(())
}
