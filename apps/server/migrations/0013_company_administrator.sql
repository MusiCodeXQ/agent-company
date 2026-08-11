-- A company has one active administrator. Prefer the original company
-- administrator when upgrading databases that created one administrator per floor.
UPDATE employees
SET is_floor_administrator = 0,
    role = '原楼层管理员',
    title = '原管理员',
    employment_status = 'TERMINATED',
    status = 'offline',
    presence = 'OFFLINE',
    current_state = 'IDLE',
    activity = 'IDLE',
    movement = 'STATIONARY',
    device = 'NONE',
    active_movement_id = NULL,
    current_destination = NULL
WHERE is_floor_administrator = 1
  AND employment_status = 'ACTIVE'
  AND EXISTS (
    SELECT 1
    FROM employees AS keeper
    WHERE keeper.company_id = employees.company_id
      AND keeper.is_floor_administrator = 1
      AND keeper.employment_status = 'ACTIVE'
      AND (
        (keeper.id = 'hr-01' AND employees.id <> 'hr-01')
        OR (employees.id <> 'hr-01' AND keeper.id < employees.id)
      )
  );

UPDATE workstations
SET assigned_employee_id = NULL,
    status = 'AVAILABLE',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE assigned_employee_id IN (
  SELECT id FROM employees
  WHERE role = '原楼层管理员' AND employment_status = 'TERMINATED'
);

DELETE FROM role_definitions
WHERE name = '管理员'
  AND NOT EXISTS (
    SELECT 1 FROM employees
    WHERE employees.company_id = role_definitions.company_id
      AND employees.floor_id = role_definitions.floor_id
      AND employees.is_floor_administrator = 1
      AND employees.employment_status = 'ACTIVE'
  );

DROP INDEX IF EXISTS idx_active_floor_administrator;

CREATE UNIQUE INDEX IF NOT EXISTS idx_active_company_administrator
ON employees(company_id)
WHERE is_floor_administrator = 1 AND employment_status = 'ACTIVE';
