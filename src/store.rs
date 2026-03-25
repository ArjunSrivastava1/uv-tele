use crate::models::{DeviceMetrics, SystemStats, TelemetryEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono::Utc;

#[derive(Clone)]
pub struct MetricsStore {
    inner: Arc<Mutex<StoreInner>>,
}

struct StoreInner {
    devices: HashMap<String, DeviceMetrics>,
    total_events: u64,
    start_time: chrono::DateTime<Utc>,
}

impl MetricsStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(StoreInner {
                devices: HashMap::new(),
                total_events: 0,
                start_time: Utc::now(),
            })),
        }
    }
    
    pub fn update(&self, event: TelemetryEvent) {
        let mut inner = self.inner.lock().unwrap();
        
        inner.total_events += 1;
        
        let metrics = inner.devices
            .entry(event.device_id.clone())
            .or_insert(DeviceMetrics {
                device_id: event.device_id.clone(),
                last_update: event.timestamp,
                current_speed: 0.0,
                avg_speed: 0.0,
                max_speed: 0.0,
                battery_level: 0.0,
                temperature: 0.0,
                motor_rpm: 0,
                total_events: 0,
            });
        
        // Update metrics with exponential moving average
        metrics.current_speed = event.speed_kmh;
        metrics.avg_speed = (metrics.avg_speed * 0.9) + (event.speed_kmh * 0.1);
        metrics.max_speed = metrics.max_speed.max(event.speed_kmh);
        metrics.battery_level = event.battery_percent;
        metrics.temperature = event.temperature_celsius;
        metrics.motor_rpm = event.motor_rpm;
        metrics.last_update = event.timestamp;
        metrics.total_events += 1;
    }
    
    pub fn get_device_metrics(&self, device_id: &str) -> Option<DeviceMetrics> {
        self.inner.lock().unwrap().devices.get(device_id).cloned()
    }
    
    pub fn get_system_stats(&self) -> SystemStats {
        let inner = self.inner.lock().unwrap();
        SystemStats {
            active_devices: inner.devices.len(),
            total_events_processed: inner.total_events,
            devices: inner.devices.values().cloned().collect(),
            uptime_seconds: (Utc::now() - inner.start_time).num_seconds() as u64,
        }
    }
    
    pub fn get_all_metrics(&self) -> Vec<DeviceMetrics> {
        self.inner.lock().unwrap()
            .devices
            .values()
            .cloned()
            .collect()
    }
}