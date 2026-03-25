use crate::store::MetricsStore;
use crate::models::TelemetryEvent;
use tokio::sync::mpsc::Receiver;

pub async fn run_aggregator(
    mut rx: Receiver<TelemetryEvent>,
    store: MetricsStore,
) {
    while let Some(event) = rx.recv().await {
        store.update(event);
    }
    tracing::info!("Aggregator shutting down");
}