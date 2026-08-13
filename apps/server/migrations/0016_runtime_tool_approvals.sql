CREATE TABLE IF NOT EXISTS runtime_tool_approvals (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  run_id TEXT NOT NULL REFERENCES runtime_runs(id) ON DELETE CASCADE,
  agent_job_id TEXT NOT NULL REFERENCES agent_jobs(id) ON DELETE CASCADE,
  employee_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
  function_call_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  arguments_redacted TEXT NOT NULL DEFAULT '{}',
  arguments_fingerprint TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'PENDING'
    CHECK (status IN ('PENDING', 'APPROVED', 'DENIED', 'EXPIRED')),
  decided_by TEXT,
  decision_reason TEXT,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  decided_at TEXT,
  UNIQUE(run_id, function_call_id)
);

ALTER TABLE runtime_tool_calls ADD COLUMN function_call_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_runtime_tool_calls_function_id
  ON runtime_tool_calls(run_id, function_call_id)
  WHERE function_call_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_runtime_tool_approvals_company
  ON runtime_tool_approvals(company_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_runtime_tool_approvals_job
  ON runtime_tool_approvals(agent_job_id, status);
