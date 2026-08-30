SELECT clock_id, COUNT(*) AS samples
FROM clock_snapshot
GROUP BY clock_id
ORDER BY clock_id;

SELECT name, COUNT(*) AS count, MIN(dur) AS minimum_duration,
       MAX(dur) AS maximum_duration
FROM slice
GROUP BY name
ORDER BY name;

SELECT counter_track.name, COUNT(*) AS samples,
       MIN(counter.value) AS minimum, MAX(counter.value) AS maximum
FROM counter
JOIN counter_track ON counter.track_id = counter_track.id
GROUP BY counter_track.name
ORDER BY counter_track.name;

SELECT key, display_value
FROM slice
JOIN args USING (arg_set_id)
WHERE slice.name = 'trace producer health'
ORDER BY key;

SELECT COALESCE(SUM(value), 0) AS clock_sync_errors
FROM stats
WHERE name GLOB 'clock_sync*' AND severity != 'info';
SELECT COUNT(*) AS negative_slices FROM slice WHERE dur < 0;
