use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use std::sync::{Arc, Mutex};
use crate::store::MetricsStore;
use crate::models::{DeviceMetrics, SystemStats};

pub struct AppState {
    pub store: MetricsStore,
    pub shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(get_all_metrics))
        .route("/metrics/:device_id", get(get_device_metrics))
        .route("/stats", get(get_system_stats))
        .route("/simulate/stop", post(stop_simulation))
        .with_state(state)
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "healthy" }))
}

async fn get_all_metrics(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<DeviceMetrics>> {
    Json(state.store.get_all_metrics())
}

async fn get_device_metrics(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<DeviceMetrics>, StatusCode> {
    state.store
        .get_device_metrics(&device_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_system_stats(
    State(state): State<Arc<AppState>>,
) -> Json<SystemStats> {
    Json(state.store.get_system_stats())
}

async fn stop_simulation(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut tx_guard = state.shutdown_tx.lock().unwrap();
    if let Some(tx) = tx_guard.take() {
        let _ = tx.send(());
        Ok(Json(serde_json::json!({ "status": "shutting_down" })))
    } else {
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}