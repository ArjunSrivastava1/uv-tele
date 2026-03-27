-- View: Trip detection using window functions
CREATE OR REPLACE VIEW trips_view AS
WITH speed_gaps AS (
    SELECT 
        time,
        device_id,
        speed_kmh,
        latitude,
        longitude,
        CASE 
            WHEN speed_kmh > 5 THEN 1 
            ELSE 0 
        END AS is_moving,
        LAG(speed_kmh) OVER (PARTITION BY device_id ORDER BY time) AS prev_speed
    FROM telemetry_events
),
trip_boundaries AS (
    SELECT 
        *,
        SUM(CASE 
            WHEN is_moving = 0 AND prev_speed > 5 THEN 1 
            ELSE 0 
        END) OVER (PARTITION BY device_id ORDER BY time) AS trip_id
    FROM speed_gaps
)
SELECT 
    device_id,
    MIN(time) AS trip_start,
    MAX(time) AS trip_end,
    EXTRACT(EPOCH FROM (MAX(time) - MIN(time))) AS duration_seconds,
    COUNT(*) AS readings_count,
    AVG(speed_kmh) AS avg_speed,
    MAX(speed_kmh) AS max_speed,
    MIN(battery_percent) AS min_battery,
    AVG(battery_percent) AS avg_battery
FROM trip_boundaries
WHERE is_moving = 1
GROUP BY device_id, trip_id
HAVING COUNT(*) > 10  -- Only trips with at least 10 readings
ORDER BY trip_start DESC;

-- View: Device health dashboard
CREATE OR REPLACE VIEW device_health_dashboard AS
SELECT 
    device_id,
    NOW() - MAX(time) AS last_seen_ago,
    COUNT(*) FILTER (WHERE time > NOW() - INTERVAL '5 minutes') AS events_last_5min,
    AVG(speed_kmh) AS current_avg_speed,
    AVG(battery_percent) AS current_avg_battery,
    AVG(temperature_celsius) AS current_avg_temp,
    -- Alert conditions
    EXISTS(SELECT 1 FROM telemetry_events 
           WHERE device_id = t.device_id 
           AND temperature_celsius > 70 
           AND time > NOW() - INTERVAL '1 minute') AS overheat_alert,
    EXISTS(SELECT 1 FROM telemetry_events 
           WHERE device_id = t.device_id 
           AND battery_percent < 15 
           AND time > NOW() - INTERVAL '1 minute') AS low_battery_alert,
    EXISTS(SELECT 1 FROM telemetry_events 
           WHERE device_id = t.device_id 
           AND speed_kmh > 120 
           AND time > NOW() - INTERVAL '1 minute') AS speeding_alert
FROM telemetry_events t
GROUP BY device_id;

-- Function: Get speed percentile distribution
CREATE OR REPLACE FUNCTION get_speed_distribution(
    p_device_id TEXT,
    p_hours INTEGER DEFAULT 24
)
RETURNS TABLE (
    percentile DOUBLE PRECISION,
    speed DOUBLE PRECISION
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        percentile,
        percentile_cont(percentile/100.0) WITHIN GROUP (ORDER BY speed_kmh) as speed
    FROM telemetry_events,
    generate_series(0, 100, 10) AS percentile
    WHERE device_id = p_device_id
    AND time > NOW() - (p_hours || ' hours')::INTERVAL
    GROUP BY percentile
    ORDER BY percentile;
END;
$$ LANGUAGE plpgsql;