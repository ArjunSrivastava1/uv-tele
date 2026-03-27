use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::FromRow;  // Add this import
use crate::models::TelemetryEvent;
use anyhow::Result;
use serde::{Serialize, Deserialize};

#[derive(Clone)]
pub struct TelemetryDb {
    pool: PgPool,
}

// Add FromRow here
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Trip {
    pub device_id: String,
    pub trip_start: DateTime<Utc>,
    pub trip_end: DateTime<Utc>,
    pub duration_seconds: f64,
    pub readings_count: i64,
    pub avg_speed: f64,
    pub max_speed: f64,
    pub min_battery: f64,
    pub avg_battery: f64,
}

// Add FromRow here too
#[derive(Debug, Serialize, FromRow)]
pub struct DeviceHealth {
    pub device_id: String,
    pub last_seen_ago: String,
    pub events_last_5min: i64,
    pub current_avg_speed: f64,
    pub current_avg_battery: f64,
    pub current_avg_temp: f64,
    pub overheat_alert: bool,
    pub low_battery_alert: bool,
    pub speeding_alert: bool,
}

impl TelemetryDb {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        
        // Run migrations
        sqlx::migrate!().run(&pool).await?;
        
        Ok(Self { pool })
    }
    
    // Insert a single event
    pub async fn insert_event(&self, event: &TelemetryEvent) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO telemetry_events 
            (time, device_id, speed_kmh, battery_percent, 
             temperature_celsius, latitude, longitude, motor_rpm)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#
        )
        .bind(event.timestamp)
        .bind(&event.device_id)
        .bind(event.speed_kmh)
        .bind(event.battery_percent)
        .bind(event.temperature_celsius)
        .bind(event.latitude)
        .bind(event.longitude)
        .bind(event.motor_rpm as i32)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    // Batch insert for better performance
    pub async fn insert_batch(&self, events: &[TelemetryEvent]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        
        for event in events {
            sqlx::query(
                r#"
                INSERT INTO telemetry_events 
                (time, device_id, speed_kmh, battery_percent, 
                 temperature_celsius, latitude, longitude, motor_rpm)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#
            )
            .bind(event.timestamp)
            .bind(&event.device_id)
            .bind(event.speed_kmh)
            .bind(event.battery_percent)
            .bind(event.temperature_celsius)
            .bind(event.latitude)
            .bind(event.longitude)
            .bind(event.motor_rpm as i32)
            .execute(&mut *tx)
            .await?;
        }
        
        tx.commit().await?;
        Ok(())
    }
    
    // 1. MOVING AVERAGE with window function
    pub async fn get_moving_average(&self, device_id: &str, window_size: i32) -> Result<Vec<(DateTime<Utc>, f64, f64)>> {
        let rows = sqlx::query_as::<_, (DateTime<Utc>, f64, f64)>(
            r#"
            SELECT 
                time,
                speed_kmh,
                AVG(speed_kmh) OVER (
                    PARTITION BY device_id 
                    ORDER BY time 
                    ROWS BETWEEN $1 PRECEDING AND CURRENT ROW
                ) as moving_avg
            FROM telemetry_events
            WHERE device_id = $2
            ORDER BY time DESC
            LIMIT 100
            "#
        )
        .bind(window_size - 1)
        .bind(device_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    // 2. TRIP SEGMENTATION with CTE
    pub async fn get_trips(&self, device_id: &str, hours: i64) -> Result<Vec<Trip>> {
        let trips = sqlx::query_as::<_, Trip>(
            r#"
            WITH speed_threshold AS (
                SELECT 
                    time,
                    speed_kmh,
                    battery_percent,
                    CASE WHEN speed_kmh > 5 THEN 1 ELSE 0 END as is_riding
                FROM telemetry_events
                WHERE device_id = $1
                AND time > NOW() - ($2 || ' hours')::INTERVAL
            ),
            trip_groups AS (
                SELECT 
                    *,
                    SUM(CASE WHEN is_riding = 0 THEN 1 ELSE 0 END) 
                        OVER (ORDER BY time) as trip_group
                FROM speed_threshold
            )
            SELECT 
                $1 as device_id,
                MIN(time) as trip_start,
                MAX(time) as trip_end,
                EXTRACT(EPOCH FROM (MAX(time) - MIN(time))) as duration_seconds,
                COUNT(*) as readings_count,
                AVG(speed_kmh) as avg_speed,
                MAX(speed_kmh) as max_speed,
                MIN(battery_percent) as min_battery,
                AVG(battery_percent) as avg_battery
            FROM trip_groups
            WHERE is_riding = 1
            GROUP BY trip_group
            HAVING COUNT(*) > 10
            ORDER BY trip_start DESC
            "#
        )
        .bind(device_id)
        .bind(hours)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(trips)
    }
    
    // 3. PERCENTILE DISTRIBUTION
    pub async fn get_percentiles(&self, device_id: &str, hours: i64) -> Result<Vec<(f64, f64)>> {
        let rows = sqlx::query_as::<_, (f64, f64)>(
            r#"
            SELECT * FROM get_speed_distribution($1, $2)
            "#
        )
        .bind(device_id)
        .bind(hours as i32)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    // 4. TIME-WEIGHTED AVERAGE
    pub async fn get_time_weighted_avg_speed(&self, device_id: &str, hours: i64) -> Result<f64> {
        let (avg,): (f64,) = sqlx::query_as(
            r#"
            WITH time_diffs AS (
                SELECT 
                    speed_kmh,
                    EXTRACT(EPOCH FROM (time - LAG(time) OVER (ORDER BY time))) as seconds_diff
                FROM telemetry_events
                WHERE device_id = $1
                AND time > NOW() - ($2 || ' hours')::INTERVAL
            )
            SELECT 
                COALESCE(
                    SUM(speed_kmh * seconds_diff) / NULLIF(SUM(seconds_diff), 0),
                    0
                ) as time_weighted_avg
            FROM time_diffs
            "#
        )
        .bind(device_id)
        .bind(hours)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(avg)
    }
    
    // 5. DEVICE HEALTH DASHBOARD
    pub async fn get_device_health(&self) -> Result<Vec<DeviceHealth>> {
        let health = sqlx::query_as::<_, DeviceHealth>(
            r#"
            SELECT 
                device_id,
                EXTRACT(EPOCH FROM (NOW() - MAX(time)))::TEXT || ' seconds' as last_seen_ago,
                COUNT(*) FILTER (WHERE time > NOW() - INTERVAL '5 minutes') as events_last_5min,
                AVG(speed_kmh) as current_avg_speed,
                AVG(battery_percent) as current_avg_battery,
                AVG(temperature_celsius) as current_avg_temp,
                EXISTS(
                    SELECT 1 FROM telemetry_events t2 
                    WHERE t2.device_id = telemetry_events.device_id 
                    AND t2.temperature_celsius > 70 
                    AND t2.time > NOW() - INTERVAL '1 minute'
                ) as overheat_alert,
                EXISTS(
                    SELECT 1 FROM telemetry_events t2 
                    WHERE t2.device_id = telemetry_events.device_id 
                    AND t2.battery_percent < 15 
                    AND t2.time > NOW() - INTERVAL '1 minute'
                ) as low_battery_alert,
                EXISTS(
                    SELECT 1 FROM telemetry_events t2 
                    WHERE t2.device_id = telemetry_events.device_id 
                    AND t2.speed_kmh > 120 
                    AND t2.time > NOW() - INTERVAL '1 minute'
                ) as speeding_alert
            FROM telemetry_events
            GROUP BY device_id
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(health)
    }
    
    // 6. HOURLY AGGREGATES (using continuous aggregate)
    pub async fn get_hourly_aggregates(&self, device_id: &str, hours: i64) -> Result<Vec<(DateTime<Utc>, f64, f64, i64)>> {
        let rows = sqlx::query_as::<_, (DateTime<Utc>, f64, f64, i64)>(
            r#"
            SELECT 
                bucket,
                avg_speed,
                max_speed,
                event_count
            FROM telemetry_hourly
            WHERE device_id = $1
            AND bucket > NOW() - ($2 || ' hours')::INTERVAL
            ORDER BY bucket DESC
            "#
        )
        .bind(device_id)
        .bind(hours)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
}