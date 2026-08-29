SELECT name, value, severity
FROM stats
WHERE name IN (
  'traced_buf_bytes_overwritten',
  'traced_buf_chunks_overwritten',
  'traced_buf_write_wrap_count',
  'traced_buf_incremental_sequences_dropped'
)
ORDER BY name;
