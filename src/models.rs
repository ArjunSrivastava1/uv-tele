use serde::{Serialize, Serializer};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TelemetryEvent {
    pub id: Uuid,
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub speed_kmh: f64,
    pub battery_percent: f64,
    pub temperature_celsius: f64,
    pub latitude: f64,
    pub longitude: f64,
    pub motor_rpm: u32,
}

impl Serialize for TelemetryEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TelemetryEvent", 9)?;
        state.serialize_field("id", &self.id.to_string())?;
        state.serialize_field("device_id", &self.device_id)?;
        state.serialize_field("timestamp", &self.timestamp.to_rfc3339())?;
        state.serialize_field("speed_kmh", &self.speed_kmh)?;
        state.serialize_field("battery_percent", &self.battery_percent)?;
        state.serialize_field("temperature_celsius", &self.temperature_celsius)?;
        state.serialize_field("latitude", &self.latitude)?;
        state.serialize_field("longitude", &self.longitude)?;
        state.serialize_field("motor_rpm", &self.motor_rpm)?;
        state.end()
    }
}

#[derive(Debug, Clone)]
pub struct DeviceMetrics {
    pub device_id: String,
    pub last_update: DateTime<Utc>,
    pub current_speed: f64,
    pub avg_speed: f64,
    pub max_speed: f64,
    pub battery_level: f64,
    pub temperature: f64,
    pub motor_rpm: u32,
    pub total_events: u64,
}

impl Serialize for DeviceMetrics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("DeviceMetrics", 9)?;
        state.serialize_field("device_id", &self.device_id)?;
        state.serialize_field("last_update", &self.last_update.to_rfc3339())?;
        state.serialize_field("current_speed", &self.current_speed)?;
        state.serialize_field("avg_speed", &self.avg_speed)?;
        state.serialize_field("max_speed", &self.max_speed)?;
        state.serialize_field("battery_level", &self.battery_level)?;
        state.serialize_field("temperature", &self.temperature)?;
        state.serialize_field("motor_rpm", &self.motor_rpm)?;
        state.serialize_field("total_events", &self.total_events)?;
        state.end()
    }
}

#[derive(Debug, Serialize)]
pub struct SystemStats {
    pub active_devices: usize,
    pub total_events_processed: u64,
    pub devices: Vec<DeviceMetrics>,
    pub uptime_seconds: u64,
}