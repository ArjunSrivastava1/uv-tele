mod models;
mod simulator;
mod aggregator;
mod store;
mod api;
mod db;  // Add the db module

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    // Load database URL from environment
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://telemetry:telemetry@localhost:5432/telemetry".to_string());
    
    // Initialize database connection (optional)
    let db = match db::TelemetryDb::new(&database_url).await {
        Ok(db) => {
            tracing::info!("Connected to PostgreSQL/TimescaleDB");
            Some(db)
        }
        Err(e) => {
            tracing::warn!("Failed to connect to database: {} — running without persistence", e);
            None
        }
    };
    
    // Create channel and store
    let (tx, rx) = mpsc::channel::<models::TelemetryEvent>(1000);
    let store = store::MetricsStore::new();
    
    // Start aggregator with database
    let store_clone = store.clone();
    let db_clone = db.clone();
    let aggregator_handle = task::spawn(async move {
        aggregator::run_aggregator(rx, store_clone, db_clone).await;
    });
    
    // Start simulators for multiple devices
    let mut simulator_handles = vec![];
    let devices = vec!["uv-f77-001", "uv-f77-002", "uv-f77-003"];
    
    for device in devices {
        let tx_clone = tx.clone();
        let device_id = device.to_string();
        let handle = task::spawn(async move {
            simulator::start_simulator(device_id, tx_clone, 1000).await
        });
        simulator_handles.push(handle);
    }
    
    // Drop original tx so channel closes when simulators stop
    drop(tx);
    
    // Setup shutdown channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    
    // Wrap shutdown_tx in Arc<Mutex<Option>> for API access
    let shutdown_tx_wrapped = Arc::new(Mutex::new(Some(shutdown_tx)));
    
    // Create API state with database
    let app_state = Arc::new(api::AppState {
        store: store.clone(),
        db: db.clone(),  // Add database to state
        shutdown_tx: shutdown_tx_wrapped,
    });
    
    let app = api::create_router(app_state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    
    tracing::info!("Server running on http://localhost:3000");
    if db.is_some() {
        tracing::info!("Database connected — analytics endpoints available");
    } else {
        tracing::info!("Running without database — analytics endpoints unavailable");
    }
    
    // Run server until shutdown signal
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown_rx.await.ok();
            tracing::info!("Shutting down gracefully");
        })
        .await?;
    
    // Wait for aggregator
    aggregator_handle.await?;
    
    tracing::info!("Shutdown complete");
    
    Ok(())
}