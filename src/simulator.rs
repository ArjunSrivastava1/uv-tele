use crate::models::TelemetryEvent;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use chrono::Utc;
use uuid::Uuid;
use tokio::sync::mpsc::Sender;
use tokio::time::{interval, Duration};

pub async fn start_simulator(
    device_id: String,
    tx: Sender<TelemetryEvent>,
    interval_ms: u64,
) -> Result<(), anyhow::Error> {
    let mut interval = interval(Duration::from_millis(interval_ms));
    // Create a seeded RNG that is Send
    let mut rng = StdRng::from_entropy();
    
    loop {
        interval.tick().await;
        
        let event = TelemetryEvent {
            id: Uuid::new_v4(),
            device_id: device_id.clone(),
            timestamp: Utc::now(),
            speed_kmh: rng.gen_range(0.0..140.0),
            battery_percent: rng.gen_range(20.0..100.0),
            temperature_celsius: rng.gen_range(25.0..75.0),
            latitude: rng.gen_range(12.9..13.1),
            longitude: rng.gen_range(77.5..77.7),
            motor_rpm: rng.gen_range(0..12000),
        };
        
        if tx.send(event).await.is_err() {
            tracing::warn!("Receiver dropped for device {}", device_id);
            break;
        }
    }
    
    Ok(())
}