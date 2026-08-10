CREATE TABLE IF NOT EXISTS model_profiles (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  protocol TEXT NOT NULL,
  base_url TEXT NOT NULL DEFAULT '',
  encrypted_api_key TEXT,
  api_key_hint TEXT,
  model TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE
);

ALTER TABLE employees ADD COLUMN model_profile_id TEXT REFERENCES model_profiles(id) ON DELETE SET NULL;

UPDATE employees
SET current_state = CASE lower(current_state)
  WHEN 'idle' THEN 'IDLE'
  WHEN 'walking' THEN 'WALKING'
  WHEN 'checking_phone' THEN 'CHECK_PHONE'
  WHEN 'thinking' THEN 'THINKING'
  WHEN 'coding' THEN 'WORKING'
  WHEN 'meeting' THEN 'MEETING'
  WHEN 'waiting' THEN 'WAITING'
  WHEN 'error' THEN 'ERROR'
  ELSE current_state
END;

CREATE TABLE IF NOT EXISTS goals (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'ACTIVE',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE
);

ALTER TABLE tasks ADD COLUMN goal_id TEXT REFERENCES goals(id) ON DELETE SET NULL;

CREATE TABLE IF NOT EXISTS messages (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL,
  from_type TEXT NOT NULL,
  from_id TEXT NOT NULL,
  to_type TEXT NOT NULL,
  to_id TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TEXT NOT NULL,
  read_at TEXT,
  job_id TEXT,
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation
  ON messages(company_id, from_id, to_id, created_at);
CREATE INDEX IF NOT EXISTS idx_messages_unread
  ON messages(company_id, to_type, to_id, read_at);

CREATE TABLE IF NOT EXISTS agent_memory (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL,
  employee_id TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'note',
  content TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE,
  FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_memory_employee
  ON agent_memory(company_id, employee_id, updated_at);

CREATE TABLE IF NOT EXISTS agent_jobs (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL,
  job_type TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'PENDING'
    CHECK (status IN ('PENDING', 'RUNNING', 'WAITING', 'COMPLETED', 'FAILED', 'CANCELLED')),
  employee_id TEXT NOT NULL,
  message_id TEXT,
  goal_id TEXT,
  task_id TEXT,
  payload TEXT NOT NULL DEFAULT '{}',
  result TEXT,
  error TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  available_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (company_id) REFERENCES companies(id) ON DELETE CASCADE,
  FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE,
  FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE SET NULL,
  FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE SET NULL,
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_jobs_queue
  ON agent_jobs(status, available_at, created_at);
