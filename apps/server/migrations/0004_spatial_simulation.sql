ALTER TABLE employees ADD COLUMN activity TEXT NOT NULL DEFAULT 'IDLE';
ALTER TABLE employees ADD COLUMN movement TEXT NOT NULL DEFAULT 'STATIONARY';
ALTER TABLE employees ADD COLUMN location_id TEXT;
ALTER TABLE employees ADD COLUMN position_x REAL NOT NULL DEFAULT 0;
ALTER TABLE employees ADD COLUMN position_y REAL NOT NULL DEFAULT 0;
ALTER TABLE employees ADD COLUMN room_id TEXT;
ALTER TABLE employees ADD COLUMN device TEXT NOT NULL DEFAULT 'NONE';
ALTER TABLE employees ADD COLUMN posture TEXT NOT NULL DEFAULT 'SITTING';
ALTER TABLE employees ADD COLUMN presence TEXT NOT NULL DEFAULT 'AVAILABLE';
ALTER TABLE employees ADD COLUMN active_movement_id TEXT;

CREATE TABLE IF NOT EXISTS office_locations (
    id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    room_id TEXT,
    x REAL NOT NULL,
    y REAL NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_office_locations_company ON office_locations(company_id);

CREATE TABLE IF NOT EXISTS agent_movements (
    id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    from_location_id TEXT NOT NULL,
    to_location_id TEXT NOT NULL,
    path TEXT NOT NULL,
    started_at TEXT NOT NULL,
    estimated_arrival_at TEXT NOT NULL,
    movement_status TEXT NOT NULL CHECK (movement_status IN ('PENDING', 'MOVING', 'ARRIVED', 'CANCELLED', 'FAILED')),
    created_at TEXT NOT NULL,
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_agent_movements_active
    ON agent_movements(company_id, movement_status, estimated_arrival_at);
CREATE INDEX IF NOT EXISTS idx_agent_movements_agent
    ON agent_movements(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS location_reservations (
    id TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    location_id TEXT NOT NULL,
    agent_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    reserved_until TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_location_reservations_location
    ON location_reservations(company_id, location_id);

CREATE TABLE IF NOT EXISTS meeting_participants (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    seat_location_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('INVITED', 'ON_THE_WAY', 'ARRIVED', 'WAITING', 'JOINED', 'LEFT')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (meeting_id, agent_id)
);

CREATE INDEX IF NOT EXISTS idx_meeting_participants_agent
    ON meeting_participants(agent_id, updated_at DESC);
