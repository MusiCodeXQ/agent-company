CREATE TABLE IF NOT EXISTS floors (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  floor_number INTEGER NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  mission TEXT NOT NULL DEFAULT '',
  responsibilities TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL DEFAULT 'ACTIVE',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(company_id, floor_number)
);

CREATE INDEX IF NOT EXISTS idx_floors_company ON floors(company_id, floor_number);

ALTER TABLE employees ADD COLUMN floor_id TEXT REFERENCES floors(id) ON DELETE SET NULL;
ALTER TABLE office_locations ADD COLUMN floor_id TEXT REFERENCES floors(id) ON DELETE CASCADE;

CREATE TABLE IF NOT EXISTS workstations (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  floor_id TEXT NOT NULL REFERENCES floors(id) ON DELETE CASCADE,
  location_id TEXT NOT NULL UNIQUE,
  room_id TEXT,
  position_x REAL NOT NULL,
  position_y REAL NOT NULL,
  orientation TEXT NOT NULL DEFAULT 'DOWN',
  status TEXT NOT NULL DEFAULT 'AVAILABLE'
    CHECK (status IN ('AVAILABLE', 'OCCUPIED', 'DISABLED')),
  assigned_employee_id TEXT REFERENCES employees(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workstations_floor ON workstations(company_id, floor_id, status);

CREATE TABLE IF NOT EXISTS role_definitions (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  floor_id TEXT REFERENCES floors(id) ON DELETE SET NULL,
  name TEXT NOT NULL,
  responsibilities TEXT NOT NULL DEFAULT '[]',
  skills TEXT NOT NULL DEFAULT '[]',
  preferences TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_role_definitions_company ON role_definitions(company_id, floor_id, name);

CREATE TABLE IF NOT EXISTS hiring_requests (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  floor_id TEXT NOT NULL REFERENCES floors(id) ON DELETE CASCADE,
  requester_id TEXT,
  summary TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'CANDIDATES_READY',
  source_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_hiring_requests_company ON hiring_requests(company_id, created_at DESC);

CREATE TABLE IF NOT EXISTS hiring_positions (
  id TEXT PRIMARY KEY NOT NULL,
  hiring_request_id TEXT NOT NULL REFERENCES hiring_requests(id) ON DELETE CASCADE,
  role_definition_id TEXT REFERENCES role_definitions(id) ON DELETE SET NULL,
  title TEXT NOT NULL,
  headcount INTEGER NOT NULL DEFAULT 1,
  requirements TEXT NOT NULL DEFAULT '[]',
  preferences TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL DEFAULT 'CANDIDATES_READY',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_hiring_positions_request ON hiring_positions(hiring_request_id);

CREATE TABLE IF NOT EXISTS candidates (
  id TEXT PRIMARY KEY NOT NULL,
  company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  hiring_request_id TEXT NOT NULL REFERENCES hiring_requests(id) ON DELETE CASCADE,
  position_id TEXT NOT NULL REFERENCES hiring_positions(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  age INTEGER NOT NULL,
  gender TEXT NOT NULL,
  title TEXT NOT NULL,
  personality TEXT NOT NULL DEFAULT '{}',
  skills TEXT NOT NULL DEFAULT '[]',
  work_style TEXT NOT NULL DEFAULT '',
  strengths TEXT NOT NULL DEFAULT '[]',
  weaknesses TEXT NOT NULL DEFAULT '[]',
  expertise TEXT NOT NULL DEFAULT '[]',
  expected_model TEXT NOT NULL DEFAULT 'mock-office-v1',
  agent_runtime TEXT NOT NULL DEFAULT 'Native Runtime',
  appearance TEXT NOT NULL DEFAULT '{}',
  match_score INTEGER NOT NULL DEFAULT 70,
  interview_evaluation TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'AVAILABLE'
    CHECK (status IN ('AVAILABLE', 'SHORTLISTED', 'REJECTED', 'HIRED')),
  hired_employee_id TEXT REFERENCES employees(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_candidates_request ON candidates(company_id, hiring_request_id, status);
