ALTER TABLE messages ADD COLUMN meeting_id TEXT;
ALTER TABLE messages ADD COLUMN broadcast_id TEXT;

ALTER TABLE agent_jobs ADD COLUMN meeting_id TEXT;

CREATE INDEX IF NOT EXISTS idx_messages_meeting
  ON messages(company_id, meeting_id, created_at);

CREATE INDEX IF NOT EXISTS idx_messages_broadcast
  ON messages(company_id, broadcast_id);

CREATE INDEX IF NOT EXISTS idx_agent_jobs_meeting
  ON agent_jobs(company_id, meeting_id, created_at);
