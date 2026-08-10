import { FormEvent, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { ReactElement } from 'react';
import { OfficeCanvas } from './office/OfficeCanvas';
import { selectedEmployee, useCompanyStore } from './store/company-store';
import type {
  ActorState,
  AgentJobSummary,
  AgentMemoryRecord,
  DomainEvent,
  Employee,
  MessageRecord,
  ModelProfileSummary,
  TokenUsageSummary,
} from './domain/types';
import { EMPLOYEE_MESSAGE_PAGE_SIZE, type ModelProfileInput } from './api/client';

const STATE_LABELS: Record<ActorState, string> = {
  idle: '待命',
  walking: '移动中',
  checking_phone: '查看手机',
  thinking: '思考中',
  coding: '工作中',
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

type View = 'office' | 'models';

const EMPTY_MEMORIES: AgentMemoryRecord[] = [];
const EMPTY_TOKEN_USAGE: TokenUsageSummary = {
  inputTokens: 0,
  outputTokens: 0,
  cacheTokens: 0,
  totalTokens: 0,
};

const NAV_ITEMS: Array<[string, string, View]> = [
  ['⌂', '公司', 'office'],
  ['◌', '消息', 'office'],
  ['◇', '项目', 'office'],
  ['♙', '员工', 'office'],
  ['◈', '大模型', 'models'],
];

export function App(): ReactElement {
  const bootstrap = useCompanyStore((state) => state.bootstrap);
  const employees = useCompanyStore((state) => state.employees);
  const events = useCompanyStore((state) => state.events);
  const messages = useCompanyStore((state) => state.messages);
  const selectedEmployeeId = useCompanyStore((state) => state.selectedEmployeeId);
  const phoneEmployeeId = useCompanyStore((state) => state.phoneEmployeeId);
  const loading = useCompanyStore((state) => state.loading);
  const error = useCompanyStore((state) => state.error);
  const connectionStatus = useCompanyStore((state) => state.connectionStatus);
  const loadBootstrap = useCompanyStore((state) => state.loadBootstrap);
  const connectWebSocket = useCompanyStore((state) => state.connectWebSocket);
  const sendMessage = useCompanyStore((state) => state.sendMessage);
  const openPhone = useCompanyStore((state) => state.openPhone);
  const closePhone = useCompanyStore((state) => state.closePhone);
  const sendAgentMessage = useCompanyStore((state) => state.sendAgentMessage);
  const startMeeting = useCompanyStore((state) => state.startMeeting);
  const finishMeeting = useCompanyStore((state) => state.finishMeeting);
  const [activeView, setActiveView] = useState<View>('office');
  const [message, setMessage] = useState('');
  const selected = useMemo(() => selectedEmployee(employees, selectedEmployeeId), [employees, selectedEmployeeId]);
  const phoneEmployee = employees.find((employee) => employee.id === phoneEmployeeId) ?? null;
  const activeMeeting = bootstrap?.meetings.find((meeting) => ['WAITING_FOR_PARTICIPANTS', 'IN_PROGRESS'].includes(meeting.status));

  useEffect(() => {
    let closeSocket: (() => void) | undefined;
    let cancelled = false;
    void (async () => {
      await loadBootstrap();
      if (!cancelled && !useCompanyStore.getState().error) closeSocket = connectWebSocket();
    })();
    return () => {
      cancelled = true;
      closeSocket?.();
    };
  }, [connectWebSocket, loadBootstrap]);

  async function submitMessage(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const content = message.trim();
    if (!content) return;
    await sendMessage(content);
    setMessage('');
  }

  if (loading && !bootstrap) {
    return <div className="loading-screen"><div className="loading-orbit" /><span>正在恢复公司运行状态…</span></div>;
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <div className="brand-mark">✦</div>
          <div>
            <div className="brand-name">{bootstrap?.company.name ?? 'Virtual AI Company'}</div>
            <div className="brand-subtitle">持久化、多 Agent 工作运行时</div>
          </div>
        </div>
        <div className="topbar-center">
          <span className="status-pip online" />
          <span>{employees.length} 名员工在线</span>
          <span className="topbar-divider" />
          <span className="project-chip">PERSISTENT RUNTIME / v0.2</span>
        </div>
        <div className="topbar-actions">
          <span className={`connection-pill ${connectionStatus}`}>
            <span className="status-pip" /> {connectionStatus === 'connected' ? '事件流已连接' : connectionStatus === 'disconnected' ? '事件流断开' : '连接中'}
          </span>
          <button className="icon-button" title="大模型" aria-label="大模型" type="button" onClick={() => setActiveView('models')}>⚙</button>
        </div>
      </header>

      <div className="workspace-grid">
        <aside className="left-rail">
          <div className="rail-section-label">CONTROL ROOM</div>
          <nav className="main-nav">
            {NAV_ITEMS.map(([icon, label, view]) => (
              <button
                className={`nav-item ${activeView === view && (view === 'models' || label === '公司') ? 'active' : ''}`}
                key={label}
                type="button"
                onClick={() => setActiveView(view)}
              >
                <span className="nav-icon">{icon}</span>
                <span>{label}</span>
                {label === '消息' && totalUnread(employees) > 0 && <span className="nav-badge">{totalUnread(employees)}</span>}
              </button>
            ))}
          </nav>

          <div className="console-card">
            <div className="console-heading"><span className="console-dot" /> CEO Console</div>
            <p>消息先写入 SQLite，再由 Rust Worker 在后台交给绑定的模型。</p>
            <form onSubmit={submitMessage}>
              <textarea
                value={message}
                onChange={(event) => setMessage(event.target.value)}
                placeholder="例如：请汇总当前运行时状态…"
                rows={4}
              />
              <button className="send-button" type="submit">发送给主管 Agent <span>↗</span></button>
            </form>
            <div className="console-hint">SQLite WAL · Agent Worker · 浏览器关闭后继续运行</div>
          </div>

          <div className="mini-metrics">
            <div><span>RUNTIME</span><strong>WORKER</strong></div>
            <div><span>EVENTS</span><strong>{events.length.toString().padStart(2, '0')}</strong></div>
          </div>
        </aside>

        {activeView === 'models' ? (
          <main className="models-column">
            <ModelProviderPage providers={bootstrap?.modelProfiles ?? []} employees={employees} />
          </main>
        ) : (
          <>
            <main className="office-column">
              <div className="office-toolbar">
                <div>
                  <span className="eyebrow">LIVE FLOOR</span>
                  <h1>公司办公室</h1>
                </div>
                <div className="toolbar-legend">
                  <span><i className="legend-dot green" />工作中</span>
                  <span><i className="legend-dot amber" />Agent 事件</span>
                  <span><i className="legend-dot blue" />会议</span>
                  <span className="zoom-hint">滚轮缩放 · 拖动画布 · 点击人物打开手机</span>
                  <button className="send-button spatial-demo-button" type="button" onClick={() => void startMeeting(['backend-01', 'frontend-01', 'qa-01'])}>召开三人会议</button>
                  {activeMeeting && <button className="ghost-button" type="button" onClick={() => void finishMeeting(activeMeeting.id)}>结束会议并回工位</button>}
                </div>
              </div>
              <div className="runtime-banner"><span className="runtime-pulse" /> Agent Worker 正在后台运行 · 所有消息、Job、状态和事件均持久化</div>
              <div className="canvas-shell">
                <OfficeCanvas />
                <div className="canvas-caption"><span>●</span> HD-2D OFFICE · BACKEND STATE</div>
              </div>
              <EventTimeline events={events} employees={employees} />
            </main>

            <aside className="right-panel">
              <div className="panel-header">
                <div><span className="eyebrow">EMPLOYEE DIRECTORY</span><h2>员工档案</h2></div>
                <span className="employee-count">{employees.length}/∞</span>
              </div>
              <TokenUsageOverview usage={bootstrap?.tokenUsage ?? EMPTY_TOKEN_USAGE} />
              <div className="employee-list">
                {employees.map((employee) => (
                  <EmployeeListItem
                    employee={employee}
                    selected={selected?.id === employee.id}
                    key={employee.id}
                    onClick={() => void openPhone(employee.id)}
                  />
                ))}
              </div>
              {selected && <EmployeeProfile employee={selected} onOpenPhone={() => void openPhone(selected.id)} />}
            </aside>
          </>
        )}
      </div>

      {phoneEmployee && (
        <PhoneModal
          employee={phoneEmployee}
          messages={messages}
          bootstrap={bootstrap}
          onClose={closePhone}
          onSend={sendAgentMessage}
        />
      )}
      {error && <div className="toast-error">{error}</div>}
    </div>
  );
}

function ModelProviderPage({ providers, employees }: { providers: ModelProfileSummary[]; employees: Employee[] }): ReactElement {
  const saveProvider = useCompanyStore((state) => state.saveProvider);
  const testProvider = useCompanyStore((state) => state.testProvider);
  const bindEmployeeModel = useCompanyStore((state) => state.bindEmployeeModel);
  const [editingId, setEditingId] = useState<string | undefined>();
  const [form, setForm] = useState<ModelProfileInput>(emptyProvider());
  const [status, setStatus] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  function startEdit(provider: ModelProfileSummary): void {
    setEditingId(provider.id);
    setStatus(null);
    setForm({
      name: provider.name,
      protocol: provider.protocol,
      baseUrl: provider.baseUrl,
      apiKey: '',
      model: provider.model,
      enabled: provider.enabled,
    });
  }

  function startCreate(): void {
    setEditingId(undefined);
    setStatus(null);
    setForm(emptyProvider());
  }

  async function submit(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    setSaving(true);
    setStatus(null);
    try {
      await saveProvider(form, editingId);
      setStatus('Provider 已保存；API Key 只保存在后端加密字段中。');
      if (!editingId) setForm(emptyProvider());
    } catch {
      // Store already exposes the backend error toast.
    } finally {
      setSaving(false);
    }
  }

  async function test(provider: ModelProfileSummary): Promise<void> {
    setStatus(`正在测试 ${provider.name}…`);
    try {
      setStatus(await testProvider(provider.id));
    } catch {
      setStatus('连接测试失败，请检查后端日志和 Provider 配置。');
    }
  }

  return (
    <section className="models-page">
      <div className="models-heading">
        <div><span className="eyebrow">MODEL CONTROL PLANE</span><h1>大模型</h1><p>配置多个 Provider，Agent 通过 model profile 绑定模型；完整 API Key 永远不会返回到浏览器。</p></div>
        <button className="send-button models-add" type="button" onClick={startCreate}>＋ 新增 Provider</button>
      </div>
      <div className="models-grid">
        <div className="provider-list">
          <div className="section-title">已配置 PROVIDERS</div>
          {providers.length === 0 && <div className="provider-empty">还没有 Provider。</div>}
          {providers.map((provider) => (
            <div className={`provider-card ${editingId === provider.id ? 'selected' : ''}`} key={provider.id}>
              <div className="provider-card-top"><div><strong>{provider.name}</strong><small>{provider.protocol} · {provider.model}</small></div><span className={`enabled-pill ${provider.enabled ? 'on' : ''}`}>{provider.enabled ? '启用' : '停用'}</span></div>
              <div className="provider-url">{provider.baseUrl || '本地 Mock Provider'}</div>
              <div className="provider-key">{provider.apiKeyConfigured ? `API Key ${provider.apiKeyHint ?? '已配置'}` : '未配置 API Key'}</div>
              <div className="provider-actions"><button type="button" onClick={() => startEdit(provider)}>编辑</button><button type="button" onClick={() => void test(provider)}>测试连接</button></div>
            </div>
          ))}
        </div>
        <form className="provider-form" onSubmit={submit}>
          <div className="section-title">{editingId ? '编辑 PROVIDER' : '新增 PROVIDER'}</div>
          <label>名称<input required value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} placeholder="例如：生产 OpenAI" /></label>
          <label>协议<select value={form.protocol} onChange={(event) => setForm({ ...form, protocol: event.target.value })}><option value="openai_compatible">OpenAI Compatible</option><option value="mock">Mock（本地验证）</option></select></label>
          <label>Base URL<input value={form.baseUrl} onChange={(event) => setForm({ ...form, baseUrl: event.target.value })} placeholder="https://api.openai.com/v1" /></label>
          <label>模型<input required value={form.model} onChange={(event) => setForm({ ...form, model: event.target.value })} placeholder="gpt-4o-mini" /></label>
          <label>API Key{editingId && <small className="field-hint">留空以保留已保存 Key</small>}<input type="password" autoComplete="new-password" value={form.apiKey ?? ''} onChange={(event) => setForm({ ...form, apiKey: event.target.value })} placeholder={editingId ? '不回显，留空保持不变' : '只发送到 Rust 后端'} /></label>
          <label className="checkbox-label"><input type="checkbox" checked={form.enabled} onChange={(event) => setForm({ ...form, enabled: event.target.checked })} />启用此 Provider</label>
          <button className="send-button" type="submit" disabled={saving}>{saving ? '保存中…' : '保存 Provider'}</button>
          {status && <div className="provider-status">{status}</div>}
          <p className="security-note">API Key 使用 `MODEL_PROFILE_ENCRYPTION_KEY` 派生的 AES-256-GCM 加密保存。请在运行 Rust 后端的 WSL 环境设置该变量。</p>
          <div className="binding-section"><div className="section-title">AGENT 模型绑定</div><p className="binding-note">Agent 只保存 model_profile_id，不保存 Base URL 或 API Key。</p>{employees.map((employee) => <label className="binding-row" key={employee.id}><span>{employee.name}<small>{employee.title}</small></span><select value={employee.modelProfileId ?? ''} onChange={(event) => void bindEmployeeModel(employee.id, event.target.value || null)}><option value="">未绑定</option>{providers.map((provider) => <option value={provider.id} key={provider.id}>{provider.name} · {provider.model}</option>)}</select></label>)}</div>
        </form>
      </div>
    </section>
  );
}

function PhoneModal({
  employee,
  messages,
  bootstrap,
  onClose,
  onSend,
}: {
  employee: Employee;
  messages: MessageRecord[];
  bootstrap: AppBootstrap;
  onClose: () => void;
  onSend: (employeeId: string, content: string) => Promise<void>;
}): ReactElement {
  const loadMemory = useCompanyStore((state) => state.loadMemory);
  const loadOlderConversation = useCompanyStore((state) => state.loadOlderConversation);
  const memoryRecords = useCompanyStore((state) => state.memories[employee.id]);
  const messageHistory = useCompanyStore((state) => state.messageHistory[employee.id]);
  const memories = memoryRecords ?? EMPTY_MEMORIES;
  const [tab, setTab] = useState<'messages' | 'profile' | 'work' | 'memory'>('messages');
  const [draft, setDraft] = useState('');
  const [sending, setSending] = useState(false);
  const messageListRef = useRef<HTMLDivElement>(null);
  const previousScrollHeight = useRef<number | null>(null);
  const oldestCreatedAt = messageHistory?.oldestCreatedAt;
  const scopedConversation = messages.filter((message) => (
    (message.fromType === 'employee' && message.fromId === employee.id) ||
    (message.fromType === 'user' && message.toId === employee.id)
  ));
  const conversation = oldestCreatedAt
    ? scopedConversation.filter((message) => message.createdAt >= oldestCreatedAt)
    : scopedConversation.slice(-EMPLOYEE_MESSAGE_PAGE_SIZE);
  const task = bootstrap?.tasks.find((item) => item.assigneeId === employee.id);
  const goal = task ? bootstrap?.goals.find((item) => item.id === task.goalId) : undefined;
  const jobs = bootstrap?.jobs.filter((job) => job.employeeId === employee.id).slice(-5).reverse() ?? [];

  useEffect(() => {
    if (tab === 'memory') void loadMemory(employee.id);
  }, [employee.id, loadMemory, tab]);

  useLayoutEffect(() => {
    if (tab !== 'messages') return;
    const list = messageListRef.current;
    if (!list) return;
    if (previousScrollHeight.current !== null) {
      list.scrollTop += list.scrollHeight - previousScrollHeight.current;
      previousScrollHeight.current = null;
    } else {
      list.scrollTop = list.scrollHeight;
    }
  }, [conversation.length, employee.id, messageHistory?.oldestCreatedAt, tab]);

  async function loadOlder(): Promise<void> {
    const list = messageListRef.current;
    if (!list || !messageHistory?.hasMore || messageHistory.loading) return;
    previousScrollHeight.current = list.scrollHeight;
    const loaded = await loadOlderConversation(employee.id);
    if (!loaded) previousScrollHeight.current = null;
  }

  async function submit(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const content = draft.trim();
    if (!content || sending) return;
    setSending(true);
    try {
      await onSend(employee.id, content);
      setDraft('');
    } finally {
      setSending(false);
    }
  }

  return (
    <div className="phone-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section className="phone-shell" role="dialog" aria-modal="true" aria-label={`${employee.name} 的手机`}>
        <div className="phone-speaker" />
        <div className="phone-topbar"><span className="phone-time">09:41</span><strong>{employee.name}</strong><button type="button" onClick={onClose} aria-label="关闭">×</button></div>
        <div className="phone-profile-strip"><span className="phone-avatar" style={{ background: employee.appearance.topColor }}>{employee.name.slice(0, 1)}</span><div><strong>{employee.name}</strong><small>{employee.title} · {STATE_LABELS[employee.currentState]}</small></div><span className={`phone-state ${employee.currentState}`} /></div>
        <div className="phone-tabs">
          {([['messages', '消息'], ['profile', '资料'], ['work', '工作'], ['memory', '记忆']] as const).map(([value, label]) => <button className={tab === value ? 'active' : ''} type="button" key={value} onClick={() => setTab(value)}>{label}{value === 'messages' && employee.unreadCount > 0 && <sup>{employee.unreadCount}</sup>}</button>)}
        </div>
        <div className="phone-content">
          {tab === 'messages' && <div className="phone-messages"><div className="phone-message-list" ref={messageListRef}>
            {messageHistory?.hasMore && <button className="load-older-messages" type="button" onClick={() => void loadOlder()} disabled={messageHistory.loading}>{messageHistory.loading ? '加载中…' : '加载更早消息'}</button>}
            {conversation.length === 0 && <div className="phone-empty">还没有对话，发一条消息开始工作。</div>}
            {conversation.map((message) => <div className={`chat-bubble ${message.fromType === 'user' ? 'outgoing' : 'incoming'}`} key={message.id}><span>{message.content}</span><time>{formatTime(message.createdAt)}</time></div>)}
          </div><form className="phone-compose" onSubmit={submit}><input value={draft} onChange={(event) => setDraft(event.target.value)} placeholder={`发送给 ${employee.name}…`} /><button type="submit" disabled={sending}>{sending ? '…' : '↑'}</button></form></div>}
          {tab === 'profile' && <ProfileTab employee={employee} />}
          {tab === 'work' && <WorkTab employee={employee} goalTitle={goal?.title} task={task?.title} jobs={jobs} />}
          {tab === 'memory' && <MemoryTab memories={memories} />}
        </div>
        <div className="phone-home-indicator" />
      </section>
    </div>
  );
}

type AppBootstrap = ReturnType<typeof useCompanyStore.getState>['bootstrap'];

function ProfileTab({ employee }: { employee: Employee }): ReactElement {
  return <div className="phone-tab-page"><div className="phone-fact-grid"><span>姓名<strong>{employee.name}</strong></span><span>年龄<strong>{employee.age} 岁</strong></span><span>性别<strong>{genderLabel(employee.gender)}</strong></span><span>角色<strong>{employee.role}</strong></span></div><div className="phone-section"><div className="section-title">性格</div><div className="trait-row">{employee.personality.traits.map((trait) => <span className="trait-chip" key={trait}>{trait}</span>)}</div><p>{employee.personality.communicationStyle}</p></div><div className="phone-section"><div className="section-title">技能</div><div className="phone-skill-list">{employee.skills.map((skill) => <span key={skill.name}>{skill.name} <small>{skill.level}</small></span>)}</div></div><div className="phone-section"><div className="section-title">模型</div><p>{employee.modelProfile?.name ?? employee.modelConfig.provider} · {employee.modelProfile?.model ?? employee.modelConfig.model}</p></div></div>;
}

function WorkTab({ employee, goalTitle, task, jobs }: { employee: Employee; goalTitle?: string; task?: string; jobs: AgentJobSummary[] }): ReactElement {
  return <div className="phone-tab-page"><div className="work-card"><span>当前状态</span><strong>{STATE_LABELS[employee.currentState]}</strong><small>{employee.activity} · {employee.movement} · {employee.posture}</small></div><div className="work-card"><span>当前位置</span><strong>{employee.locationId ?? '未定位'}</strong><small>{employee.roomId ?? '未知区域'} · ({Math.round(employee.x)}, {Math.round(employee.y)})</small></div><div className="work-card"><span>Current Goal</span><strong>{goalTitle ?? '暂无目标'}</strong></div><div className="work-card"><span>Current Task</span><strong>{task ?? '暂无任务'}</strong></div><div className="phone-section"><div className="section-title">最近活动</div>{jobs.length === 0 && <p>暂无已执行 Job。</p>}{jobs.map((job) => <div className="job-row" key={job.id}><span>{job.jobType}</span><strong className={`job-${job.status.toLowerCase()}`}>{job.status}</strong><small>{formatTime(job.updatedAt)}</small></div>)}</div></div>;
}

function MemoryTab({ memories }: { memories: Array<{ id: string; kind: string; content: string; updatedAt: string }> }): ReactElement {
  return <div className="phone-tab-page">{memories.length === 0 && <div className="phone-empty">暂无简单记忆记录。</div>}{memories.map((memory) => <article className="memory-card" key={memory.id}><div><span>{memory.kind}</span><time>{formatTime(memory.updatedAt)}</time></div><p>{memory.content}</p></article>)}</div>;
}

function EmployeeListItem({ employee, selected, onClick }: { employee: Employee; selected: boolean; onClick: () => void }): ReactElement {
  const usage = employee.tokenUsage ?? EMPTY_TOKEN_USAGE;
  return <button className={`employee-item ${selected ? 'selected' : ''}`} type="button" onClick={onClick}><span className="employee-avatar" style={{ background: employee.appearance.topColor, color: employee.appearance.skinTone }}>{employee.name.slice(0, 1)}</span><span className="employee-item-copy"><strong>{employee.name}</strong><small>{employee.title}</small><span className="employee-token-breakdown">IN {formatTokens(usage.inputTokens)} · OUT {formatTokens(usage.outputTokens)} · CACHE {formatTokens(usage.cacheTokens)}</span></span><span className="employee-token-total" title={`累计 ${usage.totalTokens.toLocaleString('zh-CN')} tokens`}>{formatTokens(usage.totalTokens)}</span>{employee.unreadCount > 0 && <span className="employee-unread">{employee.unreadCount}</span>}<span className={`state-dot ${employee.currentState}`} /></button>;
}

function TokenUsageOverview({ usage }: { usage: TokenUsageSummary }): ReactElement {
  return <section className="token-overview"><div><span>累计 Token</span><strong title={usage.totalTokens.toLocaleString('zh-CN')}>{formatTokens(usage.totalTokens)}</strong></div><div className="token-overview-breakdown"><span>IN <b>{formatTokens(usage.inputTokens)}</b></span><span>OUT <b>{formatTokens(usage.outputTokens)}</b></span><span>CACHE <b>{formatTokens(usage.cacheTokens)}</b></span></div></section>;
}

function EmployeeProfile({ employee, onOpenPhone }: { employee: Employee; onOpenPhone: () => void }): ReactElement {
  const focusEmployee = (): void => { window.dispatchEvent(new CustomEvent('office:focus', { detail: employee.id })); };
  return <div className="profile-card"><div className="profile-banner" style={{ background: `linear-gradient(135deg, ${employee.appearance.topColor}, #1e2b3a)` }}><span className="profile-avatar" style={{ background: employee.appearance.skinTone, color: employee.appearance.hairColor }}>{employee.name.slice(0, 1)}</span><div><h3>{employee.name}</h3><span>{employee.title}</span></div><span className={`profile-state ${employee.currentState}`}>{STATE_LABELS[employee.currentState]}</span></div><div className="profile-body"><div className="profile-facts"><span>{employee.age} 岁 · {genderLabel(employee.gender)}</span><span>{employee.department}</span></div><div className="profile-section"><div className="section-title">性格特征</div><div className="trait-row">{employee.personality.traits.map((trait) => <span className="trait-chip" key={trait}>{trait}</span>)}</div><p className="profile-quote">“{employee.personality.communicationStyle}”</p></div><div className="profile-section"><div className="section-title">技能档案</div>{employee.skills.slice(0, 4).map((skill) => <div className="skill-row" key={skill.name}><div><span>{skill.name}</span><small>{skill.level}</small></div><div className="skill-track"><span style={{ width: `${skill.level}%` }} /></div></div>)}</div><div className="profile-section current-work"><div className="section-title">当前工作</div><div className="work-status"><span className={`state-dot ${employee.currentState}`} /><strong>{STATE_LABELS[employee.currentState]}</strong><span>· {employee.modelProfile?.model ?? employee.modelConfig.model}</span></div></div><div className="profile-actions"><button className="locate-button" type="button" onClick={focusEmployee}>⌖ 定位</button><button className="phone-button" type="button" onClick={onOpenPhone}>▣ 打开手机</button></div></div></div>;
}

function EventTimeline({ events, employees }: { events: DomainEvent[]; employees: Employee[] }): ReactElement {
  const recent = events.slice(-8).reverse();
  return <section className="timeline-card"><div className="timeline-heading"><div><span className="eyebrow">PERSISTED DOMAIN EVENTS</span><h2>实时活动</h2></div><span className="live-label"><i /> LIVE</span></div><div className="timeline-list">{recent.length === 0 && <div className="empty-timeline">等待 Agent Worker 产生事件…</div>}{recent.map((event) => { const actorName = employees.find((employee) => employee.id === event.actorId)?.name ?? (event.actorId === 'system' ? '系统' : event.actorId === 'user' ? '你' : '公司'); return <div className="timeline-row" key={event.eventId}><time>{formatTime(event.timestamp)}</time><span className={`event-marker ${event.type.split('.')[0]}`} /><div><strong>{actorName}</strong><span>{eventLabel(event)}</span></div></div>; })}</div></section>;
}

function eventLabel(event: DomainEvent): string {
  const payload = event.payload as Record<string, unknown>;
  if (event.type === 'employee.state.changed' && typeof payload.to === 'string') return `状态 → ${STATE_LABELS[normalizeStateForLabel(payload.to)] ?? payload.to}`;
  if (event.type === 'employee.destination.changed') return '前往新的工作地点';
  if ((event.type === 'message.received' || event.type === 'message.sent') && typeof payload.content === 'string') return payload.content;
  if (event.type === 'agent.job.created') return '创建 Agent Job · 等待 Worker';
  if (event.type === 'agent.job.state.changed' && typeof payload.status === 'string') return `Job → ${payload.status}`;
  if (event.type === 'AGENT_MOVE_REQUESTED') return '移动请求已持久化';
  if (event.type === 'AGENT_MOVE_STARTED') return '沿后端路线开始移动';
  if (event.type === 'AGENT_SPATIAL_COMMAND_EXECUTED') return '聊天指令已转换为空间动作';
  if (event.type === 'AGENT_FACE_TO_FACE_MESSAGE') return '到达工位并当面通知同事';
  if (event.type === 'AGENT_SPATIAL_COMMAND_FAILED') return '空间指令执行失败';
  if (event.type === 'AGENT_ARRIVED') return '到达目标位置';
  if (event.type === 'AGENT_SAT_DOWN') return '坐下并等待下一步';
  if (event.type === 'MEETING_CREATED') return '会议创建，参会者开始前往会议室';
  if (event.type === 'MEETING_PARTICIPANT_ON_THE_WAY') return '参会者正在前往会议室';
  if (event.type === 'MEETING_PARTICIPANT_ARRIVED') return '参会者到达并等待全员';
  if (event.type === 'MEETING_STARTED') return '所有参会者到齐，会议开始';
  if (event.type === 'MEETING_FINISHED') return '会议结束，参会者返回工位';
  if (event.type === 'message.read') return '消息已读';
  return event.type;
}

function normalizeStateForLabel(value: string): ActorState {
  const normalized: Record<string, ActorState> = { IDLE: 'idle', WALKING: 'walking', CHECK_PHONE: 'checking_phone', THINKING: 'thinking', WORKING: 'coding', MEETING: 'meeting', WAITING: 'waiting', ERROR: 'error' };
  return normalized[value.toUpperCase()] ?? (value.toLowerCase() as ActorState);
}

function emptyProvider(): ModelProfileInput {
  return { name: '', protocol: 'openai_compatible', baseUrl: '', apiKey: '', model: '', enabled: true };
}

function totalUnread(employees: Employee[]): number {
  return employees.reduce((total, employee) => total + employee.unreadCount, 0);
}

function formatTime(value: string): string {
  return new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(new Date(value));
}

function formatTokens(value: number): string {
  if (value < 1_000) return Math.max(0, value).toLocaleString('zh-CN');
  return new Intl.NumberFormat('zh-CN', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(value);
}

function genderLabel(gender: Employee['gender']): string {
  return gender === 'male' ? '男' : gender === 'female' ? '女' : '非二元';
}
