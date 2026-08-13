import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ReactElement } from 'react';

import { fetchSystemLogs } from './api/client';
import type { ContainerRuntimeStatus, Employee, SystemLogEntry } from './domain/types';

type LevelFilter = 'ALL' | 'ERROR' | 'WARN' | 'INFO';
type SourceFilter = 'ALL' | 'JOB' | 'RUNTIME' | 'RUNTIME_EVENT' | 'MODEL';

const LEVEL_LABELS: Record<LevelFilter, string> = {
  ALL: '全部级别',
  ERROR: '错误',
  WARN: '警告',
  INFO: '信息',
};

const SOURCE_LABELS: Record<SourceFilter, string> = {
  ALL: '全部来源',
  JOB: 'Job',
  RUNTIME: 'Runtime',
  RUNTIME_EVENT: '运行事件',
  MODEL: '模型调用',
};

export function SystemLogsPage({ employees }: { employees: Employee[] }): ReactElement {
  const [entries, setEntries] = useState<SystemLogEntry[]>([]);
  const [containerRuntime, setContainerRuntime] = useState<ContainerRuntimeStatus | null>(null);
  const [level, setLevel] = useState<LevelFilter>('ALL');
  const [source, setSource] = useState<SourceFilter>('ALL');
  const [employeeId, setEmployeeId] = useState('');
  const [exceptionOnly, setExceptionOnly] = useState(false);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeControllerRef = useRef<AbortController | null>(null);
  const requestSequenceRef = useRef(0);
  const inFlightRef = useRef(false);

  const load = useCallback(async (showSpinner = false, force = false): Promise<void> => {
    if (inFlightRef.current && !force) return;
    if (force) activeControllerRef.current?.abort();
    const controller = new AbortController();
    const requestSequence = ++requestSequenceRef.current;
    activeControllerRef.current = controller;
    inFlightRef.current = true;
    if (showSpinner) setRefreshing(true);
    try {
      const response = await fetchSystemLogs({
        limit: exceptionOnly ? 500 : 100,
        level: !exceptionOnly && level !== 'ALL' ? level : undefined,
        source: source !== 'ALL' ? source : undefined,
        employeeId: employeeId || undefined,
        exceptionOnly: exceptionOnly || undefined,
      }, controller.signal);
      if (controller.signal.aborted || requestSequence !== requestSequenceRef.current) return;
      setEntries(response.entries);
      setContainerRuntime(response.containerRuntime);
      setError(null);
    } catch (requestError) {
      if (controller.signal.aborted || requestSequence !== requestSequenceRef.current) return;
      if (requestError instanceof Error && requestError.name === 'AbortError') return;
      setError(requestError instanceof Error ? requestError.message : '系统日志请求失败');
    } finally {
      if (requestSequence === requestSequenceRef.current) {
        inFlightRef.current = false;
        activeControllerRef.current = null;
        setLoading(false);
        setRefreshing(false);
      }
    }
  }, [employeeId, exceptionOnly, level, source]);

  useEffect(() => {
    setLoading(true);
    void load().catch(() => undefined);
    const timer = window.setInterval(() => {
      void load().catch(() => undefined);
    }, 5000);
    return () => {
      requestSequenceRef.current += 1;
      activeControllerRef.current?.abort();
      activeControllerRef.current = null;
      inFlightRef.current = false;
      window.clearInterval(timer);
    };
  }, [load]);

  const visibleEntries = entries;
  const employeeById = useMemo(() => new Map(employees.map((employee) => [employee.id, employee])), [employees]);

  function toggleExceptionOnly(): void {
    setExceptionOnly((enabled) => !enabled);
  }

  return (
    <main className="system-logs-column">
      <div className="system-logs-page">
        <header className="system-logs-heading">
          <div>
            <span className="eyebrow">PERSISTENT SYSTEM LOG</span>
            <h1>系统运行日志</h1>
            <p>聚合 Job、运行时事件和模型调用，帮助定位 Agent 失败原因。日志来自 SQLite 持久化记录。</p>
          </div>
          <button className="send-button system-logs-refresh" type="button" onClick={() => void load(true, true)} disabled={refreshing}>
            {refreshing ? '刷新中…' : '↻ 刷新'}
          </button>
        </header>

        <ContainerRuntimeCard status={containerRuntime} />

        <section className="system-logs-toolbar" aria-label="日志筛选">
          <label>级别<select value={level} onChange={(event) => setLevel(event.target.value as LevelFilter)}>{Object.entries(LEVEL_LABELS).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
          <label>来源<select value={source} onChange={(event) => setSource(event.target.value as SourceFilter)}>{Object.entries(SOURCE_LABELS).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
          <label>员工<select value={employeeId} onChange={(event) => setEmployeeId(event.target.value)}><option value="">全部员工</option>{employees.map((employee) => <option key={employee.id} value={employee.id}>{employee.name} · {employee.title}</option>)}</select></label>
          <button className={`ghost-button logs-exception-toggle ${exceptionOnly ? 'active' : ''}`} type="button" onClick={toggleExceptionOnly} aria-pressed={exceptionOnly}>
            {exceptionOnly ? '显示全部日志' : '仅看异常'}
          </button>
        </section>

        {error && <div className="system-logs-error" role="alert">{error}</div>}
        {loading && visibleEntries.length === 0 && <div className="system-logs-state">正在加载持久化日志…</div>}
        {!loading && !error && visibleEntries.length === 0 && <div className="system-logs-state">暂无符合条件的日志。</div>}
        <section className="system-log-list" aria-live="polite">
          {visibleEntries.map((entry) => <SystemLogRow key={entry.id} entry={entry} employeeById={employeeById} />)}
        </section>
      </div>
    </main>
  );
}

function ContainerRuntimeCard({ status }: { status: ContainerRuntimeStatus | null }): ReactElement {
  return (
    <section className="container-runtime-card">
      <div className="container-runtime-heading">
        <div><span className="section-title">CONTAINER RUNTIME</span><h2>员工执行容器</h2></div>
        <span className={`container-runtime-status ${status?.available ? 'available' : status?.enabled ? 'unavailable' : 'disabled'}`}>
          {status ? status.enabled ? status.available ? '已启用 · 可用' : '已启用 · 不可用' : '未启用' : '检查中…'}
        </span>
      </div>
      {status && <div className="container-runtime-facts"><span>镜像<strong>{status.defaultImage}</strong></span><span>引擎<strong>{status.engineVersion ?? '未连接'}</strong></span></div>}
      {status?.lastError && <pre className="container-runtime-error">{status.lastError}</pre>}
      {!status?.lastError && status?.enabled && !status.available && <p className="container-runtime-muted">Docker 已启用，但当前无法访问 daemon。请检查 Docker 服务和网络。</p>}
      {!status?.enabled && status && <p className="container-runtime-muted">Docker 执行路径未启用；ADK Rust 任务不会启动一次性容器。</p>}
    </section>
  );
}

function SystemLogRow({ entry, employeeById }: { entry: SystemLogEntry; employeeById: Map<string, Employee> }): ReactElement {
  const employee = entry.employeeId ? employeeById.get(entry.employeeId) : undefined;
  const employeeLabel = entry.employeeName ?? employee?.name ?? entry.employeeId ?? '系统';
  const relatedIds = [
    entry.jobId ? `Job ${entry.jobId}` : null,
    entry.runtimeRunId ? `Run ${entry.runtimeRunId}` : null,
  ].filter((value): value is string => Boolean(value));
  return (
    <article className={`system-log-row level-${entry.level.toLowerCase()}`}>
      <div className="system-log-meta">
        <time dateTime={entry.timestamp}>{formatDateTime(entry.timestamp)}</time>
        <span className="system-log-level">{levelLabel(entry.level)}</span>
        <span className="system-log-source">{sourceLabel(entry.source)}</span>
        <span className="system-log-employee">{employeeLabel}</span>
        {relatedIds.map((id) => <code key={id} title={id}>{id}</code>)}
      </div>
      <strong className="system-log-message">{entry.message}</strong>
      {entry.details && <details className="system-log-details" open={entry.level === 'ERROR'}><summary>查看详情</summary><pre>{entry.details}</pre></details>}
    </article>
  );
}

function levelLabel(level: string): string {
  return level === 'ERROR' ? '错误' : level === 'WARN' ? '警告' : '信息';
}

function sourceLabel(source: string): string {
  return SOURCE_LABELS[source as SourceFilter] ?? source;
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(date);
}
