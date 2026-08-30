SELECT name, type
FROM track
WHERE name IS NOT NULL
ORDER BY name;

SELECT name, COUNT(*) AS count, MIN(ts) AS first_ts, MAX(ts) AS last_ts,
       MIN(dur) AS minimum_duration, MAX(dur) AS maximum_duration
FROM slice
GROUP BY name
ORDER BY name;

SELECT counter_track.name, COUNT(*) AS samples,
       MIN(counter.value) AS minimum, MAX(counter.value) AS maximum
FROM counter
JOIN counter_track ON counter.track_id = counter_track.id
GROUP BY counter_track.name
ORDER BY counter_track.name;

SELECT source.name AS outgoing, destination.name AS incoming
FROM flow
JOIN slice source ON flow.slice_out = source.id
JOIN slice destination ON flow.slice_in = destination.id
ORDER BY flow.id;

SELECT clock_id, COUNT(*) AS samples
FROM clock_snapshot
GROUP BY clock_id
ORDER BY clock_id;

SELECT key, display_value
FROM slice
JOIN args USING (arg_set_id)
WHERE slice.name = 'request graph rebuild'
ORDER BY key;

SELECT DISTINCT key, display_value
FROM slice
JOIN args USING (arg_set_id)
WHERE slice.name = 'worker ready'
ORDER BY key, display_value;

SELECT COUNT(*) - COUNT(DISTINCT id) AS duplicate_track_ids FROM track;
SELECT COUNT(*) AS negative_slices FROM slice WHERE dur < 0;
SELECT COALESCE(SUM(value), 0) AS serious_import_errors
FROM stats
WHERE severity != 'info';
