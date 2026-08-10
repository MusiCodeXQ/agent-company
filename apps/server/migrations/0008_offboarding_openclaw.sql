ALTER TABLE employees ADD COLUMN employment_status TEXT NOT NULL DEFAULT 'ACTIVE'
  CHECK (employment_status IN ('ACTIVE', 'SUSPENDED', 'TERMINATING', 'TERMINATED'));

ALTER TABLE tasks ADD COLUMN previous_assignee_id TEXT;
ALTER TABLE tasks ADD COLUMN handover_status TEXT NOT NULL DEFAULT 'NONE';

CREATE TABLE IF NOT EXISTS runtime_profiles (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  runtime_owner TEXT NOT NULL DEFAULT 'MOCK',
  model_profile_id TEXT REFERENCES model_profiles(id) ON DELETE SET NULL,
  sandbox_profile_id TEXT,
  tool_policy TEXT NOT NULL DEFAULT '{}',
  skills TEXT NOT NULL DEFAULT '[]',
  mcp_policy TEXT NOT NULL DEFAULT '{}',
  config TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runtime_profiles_company
  ON runtime_profiles(company_id, enabled, name);

CREATE TABLE IF NOT EXISTS employee_runtime_bindings (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL UNIQUE REFERENCES employees(id) ON DELETE CASCADE,
  runtime_type TEXT NOT NULL DEFAULT 'MOCK',
  runtime_agent_id TEXT NOT NULL UNIQUE,
  runtime_profile_id TEXT REFERENCES runtime_profiles(id) ON DELETE SET NULL,
  workspace_path TEXT NOT NULL,
  agent_dir TEXT,
  provision_status TEXT NOT NULL DEFAULT 'PENDING',
  runtime_status TEXT NOT NULL DEFAULT 'ACTIVE',
  last_health_at TEXT,
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_employee_runtime_bindings_company
  ON employee_runtime_bindings(company_id, runtime_status, provision_status);

CREATE TABLE IF NOT EXISTS runtime_session_bindings (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  context_type TEXT NOT NULL,
  context_id TEXT NOT NULL,
  runtime_session_key TEXT NOT NULL,
  last_used_at TEXT,
  archived_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(employee_id, context_type, context_id)
);

CREATE INDEX IF NOT EXISTS idx_runtime_sessions_employee
  ON runtime_session_bindings(company_id, employee_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS runtime_events (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  employee_id TEXT REFERENCES employees(id) ON DELETE SET NULL,
  runtime_agent_id TEXT,
  run_id TEXT,
  event_type TEXT NOT NULL,
  payload TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runtime_events_employee
  ON runtime_events(company_id, employee_id, created_at DESC);

CREATE TABLE IF NOT EXISTS termination_records (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL REFERENCES employees(id) ON DELETE RESTRICT,
  initiated_by TEXT NOT NULL,
  reason TEXT NOT NULL DEFAULT '',
  notes TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'PENDING_CONFIRMATION',
  workspace_action TEXT NOT NULL DEFAULT 'ARCHIVE',
  task_handover_status TEXT NOT NULL DEFAULT 'PENDING',
  runtime_shutdown_status TEXT NOT NULL DEFAULT 'PENDING',
  workstation_release_status TEXT NOT NULL DEFAULT 'PENDING',
  created_at TEXT NOT NULL,
  effective_at TEXT,
  completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_termination_records_employee
  ON termination_records(company_id, employee_id, created_at DESC);

CREATE TABLE IF NOT EXISTS task_handover_records (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  termination_id TEXT NOT NULL REFERENCES termination_records(id) ON DELETE CASCADE,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
  previous_assignee_id TEXT,
  new_assignee_id TEXT,
  status TEXT NOT NULL,
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_handover_termination
  ON task_handover_records(company_id, termination_id, status);
