-- Enable TimescaleDB extension
CREATE EXTENSION IF NOT EXISTS timescaledb;

-- Create telemetry events table
CREATE TABLE IF NOT EXISTS telemetry_events (
    id UUID DEFAULT gen_random_uuid(),
    time TIMESTAMPTZ NOT NULL,
    device_id TEXT NOT NULL,
    speed_kmh DOUBLE PRECISION NOT NULL,
    battery_percent DOUBLE PRECISION NOT NULL,
    temperature_celsius DOUBLE PRECISION NOT NULL,
    latitude DOUBLE PRECISION NOT NULL,
    longitude DOUBLE PRECISION NOT NULL,
    motor_rpm INTEGER NOT NULL,
    
    -- Add derived fields for faster queries
    speed_category TEXT GENERATED ALWAYS AS (
        CASE 
            WHEN speed_kmh < 30 THEN 'city'
            WHEN speed_kmh < 80 THEN 'suburban'
            ELSE 'highway'
        END
    ) STORED,
    
    battery_level TEXT GENERATED ALWAYS AS (
        CASE 
            WHEN battery_percent < 20 THEN 'critical'
            WHEN battery_percent < 50 THEN 'low'
            ELSE 'normal'
        END
    ) STORED,
    
    PRIMARY KEY (time, device_id, id)
);

-- Convert to hypertable for time-series optimization
SELECT create_hypertable('telemetry_events', 'time', if_not_exists => TRUE);

-- Create indexes for common queries
CREATE INDEX IF NOT EXISTS idx_telemetry_device_time 
ON telemetry_events (device_id, time DESC);

CREATE INDEX IF NOT EXISTS idx_telemetry_time 
ON telemetry_events (time DESC);

CREATE INDEX IF NOT EXISTS idx_telemetry_speed 
ON telemetry_events (speed_kmh) 
WHERE speed_kmh > 100;  -- Partial index for speeding events

-- Create table for aggregated metrics (continuous aggregate)
CREATE TABLE IF NOT EXISTS device_hourly_aggregates (
    time TIMESTAMPTZ NOT NULL,
    device_id TEXT NOT NULL,
    avg_speed DOUBLE PRECISION,
    max_speed DOUBLE PRECISION,
    min_speed DOUBLE PRECISION,
    avg_battery DOUBLE PRECISION,
    avg_temperature DOUBLE PRECISION,
    event_count BIGINT,
    distance_estimate DOUBLE PRECISION  -- speed * time approximation
);

-- Create continuous aggregate view (TimescaleDB feature)
CREATE MATERIALIZED VIEW IF NOT EXISTS telemetry_hourly
WITH (timescaledb.continuous) AS
SELECT 
    time_bucket('1 hour', time) AS bucket,
    device_id,
    AVG(speed_kmh) AS avg_speed,
    MAX(speed_kmh) AS max_speed,
    MIN(speed_kmh) AS min_speed,
    AVG(battery_percent) AS avg_battery,
    AVG(temperature_celsius) AS avg_temp,
    COUNT(*) AS event_count
FROM telemetry_events
GROUP BY bucket, device_id;

-- Add refresh policy (refresh every hour)
SELECT add_continuous_aggregate_policy('telemetry_hourly',
    start_offset => INTERVAL '3 hours',
    end_offset => INTERVAL '1 hour',
    schedule_interval => INTERVAL '1 hour');