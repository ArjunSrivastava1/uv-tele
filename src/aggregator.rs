use crate::store::MetricsStore;
use crate::db::TelemetryDb;
use crate::models::TelemetryEvent;
use tokio::sync::mpsc::Receiver;

pub async fn run_aggregator(
    mut rx: Receiver<TelemetryEvent>,
    store: MetricsStore,
    db: Option<TelemetryDb>,  // Add this parameter
) {
    let mut batch = Vec::with_capacity(100);
    let mut total_events = 0;
    
    while let Some(event) = rx.recv().await {
        // Update in-memory store
        store.update(event.clone());
        total_events += 1;
        
        // Batch insert to database if available
        if let Some(ref db_conn) = db {
            batch.push(event);
            
            if batch.len() >= 100 {
                if let Err(e) = db_conn.insert_batch(&batch).await {
                    tracing::error!("Failed to insert batch: {}", e);
                } else {
                    tracing::debug!("Inserted {} events to database", batch.len());
                }
                batch.clear();
            }
        }
    }
    
    // Insert remaining events
    if let Some(ref db_conn) = db {
        if !batch.is_empty() {
            match db_conn.insert_batch(&batch).await {
                Ok(_) => tracing::info!("Inserted final {} events to database", batch.len()),
                Err(e) => tracing::error!("Failed to insert final batch: {}", e),
            }
        }
    }
    
    tracing::info!("Aggregator shutting down. Total events processed: {}", total_events);
}