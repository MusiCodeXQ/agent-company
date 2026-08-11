ALTER TABLE employees ADD COLUMN is_floor_administrator INTEGER NOT NULL DEFAULT 0;

UPDATE employees
SET role = '楼层管理员', department = '管理员办公室', title = '管理员',
    is_floor_administrator = 1
WHERE id = 'hr-01' AND company_id = 'company-01';

DELETE FROM role_definitions WHERE name = 'HR';
UPDATE role_definitions
SET responsibilities = '["招聘与解雇", "楼层人员与工位管理", "巡查任务与Token消耗", "识别并解除停滞任务"]',
    skills = '["组织管理", "人才评估", "运行巡查", "任务干预"]',
    updated_at = CURRENT_TIMESTAMP
WHERE name = '管理员';

UPDATE office_locations
SET name = REPLACE(REPLACE(name, 'HR', '管理员'), '人事', '管理员')
WHERE name LIKE '%HR%' OR name LIKE '%人事%';

CREATE UNIQUE INDEX IF NOT EXISTS idx_active_floor_administrator
  ON employees(company_id, floor_id)
  WHERE is_floor_administrator = 1 AND employment_status = 'ACTIVE';
