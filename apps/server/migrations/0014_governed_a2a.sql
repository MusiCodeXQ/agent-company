CREATE TABLE IF NOT EXISTS a2a_face_to_face_sessions (
    id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    context_id TEXT NOT NULL UNIQUE,
    initiator_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    recipient_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    initiator_location_id TEXT NOT NULL,
    recipient_location_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('GATHERING', 'ACTIVE', 'CLOSED', 'EXPIRED')),
    capability_token_hash TEXT NOT NULL,
    max_turns INTEGER NOT NULL DEFAULT 8 CHECK (max_turns > 0),
    turns_used INTEGER NOT NULL DEFAULT 0 CHECK (turns_used >= 0),
    token_budget INTEGER NOT NULL DEFAULT 8000 CHECK (token_budget > 0),
    tokens_used INTEGER NOT NULL DEFAULT 0 CHECK (tokens_used >= 0),
    company_task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    activated_at TEXT,
    closed_at TEXT,
    close_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_a2a_sessions_company_status
    ON a2a_face_to_face_sessions(company_id, status, expires_at);
CREATE INDEX IF NOT EXISTS idx_a2a_sessions_participants
    ON a2a_face_to_face_sessions(company_id, initiator_id, recipient_id, created_at DESC);

CREATE TABLE IF NOT EXISTS a2a_tasks (
    id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    context_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES a2a_face_to_face_sessions(id) ON DELETE CASCADE,
    sender_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    recipient_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    company_task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    agent_job_id TEXT REFERENCES agent_jobs(id) ON DELETE SET NULL,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_a2a_tasks_context
    ON a2a_tasks(company_id, context_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS a2a_messages (
    id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES a2a_face_to_face_sessions(id) ON DELETE CASCADE,
    context_id TEXT NOT NULL,
    a2a_task_id TEXT REFERENCES a2a_tasks(id) ON DELETE SET NULL,
    company_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
    sender_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    recipient_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    parts TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    direction TEXT NOT NULL CHECK (direction IN ('REQUEST', 'RESPONSE')),
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_a2a_messages_context
    ON a2a_messages(company_id, context_id, created_at);

CREATE TABLE IF NOT EXISTS a2a_audit_log (
    id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    session_id TEXT,
    task_id TEXT,
    actor_id TEXT,
    action TEXT NOT NULL,
    outcome TEXT NOT NULL,
    details TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_a2a_audit_company_created
    ON a2a_audit_log(company_id, created_at DESC);

