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
use crate::db::TelemetryDb;

pub struct AppState {
    pub store: MetricsStore,
    pub db: Option<TelemetryDb>,
    pub shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Existing endpoints
        .route("/health", get(health_check))
        .route("/metrics", get(get_all_metrics))
        .route("/metrics/:device_id", get(get_device_metrics))
        .route("/stats", get(get_system_stats))
        .route("/simulate/stop", post(stop_simulation))
        // New analytics endpoints
        .route("/analytics/moving-avg/:device_id", get(get_moving_avg))
        .route("/analytics/trips/:device_id", get(get_trips))
        .route("/analytics/percentiles/:device_id", get(get_percentiles))
        .route("/analytics/time-weighted-avg/:device_id", get(get_time_weighted_avg))
        .route("/analytics/health", get(get_device_health))
        .route("/analytics/hourly/:device_id", get(get_hourly_aggregates))
        .with_state(state)
}

// ============ EXISTING HANDLERS ============

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

// ============ NEW SQL ANALYTICS HANDLERS ============

/// GET /analytics/moving-avg/:device_id
/// Returns rolling 10-point moving average of speed
async fn get_moving_avg(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(db) = &state.db {
        match db.get_moving_average(&device_id, 10).await {
            Ok(avg) => {
                let formatted_avg: Vec<serde_json::Value> = avg
                    .into_iter()
                    .map(|(time, speed, moving_avg)| {
                        serde_json::json!({
                            "time": time.to_rfc3339(),
                            "speed_kmh": speed,
                            "moving_avg_kmh": moving_avg
                        })
                    })
                    .collect();
                
                Ok(Json(serde_json::json!({ 
                    "device_id": device_id,
                    "moving_average": formatted_avg,
                    "window_size": 10,
                    "description": "Rolling 10-point moving average of speed (km/h)"
                })))
            },
            Err(e) => {
                tracing::error!("Failed to get moving average: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

/// GET /analytics/trips/:device_id
/// Returns trip segmentation using CTE window functions
async fn get_trips(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(db) = &state.db {
        match db.get_trips(&device_id, 24).await {
            Ok(trips) => {
                // Convert trips to serializable format
                let formatted_trips: Vec<serde_json::Value> = trips
                    .into_iter()
                    .map(|trip| {
                        serde_json::json!({
                            "device_id": trip.device_id,
                            "trip_start": trip.trip_start.to_rfc3339(),
                            "trip_end": trip.trip_end.to_rfc3339(),
                            "duration_seconds": trip.duration_seconds,
                            "readings_count": trip.readings_count,
                            "avg_speed_kmh": trip.avg_speed,
                            "max_speed_kmh": trip.max_speed,
                            "min_battery_percent": trip.min_battery,
                            "avg_battery_percent": trip.avg_battery
                        })
                    })
                    .collect();
                
                Ok(Json(serde_json::json!({ 
                    "device_id": device_id,
                    "trips": formatted_trips,
                    "total_trips": formatted_trips.len(),
                    "time_range_hours": 24,
                    "description": "Trips detected where speed > 5 km/h for at least 10 readings"
                })))
            },
            Err(e) => {
                tracing::error!("Failed to get trips: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

/// GET /analytics/percentiles/:device_id
/// Returns speed percentiles (0th to 100th in 10% increments)
async fn get_percentiles(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(db) = &state.db {
        match db.get_percentiles(&device_id, 24).await {
            Ok(percentiles) => {
                let formatted: Vec<serde_json::Value> = percentiles
                    .into_iter()
                    .map(|(p, speed)| {
                        serde_json::json!({
                            "percentile": p,
                            "speed_kmh": speed
                        })
                    })
                    .collect();
                
                Ok(Json(serde_json::json!({ 
                    "device_id": device_id,
                    "distribution": formatted,
                    "time_range_hours": 24,
                    "description": "Speed percentile distribution"
                })))
            },
            Err(e) => {
                tracing::error!("Failed to get percentiles: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

/// GET /analytics/time-weighted-avg/:device_id
/// Returns time-weighted average speed (accounts for irregular intervals)
async fn get_time_weighted_avg(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(db) = &state.db {
        match db.get_time_weighted_avg_speed(&device_id, 24).await {
            Ok(avg) => Ok(Json(serde_json::json!({ 
                "device_id": device_id,
                "time_weighted_avg_speed_kmh": avg,
                "time_range_hours": 24,
                "description": "Time-weighted average speed (accounts for irregular sampling intervals)"
            }))),
            Err(e) => {
                tracing::error!("Failed to get time-weighted average: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

/// GET /analytics/health
/// Returns real-time health dashboard for all devices
async fn get_device_health(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(db) = &state.db {
        match db.get_device_health().await {
            Ok(health) => {
                let overheating: usize = health.iter().filter(|d| d.overheat_alert).count();
                let low_battery: usize = health.iter().filter(|d| d.low_battery_alert).count();
                let speeding: usize = health.iter().filter(|d| d.speeding_alert).count();
                
                Ok(Json(serde_json::json!({ 
                    "devices": health,
                    "alert_summary": {
                        "overheating_devices": overheating,
                        "low_battery_devices": low_battery,
                        "speeding_devices": speeding,
                        "total_devices": health.len()
                    },
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "description": "Real-time device health with active alerts"
                })))
            },
            Err(e) => {
                tracing::error!("Failed to get device health: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

/// GET /analytics/hourly/:device_id
/// Returns hourly aggregates using TimescaleDB continuous aggregates
async fn get_hourly_aggregates(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(db) = &state.db {
        match db.get_hourly_aggregates(&device_id, 24).await {
            Ok(aggregates) => {
                let formatted: Vec<serde_json::Value> = aggregates
                    .into_iter()
                    .map(|(bucket, avg_speed, max_speed, count)| {
                        serde_json::json!({
                            "hour": bucket.to_rfc3339(),
                            "avg_speed_kmh": avg_speed,
                            "max_speed_kmh": max_speed,
                            "event_count": count
                        })
                    })
                    .collect();
                
                Ok(Json(serde_json::json!({ 
                    "device_id": device_id,
                    "hourly_aggregates": formatted,
                    "time_range_hours": 24,
                    "description": "Hourly aggregates from continuous materialized view"
                })))
            },
            Err(e) => {
                tracing::error!("Failed to get hourly aggregates: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}