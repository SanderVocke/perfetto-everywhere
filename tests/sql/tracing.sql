SELECT name, COUNT(*) AS count
FROM slice
GROUP BY name
ORDER BY name;

SELECT slice.name, key, display_value
FROM slice
JOIN args USING (arg_set_id)
ORDER BY slice.name, key;

SELECT COALESCE(SUM(value), 0) AS serious_import_errors
FROM stats
WHERE severity != 'info';
