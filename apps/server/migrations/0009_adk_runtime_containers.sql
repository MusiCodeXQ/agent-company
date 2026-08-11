ALTER TABLE employee_runtime_bindings ADD COLUMN container_id TEXT;
ALTER TABLE employee_runtime_bindings ADD COLUMN container_name TEXT;
ALTER TABLE employee_runtime_bindings ADD COLUMN image_ref TEXT;
ALTER TABLE employee_runtime_bindings ADD COLUMN workspace_volume TEXT;
ALTER TABLE employee_runtime_bindings ADD COLUMN state_volume TEXT;
ALTER TABLE employee_runtime_bindings ADD COLUMN artifacts_volume TEXT;

ALTER TABLE agent_jobs ADD COLUMN runtime_run_id TEXT;
ALTER TABLE agent_jobs ADD COLUMN lease_owner TEXT;
ALTER TABLE agent_jobs ADD COLUMN lease_expires_at TEXT;
ALTER TABLE agent_jobs ADD COLUMN heartbeat_at TEXT;

CREATE TABLE IF NOT EXISTS runtime_containers (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL UNIQUE REFERENCES employees(id) ON DELETE CASCADE,
  engine_container_id TEXT UNIQUE,
  container_name TEXT NOT NULL UNIQUE,
  base_image_ref TEXT NOT NULL,
  image_ref TEXT NOT NULL,
  workspace_volume TEXT NOT NULL,
  state_volume TEXT NOT NULL,
  artifacts_volume TEXT NOT NULL,
  desired_status TEXT NOT NULL DEFAULT 'RUNNING'
    CHECK (desired_status IN ('RUNNING', 'STOPPED', 'ARCHIVED')),
  observed_status TEXT NOT NULL DEFAULT 'PENDING',
  generation INTEGER NOT NULL DEFAULT 1,
  last_checkpoint_id TEXT,
  last_health_at TEXT,
  last_error TEXT,
  last_exit_code INTEGER,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runtime_containers_reconcile
  ON runtime_containers(company_id, desired_status, observed_status);

CREATE TABLE IF NOT EXISTS runtime_runs (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  agent_job_id TEXT REFERENCES agent_jobs(id) ON DELETE SET NULL,
  runtime_session_id TEXT,
  container_id TEXT,
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  heartbeat_at TEXT,
  completed_at TEXT,
  stop_reason TEXT,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cache_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_runtime_runs_employee
  ON runtime_runs(company_id, employee_id, started_at DESC);

CREATE TABLE IF NOT EXISTS runtime_tool_calls (
  id TEXT PRIMARY KEY NOT NULL,
  run_id TEXT NOT NULL REFERENCES runtime_runs(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  tool_name TEXT NOT NULL,
  arguments_redacted TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL,
  approval_status TEXT NOT NULL DEFAULT 'NOT_REQUIRED',
  started_at TEXT NOT NULL,
  completed_at TEXT,
  result_summary TEXT
);

CREATE INDEX IF NOT EXISTS idx_runtime_tool_calls_run
  ON runtime_tool_calls(run_id, started_at);

CREATE TABLE IF NOT EXISTS runtime_checkpoints (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  runtime_container_id TEXT NOT NULL REFERENCES runtime_containers(id) ON DELETE CASCADE,
  runtime_run_id TEXT REFERENCES runtime_runs(id) ON DELETE SET NULL,
  checkpoint_key TEXT NOT NULL,
  checkpoint_type TEXT NOT NULL,
  storage_uri TEXT,
  image_ref TEXT,
  workspace_revision TEXT,
  reason TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runtime_checkpoints_employee
  ON runtime_checkpoints(company_id, employee_id, created_at DESC);

CREATE TABLE IF NOT EXISTS face_to_face_sessions (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  initiator_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  target_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  topic TEXT NOT NULL DEFAULT '',
  rendezvous_location_id TEXT REFERENCES office_locations(id) ON DELETE SET NULL,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  accepted_at TEXT,
  started_at TEXT,
  ended_at TEXT,
  end_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_face_to_face_sessions_active
  ON face_to_face_sessions(company_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS face_to_face_participants (
  session_id TEXT NOT NULL REFERENCES face_to_face_sessions(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  arrived_at TEXT,
  last_turn_at TEXT,
  PRIMARY KEY (session_id, employee_id)
);
