ALTER TABLE runtime_events ADD COLUMN sequence INTEGER NOT NULL DEFAULT 0;
ALTER TABLE runtime_events ADD COLUMN event_key TEXT NOT NULL DEFAULT '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_runtime_events_run_event_key
  ON runtime_events(run_id, event_key)
  WHERE run_id IS NOT NULL AND event_key <> '';
