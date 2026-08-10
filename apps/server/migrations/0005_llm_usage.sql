CREATE TABLE IF NOT EXISTS llm_calls (
    id TEXT PRIMARY KEY NOT NULL,
    company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    employee_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    job_id TEXT REFERENCES agent_jobs(id) ON DELETE SET NULL,
    model_profile_id TEXT REFERENCES model_profiles(id) ON DELETE SET NULL,
    model TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('COMPLETED', 'FAILED')),
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_llm_calls_company_created
    ON llm_calls(company_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_llm_calls_employee_created
    ON llm_calls(company_id, employee_id, created_at DESC);
