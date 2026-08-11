ALTER TABLE employee_runtime_bindings ADD COLUMN environment_path TEXT;
ALTER TABLE employee_runtime_bindings ADD COLUMN artifacts_path TEXT;

-- Retire the previous long-lived-container design. IDs are deliberately not
-- migrated into the workspace model and are removed from existing records.
UPDATE employee_runtime_bindings
SET container_id = NULL, container_name = NULL, image_ref = NULL,
    workspace_volume = NULL, state_volume = NULL, artifacts_volume = NULL;
UPDATE runtime_runs SET container_id = NULL;
UPDATE runtime_containers
SET engine_container_id = NULL, desired_status = 'ARCHIVED', observed_status = 'RETIRED',
    last_error = 'superseded by host workspace + ephemeral run containers';
UPDATE runtime_profiles
SET tool_policy = '{"workspace":true,"bash":true,"hostEnvironment":true,"softwareSuggestions":true}',
    config = '{"maxIterations":20,"toolTimeoutSeconds":120}'
WHERE id = 'runtime-profile-adk-rust-01';

CREATE TABLE IF NOT EXISTS employee_runtime_workspaces (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL UNIQUE REFERENCES employees(id) ON DELETE CASCADE,
  host_root_path TEXT NOT NULL,
  workspace_path TEXT NOT NULL,
  state_path TEXT NOT NULL,
  artifacts_path TEXT NOT NULL,
  environment_path TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'READY'
    CHECK (status IN ('READY', 'ARCHIVED', 'ERROR')),
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_employee_runtime_workspaces_company
  ON employee_runtime_workspaces(company_id, status, employee_id);

CREATE TABLE IF NOT EXISTS runtime_environment_suggestions (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  runtime_run_id TEXT REFERENCES runtime_runs(id) ON DELETE SET NULL,
  ecosystem TEXT NOT NULL,
  package_name TEXT NOT NULL,
  requested_spec TEXT,
  source_command TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'SUGGESTED'
    CHECK (status IN ('SUGGESTED', 'ACCEPTED', 'PACKAGED', 'REJECTED')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runtime_environment_suggestions_employee
  ON runtime_environment_suggestions(company_id, employee_id, status, created_at DESC);
