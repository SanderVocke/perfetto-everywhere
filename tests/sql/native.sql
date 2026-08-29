SELECT name, COUNT(*) AS count
FROM slice
GROUP BY name
ORDER BY name;

SELECT counter_track.name, COUNT(*) AS samples,
       MIN(counter.value) AS minimum, MAX(counter.value) AS maximum,
       AVG(counter.value) AS average
FROM counter
JOIN counter_track ON counter.track_id = counter_track.id
GROUP BY counter_track.name
ORDER BY counter_track.name;

SELECT key, display_value
FROM slice
JOIN args USING (arg_set_id)
WHERE slice.name = 'compile graph'
ORDER BY key;

SELECT COUNT(*) AS flow_count FROM flow;
SELECT COUNT(*) AS filtered_events FROM slice WHERE name = 'must not appear';
SELECT COALESCE(SUM(value), 0) AS serious_import_errors
FROM stats
WHERE severity != 'info';
