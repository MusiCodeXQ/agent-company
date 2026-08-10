import { FormEvent, useEffect, useMemo, useState } from 'react';
import type { ReactElement } from 'react';
import { OfficeCanvas } from './office/OfficeCanvas';
import { selectedEmployee, useCompanyStore } from './store/company-store';
import type { ActorState, DomainEvent, Employee } from './domain/types';

const STATE_LABELS: Record<ActorState, string> = {
  idle: '待命',
  walking: '移动中',
  checking_phone: '查看手机',
  thinking: '思考中',
  coding: '编码中',
  designing: '设计中',
  testing: '测试中',
  reviewing: 'Review',
  meeting: '会议中',
  interviewing: '面试中',
  waiting: '等待',
  coffee: '茶水间',
  restroom: '卫生间',
  error: '异常',
  offline: '离线',
};

const NAV_ITEMS = [
  ['⌂', '公司'],
  ['◌', '消息'],
  ['◇', '项目'],
  ['＋', '招聘'],
  ['▦', '会议'],
  ['♙', '员工'],
];

export function App(): ReactElement {
  const bootstrap = useCompanyStore((state) => state.bootstrap);
  const employees = useCompanyStore((state) => state.employees);
  const events = useCompanyStore((state) => state.events);
  const selectedEmployeeId = useCompanyStore((state) => state.selectedEmployeeId);
  const loading = useCompanyStore((state) => state.loading);
  const error = useCompanyStore((state) => state.error);
  const connectionStatus = useCompanyStore((state) => state.connectionStatus);
  const loadBootstrap = useCompanyStore((state) => state.loadBootstrap);
  const connectWebSocket = useCompanyStore((state) => state.connectWebSocket);
  const runDemo = useCompanyStore((state) => state.runDemo);
  const sendMessage = useCompanyStore((state) => state.sendMessage);
  const selectEmployee = useCompanyStore((state) => state.selectEmployee);
  const [message, setMessage] = useState('');
  const [demoRunning, setDemoRunning] = useState(false);
  const selected = useMemo(() => selectedEmployee(employees, selectedEmployeeId), [employees, selectedEmployeeId]);

  useEffect(() => {
    let closeSocket: (() => void) | undefined;
    void loadBootstrap().finally(() => {
      closeSocket = connectWebSocket();
    });
    return () => closeSocket?.();
  }, [connectWebSocket, loadBootstrap]);

  async function submitMessage(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const content = message.trim();
    if (!content) return;
    await sendMessage(content);
    setMessage('');
  }

  async function handleDemo(): Promise<void> {
    setDemoRunning(true);
    await runDemo();
    window.setTimeout(() => setDemoRunning(false), 14_000);
  }

  if (loading && !bootstrap) {
    return <div className="loading-screen"><div className="loading-orbit" /><span>正在打开公司办公室…</span></div>;
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <div className="brand-mark">✦</div>
          <div>
            <div className="brand-name">{bootstrap?.company.name ?? 'Virtual AI Company'}</div>
            <div className="brand-subtitle">可观察的多 Agent 工作系统</div>
          </div>
        </div>
        <div className="topbar-center">
          <span className="status-pip online" />
          <span>{employees.length} 名员工在线</span>
          <span className="topbar-divider" />
          <span className="project-chip">MOCK OFFICE / v0.1</span>
        </div>
        <div className="topbar-actions">
          <span className={`connection-pill ${connectionStatus}`}>
            <span className="status-pip" /> {connectionStatus === 'connected' ? '事件流已连接' : '连接中'}
          </span>
          <button className="icon-button" title="Settings" aria-label="Settings">⚙</button>
        </div>
      </header>

      <div className="workspace-grid">
        <aside className="left-rail">
          <div className="rail-section-label">CONTROL ROOM</div>
          <nav className="main-nav">
            {NAV_ITEMS.map(([icon, label], index) => (
              <button className={`nav-item ${index === 0 ? 'active' : ''}`} key={label} type="button">
                <span className="nav-icon">{icon}</span>
                <span>{label}</span>
                {label === '消息' && events.some((event) => event.type === 'message.received') && <span className="nav-badge">1</span>}
              </button>
            ))}
          </nav>

          <div className="console-card">
            <div className="console-heading"><span className="console-dot" /> CEO Console</div>
            <p>对公司下达目标，事件会进入 Agent 工作流。</p>
            <form onSubmit={submitMessage}>
              <textarea
                value={message}
                onChange={(event) => setMessage(event.target.value)}
                placeholder="例如：确认本周 API 任务边界…"
                rows={4}
              />
              <button className="send-button" type="submit">发送给公司 <span>↗</span></button>
            </form>
            <button className="demo-button" type="button" onClick={() => void handleDemo()} disabled={demoRunning}>
              <span className="demo-icon">{demoRunning ? '◌' : '▶'}</span>
              {demoRunning ? 'Mock Event 运行中…' : '运行 Mock Event'}
            </button>
            <div className="console-hint">Backend · 看手机 → 会议室 → 工位编码</div>
          </div>

          <div className="mini-metrics">
            <div><span>MODEL MODE</span><strong>MOCK</strong></div>
            <div><span>EVENTS</span><strong>{events.length.toString().padStart(2, '0')}</strong></div>
          </div>
        </aside>

        <main className="office-column">
          <div className="office-toolbar">
            <div>
              <span className="eyebrow">LIVE FLOOR</span>
              <h1>公司办公室</h1>
            </div>
            <div className="toolbar-legend">
              <span><i className="legend-dot green" />工作中</span>
              <span><i className="legend-dot amber" />事件</span>
              <span><i className="legend-dot blue" />会议</span>
              <span className="zoom-hint">滚轮缩放 · 拖动画布</span>
            </div>
          </div>
          <div className="canvas-shell">
            <OfficeCanvas />
            <div className="canvas-caption"><span>●</span> OFFICE MAP · 1600 × 900 · PROCEDURAL RENDER</div>
          </div>
          <EventTimeline events={events} employees={employees} />
        </main>

        <aside className="right-panel">
          <div className="panel-header">
            <div><span className="eyebrow">EMPLOYEE DIRECTORY</span><h2>员工档案</h2></div>
            <span className="employee-count">{employees.length}/∞</span>
          </div>
          <div className="employee-list">
            {employees.map((employee) => (
              <EmployeeListItem
                employee={employee}
                selected={selected?.id === employee.id}
                key={employee.id}
                onClick={() => selectEmployee(employee.id)}
              />
            ))}
          </div>
          {selected && <EmployeeProfile employee={selected} />}
        </aside>
      </div>
      {error && <div className="toast-error">{error}</div>}
    </div>
  );
}

function EmployeeListItem({ employee, selected, onClick }: { employee: Employee; selected: boolean; onClick: () => void }): ReactElement {
  return (
    <button className={`employee-item ${selected ? 'selected' : ''}`} type="button" onClick={onClick}>
      <span className="employee-avatar" style={{ background: employee.appearance.topColor, color: employee.appearance.skinTone }}>{employee.name.slice(0, 1)}</span>
      <span className="employee-item-copy"><strong>{employee.name}</strong><small>{employee.title}</small></span>
      <span className={`state-dot ${employee.currentState}`} />
    </button>
  );
}

function EmployeeProfile({ employee }: { employee: Employee }): ReactElement {
  const focusEmployee = (): void => { window.dispatchEvent(new CustomEvent('office:focus', { detail: employee.id })); };
  return (
    <div className="profile-card">
      <div className="profile-banner" style={{ background: `linear-gradient(135deg, ${employee.appearance.topColor}, #1e2b3a)` }}>
        <span className="profile-avatar" style={{ background: employee.appearance.skinTone, color: employee.appearance.hairColor }}>{employee.name.slice(0, 1)}</span>
        <div><h3>{employee.name}</h3><span>{employee.title}</span></div>
        <span className={`profile-state ${employee.currentState}`}>{STATE_LABELS[employee.currentState]}</span>
      </div>
      <div className="profile-body">
        <div className="profile-facts"><span>{employee.age} 岁 · {genderLabel(employee.gender)}</span><span>{employee.department}</span></div>
        <div className="profile-section"><div className="section-title">性格特征</div><div className="trait-row">{employee.personality.traits.map((trait) => <span className="trait-chip" key={trait}>{trait}</span>)}</div><p className="profile-quote">“{employee.personality.communicationStyle}”</p></div>
        <div className="profile-section"><div className="section-title">技能档案</div>{employee.skills.slice(0, 4).map((skill) => <div className="skill-row" key={skill.name}><div><span>{skill.name}</span><small>{skill.level}</small></div><div className="skill-track"><span style={{ width: `${skill.level}%` }} /></div></div>)}</div>
        <div className="profile-section current-work"><div className="section-title">当前工作</div><div className="work-status"><span className={`state-dot ${employee.currentState}`} /><strong>{STATE_LABELS[employee.currentState]}</strong><span>· {employee.currentState === 'coding' ? 'task-demo-coding' : employee.currentState === 'meeting' ? '实时通信边界同步' : '暂无任务'}</span></div></div>
        <button className="locate-button" type="button" onClick={focusEmployee}>⌖ 定位到办公室</button>
      </div>
    </div>
  );
}

function EventTimeline({ events, employees }: { events: DomainEvent[]; employees: Employee[] }): ReactElement {
  const recent = events.slice(-7).reverse();
  return (
    <section className="timeline-card">
      <div className="timeline-heading"><div><span className="eyebrow">DOMAIN EVENT STREAM</span><h2>实时活动</h2></div><span className="live-label"><i /> LIVE</span></div>
      <div className="timeline-list">
        {recent.length === 0 && <div className="empty-timeline">等待公司事件… 点击左侧 Mock Event 观察 Backend 工作流。</div>}
        {recent.map((event) => {
          const actorName = employees.find((employee) => employee.id === event.actorId)?.name ?? (event.actorId === 'system' ? '系统' : '公司');
          return <div className="timeline-row" key={event.eventId}><time>{formatTime(event.timestamp)}</time><span className={`event-marker ${event.type.split('.')[0]}`} /><div><strong>{actorName}</strong><span>{eventLabel(event)}</span></div></div>;
        })}
      </div>
    </section>
  );
}

function eventLabel(event: DomainEvent): string {
  const payload = event.payload;
  if (event.type === 'employee.state.changed' && typeof payload.to === 'string') return `状态 → ${STATE_LABELS[payload.to as ActorState] ?? payload.to}`;
  if (event.type === 'employee.destination.changed') return '前往新的工作地点';
  if (event.type === 'message.received' && typeof payload.content === 'string') return payload.content;
  if (event.type === 'meeting.created') return '创建协作会议 · 协作会议室';
  if (event.type === 'meeting.started') return '会议开始';
  if (event.type === 'meeting.finished') return '会议结束并形成决策';
  if (event.type === 'task.state.changed') return '任务进入进行中';
  if (event.type === 'demo.started') return 'Mock Event 已启动';
  if (event.type === 'demo.finished') return '回到工位，电脑已点亮';
  if (event.type === 'message.sent') return 'CEO 发送了一条公司消息';
  return event.type;
}

function formatTime(value: string): string {
  return new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(new Date(value));
}

function genderLabel(gender: Employee['gender']): string {
  return gender === 'male' ? '男' : gender === 'female' ? '女' : '非二元';
}
