import { FormEvent, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { ReactElement } from 'react';
import { OfficeCanvas } from './office/OfficeCanvas';
import { selectedEmployee, useCompanyStore } from './store/company-store';
import { formatTokens } from './domain/format';
import type {
  ActorState,
  AgentJobSummary,
  AgentMemoryRecord,
  CandidateSummary,
  ContainerRuntimeStatus,
  DomainEvent,
  Employee,
  FloorSummary,
  HiringRequestSummary,
  MeetingSummary,
  MessageRecord,
  ModelProfileSummary,
  OpenClawStatus,
  RuntimeEnvironmentSuggestionSummary,
  TokenUsageSummary,
  TerminationPreview,
} from './domain/types';
import { EMPLOYEE_MESSAGE_PAGE_SIZE, type ModelProfileInput, type TerminationInput } from './api/client';

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
type DirectoryTab = 'active' | 'former' | 'hiring';

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
  const activeFloorId = useCompanyStore((state) => state.activeFloorId);
  const loadBootstrap = useCompanyStore((state) => state.loadBootstrap);
  const connectWebSocket = useCompanyStore((state) => state.connectWebSocket);
  const sendMessage = useCompanyStore((state) => state.sendMessage);
  const openPhone = useCompanyStore((state) => state.openPhone);
  const closePhone = useCompanyStore((state) => state.closePhone);
  const sendAgentMessage = useCompanyStore((state) => state.sendAgentMessage);
  const sendMeetingMessage = useCompanyStore((state) => state.sendMeetingMessage);
  const startMeeting = useCompanyStore((state) => state.startMeeting);
  const finishMeeting = useCompanyStore((state) => state.finishMeeting);
  const loadFloor = useCompanyStore((state) => state.loadFloor);
  const createFloor = useCompanyStore((state) => state.createFloor);
  const updateFloor = useCompanyStore((state) => state.updateFloor);
  const deleteFloor = useCompanyStore((state) => state.deleteFloor);
  const createWorkstation = useCompanyStore((state) => state.createWorkstation);
  const hireCandidate = useCompanyStore((state) => state.hireCandidate);
  const selectEmployee = useCompanyStore((state) => state.selectEmployee);
  const refreshHiringCandidates = useCompanyStore((state) => state.refreshHiringCandidates);
  const terminationPreview = useCompanyStore((state) => state.terminationPreview);
  const loadTerminationPreview = useCompanyStore((state) => state.loadTerminationPreview);
  const clearTerminationPreview = useCompanyStore((state) => state.clearTerminationPreview);
  const terminateEmployee = useCompanyStore((state) => state.terminateEmployee);
  const cancelEmployeeTermination = useCompanyStore((state) => state.cancelEmployeeTermination);
  const [activeView, setActiveView] = useState<View>('office');
  const [message, setMessage] = useState('');
  const [meetingMessageOpen, setMeetingMessageOpen] = useState(false);
  const [floorModalOpen, setFloorModalOpen] = useState(false);
  const [directoryTab, setDirectoryTab] = useState<DirectoryTab>('active');
  const phoneEmployee = employees.find((employee) => employee.id === phoneEmployeeId) ?? null;
  const activeMeeting = bootstrap?.meetings.find((meeting) => ['WAITING_FOR_PARTICIPANTS', 'IN_PROGRESS'].includes(meeting.status));
  const activeFloor = bootstrap?.floors.find((floor) => floor.id === activeFloorId) ?? bootstrap?.floors[0];
  const floorEmployees = employees.filter((employee) => employee.floorId === activeFloor?.id);
  const activeEmployees = floorEmployees.filter((employee) => employee.employmentStatus === 'ACTIVE');
  const formerEmployees = floorEmployees.filter((employee) => employee.employmentStatus !== 'ACTIVE');
  const selected = useMemo(
    () => selectedEmployee(floorEmployees, selectedEmployeeId),
    [floorEmployees, selectedEmployeeId],
  );
  const visibleEmployees = directoryTab === 'active' ? activeEmployees : formerEmployees;
  const availableCandidateCount = bootstrap?.hiringRequests
    .filter((request) => request.floorId === activeFloor?.id)
    .flatMap((request) => request.candidates)
    .filter((candidate) => candidate.status === 'AVAILABLE').length ?? 0;
  const meetingCandidates = employees
    .filter((employee) => employee.floorId === activeFloor?.id && employee.employmentStatus === 'ACTIVE')
    .slice(0, 3)
    .map((employee) => employee.id);

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
          <span>{employees.filter((employee) => employee.employmentStatus === 'ACTIVE').length} 名员工在线</span>
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
            <ModelProviderPage providers={bootstrap?.modelProfiles ?? []} employees={employees} openclaw={bootstrap?.openclaw} containerRuntime={bootstrap?.containerRuntime} environmentSuggestions={bootstrap?.environmentSuggestions ?? []} />
          </main>
        ) : (
          <>
            <main className="office-column">
              <div className="office-toolbar">
                <div>
                  <span className="eyebrow">LIVE FLOOR</span>
                  <h1>{activeFloor?.name ?? '公司办公室'}</h1>
                </div>
                <div className="toolbar-legend">
                  <span><i className="legend-dot green" />工作中</span>
                  <span><i className="legend-dot amber" />Agent 事件</span>
                  <span><i className="legend-dot blue" />会议</span>
                  <span className="zoom-hint">滚轮缩放 · 拖动画布 · 单击选中 · 双击打开手机</span>
                  <FloorSwitcher floors={bootstrap?.floors ?? []} activeFloorId={activeFloor?.id} onSelect={(floorId) => void loadFloor(floorId)} onCreate={() => setFloorModalOpen(true)} />
                  <button className="send-button spatial-demo-button" type="button" disabled={meetingCandidates.length < 2} onClick={() => void startMeeting(meetingCandidates)}>召开本层会议</button>
                  {activeMeeting && <button className="ghost-button" type="button" onClick={() => void finishMeeting(activeMeeting.id)}>结束会议并回工位</button>}
                </div>
              </div>
              <div className="runtime-banner"><span className="runtime-pulse" /> Agent Worker 正在后台运行 · 所有消息、Job、状态和事件均持久化</div>
              {activeFloor && <FloorMissionCard floor={activeFloor} canDelete={activeFloor.id !== 'floor-01'} onSave={(name, mission) => void updateFloor(activeFloor.id, { name, mission })} onDelete={() => { if (window.confirm(`确定删除楼层“${activeFloor.name}”吗？`)) void deleteFloor(activeFloor.id); }} />}
              <div className="canvas-shell">
                <OfficeCanvas onMeetingTableClick={() => setMeetingMessageOpen(true)} />
                <div className="canvas-caption"><span>●</span> HD-2D OFFICE · BACKEND STATE</div>
              </div>
              <EventTimeline events={events} employees={employees} />
            </main>

            <aside className="right-panel">
              <div className="panel-header">
                <div><span className="eyebrow">EMPLOYEE DIRECTORY</span><h2>员工档案</h2></div>
                <span className="employee-count">{floorEmployees.length}/∞</span>
              </div>
              <TokenUsageOverview usage={bootstrap?.tokenUsage ?? EMPTY_TOKEN_USAGE} />
              <div className="directory-tabs" role="tablist" aria-label="员工档案分类">
                <button type="button" className={directoryTab === 'active' ? 'active' : ''} onClick={() => setDirectoryTab('active')}>在职 <span>{activeEmployees.length}</span></button>
                <button type="button" className={directoryTab === 'former' ? 'active' : ''} onClick={() => setDirectoryTab('former')}>离职 <span>{formerEmployees.length}</span></button>
                <button type="button" className={directoryTab === 'hiring' ? 'active' : ''} onClick={() => setDirectoryTab('hiring')}>招聘 <span>{availableCandidateCount}</span></button>
              </div>
              {directoryTab !== 'hiring' && <div className="employee-list">
                {visibleEmployees.map((employee) => (
                  <EmployeeListItem
                    employee={employee}
                    selected={selected?.id === employee.id}
                    key={employee.id}
                    onClick={() => selectEmployee(employee.id)}
                    onDoubleClick={() => void openPhone(employee.id)}
                  />
                ))}
                {visibleEmployees.length === 0 && <p className="directory-empty">当前分类暂无员工。</p>}
              </div>}
              {directoryTab === 'hiring' && bootstrap && <HiringPanel activeFloorId={activeFloor?.id} requests={bootstrap.hiringRequests} workstations={bootstrap.workstations} onHire={hireCandidate} onRefresh={refreshHiringCandidates} onAddWorkstation={async () => { if (activeFloor) await createWorkstation(activeFloor.id); }} />}
              {directoryTab !== 'hiring' && selected && <EmployeeProfile employee={selected} onOpenPhone={() => void openPhone(selected.id)} onRequestTermination={() => void loadTerminationPreview(selected.id)} onCancelTermination={() => void cancelEmployeeTermination(selected.id)} />}
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
      {meetingMessageOpen && (
        <MeetingMessageModal
          meeting={activeMeeting ?? null}
          employees={employees}
          messages={messages}
          onClose={() => setMeetingMessageOpen(false)}
          onSend={sendMeetingMessage}
        />
      )}
      {floorModalOpen && <CreateFloorModal onClose={() => setFloorModalOpen(false)} onCreate={async (name, mission) => { await createFloor({ name, mission }); setFloorModalOpen(false); }} />}
      {terminationPreview && <TerminationModal preview={terminationPreview} onClose={clearTerminationPreview} onConfirm={async (input) => { await terminateEmployee(terminationPreview.employee.id, input); }} />}
      {error && <div className="toast-error">{error}</div>}
    </div>
  );
}

function FloorSwitcher({
  floors,
  activeFloorId,
  onSelect,
  onCreate,
}: {
  floors: FloorSummary[];
  activeFloorId?: string;
  onSelect: (floorId: string) => void;
  onCreate: () => void;
}): ReactElement {
  return <div className="floor-switcher"><select aria-label="切换楼层" value={activeFloorId ?? ''} onChange={(event) => onSelect(event.target.value)}>{floors.map((floor) => <option value={floor.id} key={floor.id}>F{floor.floorNumber} · {floor.name}</option>)}</select><button className="ghost-button" type="button" onClick={onCreate}>＋ 新建楼层</button></div>;
}

function FloorMissionCard({ floor, canDelete, onSave, onDelete }: { floor: FloorSummary; canDelete: boolean; onSave: (name: string, mission: string) => void; onDelete: () => void }): ReactElement {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(floor.name);
  const [mission, setMission] = useState(floor.mission);
  useEffect(() => {
    setName(floor.name);
    setMission(floor.mission);
    setEditing(false);
  }, [floor.id, floor.mission]);
  return <section className="floor-mission-card"><div><span className="eyebrow">FLOOR MISSION · F{floor.floorNumber}</span>{editing ? <input value={name} onChange={(event) => setName(event.target.value)} aria-label="楼层名称" /> : <strong>{floor.name}</strong>}<p>{floor.mission || '还没有定义职责。可以直接告诉本层管理员，或在这里补充这一层的使命。'}</p><div className="floor-role-chips">{floor.roles.map((role) => <span key={role.id}>{role.name}</span>)}</div></div><div className="floor-actions"><button className="ghost-button" type="button" onClick={() => { if (editing) onSave(name.trim(), mission.trim()); setEditing(!editing); }}>{editing ? '保存' : '编辑楼层'}</button>{canDelete && <button className="danger-button" type="button" onClick={onDelete}>删除</button>}</div>{editing && <textarea value={mission} onChange={(event) => setMission(event.target.value)} rows={2} placeholder="例如：负责科技类内容的选题、研究、创作与审核。" />}</section>;
}

function CreateFloorModal({ onClose, onCreate }: { onClose: () => void; onCreate: (name: string, mission?: string) => Promise<void> }): ReactElement {
  const [name, setName] = useState('');
  const [mission, setMission] = useState('');
  const [saving, setSaving] = useState(false);
  async function submit(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!name.trim() || saving) return;
    setSaving(true);
    try {
      await onCreate(name.trim(), mission.trim() || undefined);
    } finally {
      setSaving(false);
    }
  }
  return <div className="simple-modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}><form className="simple-modal" onSubmit={submit}><div className="modal-heading"><div><span className="eyebrow">CREATE FLOOR</span><h2>新建楼层</h2></div><button type="button" onClick={onClose} aria-label="关闭">×</button></div><label>楼层名称<input autoFocus required value={name} onChange={(event) => setName(event.target.value)} placeholder="例如：自媒体中心" /></label><label>初始使命 <small>可稍后编辑</small><textarea value={mission} onChange={(event) => setMission(event.target.value)} rows={3} placeholder="例如：负责科技类内容的选题、研究、创作与审核。" /></label><p className="modal-hint">系统自动分配下一个 F 编号并初始化办公室地图。公司唯一管理员负责所有楼层的招聘、解雇与运行巡查。</p><div className="modal-actions"><button className="ghost-button" type="button" onClick={onClose}>取消</button><button className="send-button" type="submit" disabled={saving}>{saving ? '创建中…' : '创建楼层'}</button></div></form></div>;
}

function HiringPanel({
  activeFloorId,
  requests,
  workstations,
  onHire,
  onRefresh,
  onAddWorkstation,
}: {
  activeFloorId?: string;
  requests: HiringRequestSummary[];
  workstations: Array<{ id: string; status: string }>;
  onHire: (requestId: string, candidateId: string, workstationId?: string) => Promise<unknown>;
  onRefresh: (requestId: string) => Promise<void>;
  onAddWorkstation: () => Promise<void>;
}): ReactElement {
  const [busyCandidateId, setBusyCandidateId] = useState<string | null>(null);
  const [refreshingRequestId, setRefreshingRequestId] = useState<string | null>(null);
  const available = workstations.filter((workstation) => workstation.status === 'AVAILABLE');
  const activeRequests = requests.filter((request) => request.floorId === activeFloorId && request.candidates.some((candidate) => candidate.status === 'AVAILABLE'));
  if (activeRequests.length === 0) return <section className="hiring-panel"><div className="section-title">招聘 PIPELINE</div><p className="panel-muted">和本层管理员聊天提出招聘需求，三名候选人会出现在这里。候选人不会自动入职。</p></section>;
  async function hire(requestId: string, candidate: CandidateSummary): Promise<void> {
    if (busyCandidateId) return;
    setBusyCandidateId(candidate.id);
    try {
      if (!available[0]) await onAddWorkstation();
      await onHire(requestId, candidate.id, available[0]?.id);
    } finally {
      setBusyCandidateId(null);
    }
  }
  async function refresh(requestId: string): Promise<void> {
    if (refreshingRequestId || busyCandidateId) return;
    setRefreshingRequestId(requestId);
    try {
      await onRefresh(requestId);
    } finally {
      setRefreshingRequestId(null);
    }
  }
  return <section className="hiring-panel"><div className="hiring-panel-heading"><div><div className="section-title">招聘 PIPELINE</div><p className="panel-muted">选择候选人后才会创建 Employee，并从公司入口开始入职移动。</p></div>{available.length === 0 && <button className="ghost-button" type="button" onClick={() => void onAddWorkstation()}>＋ 新增工位</button>}</div>{activeRequests.slice(0, 3).map((request) => <div className="hiring-request" key={request.id}><div className="hiring-request-title"><strong>{request.summary}</strong><div><span>{request.status}</span><button className="ghost-button" type="button" disabled={refreshingRequestId !== null || busyCandidateId !== null} onClick={() => void refresh(request.id)}>{refreshingRequestId === request.id ? '生成中…' : '换一批候选人'}</button></div></div>{request.positions.map((position) => <div className="position-group" key={position.id}><div className="position-label">{position.title} · 需要 {position.headcount} 人</div><div className="candidate-list">{request.candidates.filter((candidate) => candidate.positionId === position.id && candidate.status === 'AVAILABLE').map((candidate) => <CandidateCard candidate={candidate} key={candidate.id} busy={busyCandidateId !== null || refreshingRequestId !== null} needsWorkstation={!available[0]} onHire={() => void hire(request.id, candidate)} />)}</div></div>)}</div>)}</section>;
}

function CandidateCard({ candidate, busy, needsWorkstation, onHire }: { candidate: CandidateSummary; busy: boolean; needsWorkstation: boolean; onHire: () => void }): ReactElement {
  return <article className="candidate-card"><div className="candidate-card-heading"><div className="candidate-avatar" style={{ background: candidate.appearance.topColor }}>{candidate.name.slice(0, 1)}</div><div><strong>{candidate.name}</strong><span>{candidate.title} · {candidate.age} 岁</span></div><b>{candidate.matchScore}%</b></div><div className="candidate-chips">{candidate.personality.traits.slice(0, 3).map((trait) => <span key={trait}>{trait}</span>)}</div><p>{candidate.strengths.slice(0, 2).join('、')}</p><small>技能：{candidate.skills.slice(0, 3).map((skill) => `${skill.name} ${skill.level}`).join(' · ')}</small><button className="send-button" type="button" disabled={busy} onClick={onHire}>{busy ? '处理中…' : needsWorkstation ? '创建工位并录用' : '录用并开始入职'}</button></article>;
}

function TerminationModal({ preview, onClose, onConfirm }: { preview: TerminationPreview; onClose: () => void; onConfirm: (input: TerminationInput) => Promise<void> }): ReactElement {
  const [taskAction, setTaskAction] = useState<TerminationInput['taskAction']>('REASSIGN');
  const [workspaceAction, setWorkspaceAction] = useState<TerminationInput['workspaceAction']>('ARCHIVE');
  const [reason, setReason] = useState('岗位或组织调整');
  const [notes, setNotes] = useState('保留历史记录，完成任务交接。');
  const [confirming, setConfirming] = useState(false);
  const [saving, setSaving] = useState(false);

  async function submit(): Promise<void> {
    if (!confirming) {
      setConfirming(true);
      return;
    }
    setSaving(true);
    try {
      await onConfirm({ reason, notes, taskAction, workspaceAction });
    } finally {
      setSaving(false);
    }
  }

  return <div className="termination-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}><section className="termination-modal" role="dialog" aria-modal="true" aria-label="员工离职预览"><div className="termination-heading"><div><span className="eyebrow">OFFBOARDING PREVIEW</span><h2>解雇 {preview.employee.name}</h2></div><button type="button" onClick={onClose} aria-label="关闭">×</button></div><p className="termination-warning">这不会删除员工。聊天、会议、Job、任务和工作记录会继续保留；员工会先完成交接、停止 Runtime，再沿办公室路线离开。</p><div className="termination-metrics"><span>未完成任务<strong>{preview.activeTaskCount}</strong></span><span>运行中 Job<strong>{preview.runningJobCount}</strong></span><span>关联 Goal<strong>{preview.activeGoalCount}</strong></span><span>进行中会议<strong>{preview.activeMeetingCount}</strong></span><span>Workspace<strong>{preview.workspaceStatus}</strong></span><span>Runtime<strong>{preview.runtimeType} / {preview.runtimeStatus}</strong></span></div>{preview.tasks.length > 0 && <div className="termination-task-list"><div className="section-title">任务交接对象</div>{preview.tasks.slice(0, 6).map((task) => <div key={task.id}><span>{task.title}</span><small>{task.status}</small></div>)}</div>}<div className="termination-form"><label>任务处理<select value={taskAction} onChange={(event) => setTaskAction(event.target.value as TerminationInput['taskAction'])}><option value="REASSIGN">标记为待重新分配</option><option value="PAUSE">先暂停</option><option value="CANCEL">取消任务</option></select></label><label>Workspace<select value={workspaceAction} onChange={(event) => setWorkspaceAction(event.target.value as TerminationInput['workspaceAction'])}><option value="ARCHIVE">归档保留</option><option value="READ_ONLY">保留为只读</option></select></label><label>原因<input value={reason} onChange={(event) => setReason(event.target.value)} /></label><label>备注<textarea value={notes} onChange={(event) => setNotes(event.target.value)} rows={2} /></label></div><div className="termination-actions"><button className="ghost-button" type="button" onClick={onClose}>取消</button><button className="danger-button" type="button" onClick={() => void submit()} disabled={saving}>{saving ? '处理中…' : confirming ? '再次确认并开始离职' : '查看影响后继续'}</button></div></section></div>;
}

function ModelProviderPage({ providers, employees, openclaw, containerRuntime, environmentSuggestions }: { providers: ModelProfileSummary[]; employees: Employee[]; openclaw?: OpenClawStatus; containerRuntime?: ContainerRuntimeStatus; environmentSuggestions: RuntimeEnvironmentSuggestionSummary[] }): ReactElement {
  const saveProvider = useCompanyStore((state) => state.saveProvider);
  const testProvider = useCompanyStore((state) => state.testProvider);
  const bindEmployeeModel = useCompanyStore((state) => state.bindEmployeeModel);
  const testOpenClaw = useCompanyStore((state) => state.testOpenClaw);
  const provisionEmployeeRuntime = useCompanyStore((state) => state.provisionEmployeeRuntime);
  const provisionEmployeeWorkspace = useCompanyStore((state) => state.provisionEmployeeWorkspace);
  const [editingId, setEditingId] = useState<string | undefined>();
  const [form, setForm] = useState<ModelProfileInput>(emptyProvider());
  const [status, setStatus] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [openclawBusy, setOpenclawBusy] = useState(false);

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

  async function checkOpenClaw(): Promise<void> {
    setOpenclawBusy(true);
    try {
      const result = await testOpenClaw();
      setStatus(result.reachable ? 'OpenClaw Gateway 连接成功。' : result.lastError ?? 'OpenClaw Gateway 暂不可达。');
    } catch {
      setStatus('OpenClaw Gateway 检查失败。');
    } finally {
      setOpenclawBusy(false);
    }
  }

  async function provision(employee: Employee): Promise<void> {
    setOpenclawBusy(true);
    try {
      await provisionEmployeeRuntime(employee.id);
      setStatus(`${employee.name} 已绑定 OpenClaw Agent；后续消息由该 Agent Runtime 处理。`);
    } catch {
      setStatus('Provision 失败：请先安装 OpenClaw、启动 Gateway，并检查后端环境变量。');
    } finally {
      setOpenclawBusy(false);
    }
  }

  async function provisionWorkspace(employee: Employee): Promise<void> {
    setOpenclawBusy(true);
    try {
      await provisionEmployeeWorkspace(employee.id);
      setStatus(`${employee.name} 已绑定宿主机员工目录；任务使用临时容器，文件和依赖持续保留。`);
    } catch {
      setStatus('员工目录创建失败：请确认后端已启用 Docker 并且对配置的根目录有写权限。');
    } finally {
      setOpenclawBusy(false);
    }
  }

  return (
    <section className="models-page">
      <div className="models-heading">
        <div><span className="eyebrow">MODEL CONTROL PLANE</span><h1>大模型</h1><p>配置多个 Provider，Agent 通过 model profile 绑定模型；完整 API Key 永远不会返回到浏览器。</p></div>
        <button className="send-button models-add" type="button" onClick={startCreate}>＋ 新增 Provider</button>
      </div>
      <section className="runtime-control-card">
        <div><span className="eyebrow">OPTIONAL AGENT RUNTIME</span><h2>OpenClaw Gateway</h2><p>{openclaw?.enabled ? `地址：${openclaw.gatewayUrl}` : '当前未启用；系统继续使用内置 Mock / Provider Runtime。'}</p>{openclaw?.lastError && <small>{openclaw.lastError}</small>}</div>
        <div className="runtime-control-actions"><span className={`enabled-pill ${openclaw?.reachable ? 'on' : ''}`}>{openclaw?.reachable ? 'Gateway 在线' : openclaw?.enabled ? '未连接' : '未启用'}</span><button className="ghost-button" type="button" onClick={() => void checkOpenClaw()} disabled={openclawBusy}>测试 Gateway</button></div>
      </section>
      <section className="runtime-control-card">
        <div><span className="eyebrow">EPHEMERAL CONTAINER RUNTIME</span><h2>ADK Rust + Docker</h2><p>{containerRuntime?.enabled ? `临时执行镜像：${containerRuntime.defaultImage}。每位员工的文件、Session 和依赖保存在宿主机独立目录。` : '当前未启用；设置 AI_COMPANY_DOCKER_ENABLED=true 后使用一次性任务容器。'}</p>{containerRuntime?.lastError && <small>{containerRuntime.lastError}</small>}</div>
        <div className="runtime-control-actions"><span className={`enabled-pill ${containerRuntime?.available ? 'on' : ''}`}>{containerRuntime?.available ? `Docker ${containerRuntime.engineVersion ?? '在线'}` : containerRuntime?.enabled ? '不可用' : '未启用'}</span></div>
      </section>
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
          <div className="binding-section"><div className="section-title">AGENT 模型与 RUNTIME 绑定</div><p className="binding-note">API Key 按次下发。ADK 不保存 container ID；文件和安装依赖直接持久化到宿主机员工目录。</p>{employees.map((employee) => { const suggestions = environmentSuggestions.filter((item) => item.employeeId === employee.id && item.status === 'SUGGESTED'); return <div className="binding-row" key={employee.id}><label><span>{employee.name}<small>{employee.title}</small></span><select value={employee.modelProfileId ?? ''} onChange={(event) => void bindEmployeeModel(employee.id, event.target.value || null)}><option value="">未绑定</option>{providers.map((provider) => <option value={provider.id} key={provider.id}>{provider.name} · {provider.model}</option>)}</select></label><div className="runtime-binding"><small>{employee.runtimeBinding?.runtimeType ?? 'MOCK'} · {employee.runtimeBinding?.provisionStatus ?? 'READY'}</small>{employee.runtimeBinding?.runtimeType === 'ADK_RUST' && <small title={employee.runtimeBinding.workspacePath}>{employee.runtimeBinding.workspacePath}</small>}{suggestions.length > 0 && <small>镜像建议：{suggestions.slice(0, 3).map((item) => `${item.packageName}${item.requestedSpec ?? ''}`).join('、')}{suggestions.length > 3 ? ` 等 ${suggestions.length} 项` : ''}</small>}{employee.employmentStatus === 'ACTIVE' && employee.runtimeBinding?.runtimeType !== 'OPENCLAW' && <button className="ghost-button" type="button" onClick={() => void provision(employee)} disabled={openclawBusy}>绑定 OpenClaw</button>}{employee.employmentStatus === 'ACTIVE' && <button className="ghost-button" type="button" onClick={() => void provisionWorkspace(employee)} disabled={openclawBusy}>{employee.runtimeBinding?.runtimeType === 'ADK_RUST' ? '检查员工目录' : '创建员工目录'}</button>}</div></div>; })}</div>
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
    if (!content || sending || employee.employmentStatus !== 'ACTIVE') return;
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
          </div><form className="phone-compose" onSubmit={submit}><input value={draft} disabled={employee.employmentStatus !== 'ACTIVE'} onChange={(event) => setDraft(event.target.value)} placeholder={employee.employmentStatus === 'ACTIVE' ? `发送给 ${employee.name}…` : '该员工已离职，仅保留历史消息'} /><button type="submit" disabled={sending || employee.employmentStatus !== 'ACTIVE'}>{sending ? '…' : '↑'}</button></form></div>}
          {tab === 'profile' && <ProfileTab employee={employee} />}
          {tab === 'work' && <WorkTab employee={employee} goalTitle={goal?.title} task={task?.title} jobs={jobs} />}
          {tab === 'memory' && <MemoryTab memories={memories} />}
        </div>
        <div className="phone-home-indicator" />
      </section>
    </div>
  );
}

function MeetingMessageModal({
  meeting,
  employees,
  messages,
  onClose,
  onSend,
}: {
  meeting: MeetingSummary | null;
  employees: Employee[];
  messages: MessageRecord[];
  onClose: () => void;
  onSend: (meetingId: string, content: string) => Promise<void>;
}): ReactElement {
  const [draft, setDraft] = useState('');
  const [sending, setSending] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const employeeById = useMemo(() => new Map(employees.map((employee) => [employee.id, employee])), [employees]);
  const conversation = useMemo(() => {
    if (!meeting) return [];
    const seenBroadcasts = new Set<string>();
    return messages
      .filter((message) => message.meetingId === meeting.id)
      .sort((left, right) => left.createdAt.localeCompare(right.createdAt))
      .filter((message) => {
        if (message.fromType !== 'user' || !message.broadcastId) return true;
        if (seenBroadcasts.has(message.broadcastId)) return false;
        seenBroadcasts.add(message.broadcastId);
        return true;
      });
  }, [meeting, messages]);

  useLayoutEffect(() => {
    const list = listRef.current;
    if (list) list.scrollTop = list.scrollHeight;
  }, [conversation.length]);

  async function submit(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const content = draft.trim();
    if (!meeting || !content || sending) return;
    setSending(true);
    try {
      await onSend(meeting.id, content);
      setDraft('');
    } finally {
      setSending(false);
    }
  }

  return (
    <div className="meeting-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section className="meeting-message-shell" role="dialog" aria-modal="true" aria-label="会议桌消息">
        <div className="meeting-message-header">
          <div><span className="eyebrow">MEETING TABLE / BROADCAST</span><h2>会议桌消息</h2></div>
          <button type="button" onClick={onClose} aria-label="关闭">×</button>
        </div>
        {!meeting ? (
          <div className="meeting-empty">当前没有进行中的会议。先召开会议，等 Agent 到达会议桌后即可向全体在席成员发送消息。</div>
        ) : (
          <>
            <div className="meeting-context-card">
              <strong>{meeting.title}</strong>
              <span>{meeting.status === 'IN_PROGRESS' ? '会议进行中' : '等待会议开始'} · 只有已经坐到会议桌的 Agent 会收到广播</span>
              <div className="meeting-participant-list">
                {meeting.participants.map((participant) => {
                  const employee = employeeById.get(participant.agentId);
                  return <span key={participant.agentId} className={`meeting-participant ${participant.status}`}><i />{employee?.name ?? participant.agentId}<small>{participant.status}</small></span>;
                })}
              </div>
            </div>
            <div className="meeting-message-list" ref={listRef}>
              {conversation.length === 0 && <div className="meeting-empty">还没有会议消息。可以直接向在席 Agent 描述目标、需求或决策。</div>}
              {conversation.map((message) => {
                const employee = message.fromType === 'employee' ? employeeById.get(message.fromId) : undefined;
                return <div className={`meeting-bubble ${message.fromType === 'user' ? 'outgoing' : 'incoming'}`} key={message.id}><strong>{message.fromType === 'user' ? 'CEO → 全体在席 Agent' : employee?.name ?? message.fromId}</strong><span>{message.content}</span><time>{formatTime(message.createdAt)}</time></div>;
              })}
            </div>
            <form className="meeting-compose" onSubmit={submit}>
              <input value={draft} onChange={(event) => setDraft(event.target.value)} placeholder="向会议桌上的所有 Agent 发送消息…" />
              <button type="submit" disabled={sending}>{sending ? '…' : '发送广播'}</button>
            </form>
          </>
        )}
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

function EmployeeListItem({ employee, selected, onClick, onDoubleClick }: { employee: Employee; selected: boolean; onClick: () => void; onDoubleClick: () => void }): ReactElement {
  const usage = employee.tokenUsage ?? EMPTY_TOKEN_USAGE;
  const employmentLabel = employee.employmentStatus === 'TERMINATING' ? '离职中' : employee.employmentStatus === 'TERMINATED' ? '已离职' : null;
  return <button className={`employee-item ${selected ? 'selected' : ''} ${employee.employmentStatus !== 'ACTIVE' ? 'former-employee' : ''}`} type="button" onClick={onClick} onDoubleClick={onDoubleClick} title="单击选中，双击打开手机"><span className="employee-avatar" style={{ background: employee.appearance.topColor, color: employee.appearance.skinTone }}>{employee.name.slice(0, 1)}</span><span className="employee-item-copy"><strong>{employee.name}{employmentLabel && <em>{employmentLabel}</em>}</strong><small>{employee.title}</small><span className="employee-token-breakdown">IN {formatTokens(usage.inputTokens)} · OUT {formatTokens(usage.outputTokens)} · CACHE {formatTokens(usage.cacheTokens)}</span></span><span className="employee-token-total" title={`累计 ${usage.totalTokens.toLocaleString('zh-CN')} tokens`}>{formatTokens(usage.totalTokens)}</span>{employee.unreadCount > 0 && <span className="employee-unread">{employee.unreadCount}</span>}<span className={`state-dot ${employee.currentState}`} /></button>;
}

function TokenUsageOverview({ usage }: { usage: TokenUsageSummary }): ReactElement {
  return <section className="token-overview"><div><span>累计 Token</span><strong title={usage.totalTokens.toLocaleString('zh-CN')}>{formatTokens(usage.totalTokens)}</strong></div><div className="token-overview-breakdown"><span>IN <b>{formatTokens(usage.inputTokens)}</b></span><span>OUT <b>{formatTokens(usage.outputTokens)}</b></span><span>CACHE <b>{formatTokens(usage.cacheTokens)}</b></span></div></section>;
}

function EmployeeProfile({ employee, onOpenPhone, onRequestTermination, onCancelTermination }: { employee: Employee; onOpenPhone: () => void; onRequestTermination: () => void; onCancelTermination: () => void }): ReactElement {
  const focusEmployee = (): void => { window.dispatchEvent(new CustomEvent('office:focus', { detail: employee.id })); };
  const employmentLabel = employee.employmentStatus === 'TERMINATING' ? '离职中' : employee.employmentStatus === 'TERMINATED' ? '已离职' : STATE_LABELS[employee.currentState];
  return <div className="profile-card"><div className="profile-banner" style={{ background: `linear-gradient(135deg, ${employee.appearance.topColor}, #1e2b3a)` }}><span className="profile-avatar" style={{ background: employee.appearance.skinTone, color: employee.appearance.hairColor }}>{employee.name.slice(0, 1)}</span><div><h3>{employee.name}</h3><span>{employee.title}</span></div><span className={`profile-state ${employee.currentState}`}>{employmentLabel}</span></div><div className="profile-body"><div className="profile-facts"><span>{employee.age} 岁 · {genderLabel(employee.gender)}</span><span>{employee.department}</span></div><div className="profile-section"><div className="section-title">性格特征</div><div className="trait-row">{employee.personality.traits.map((trait) => <span className="trait-chip" key={trait}>{trait}</span>)}</div><p className="profile-quote">“{employee.personality.communicationStyle}”</p></div><div className="profile-section"><div className="section-title">技能档案</div>{employee.skills.slice(0, 4).map((skill) => <div className="skill-row" key={skill.name}><div><span>{skill.name}</span><small>{skill.level}</small></div><div className="skill-track"><span style={{ width: `${skill.level}%` }} /></div></div>)}</div><div className="profile-section current-work"><div className="section-title">当前工作</div><div className="work-status"><span className={`state-dot ${employee.currentState}`} /><strong>{employee.employmentStatus === 'TERMINATED' ? '历史员工' : employmentLabel}</strong><span>· {employee.modelProfile?.model ?? employee.modelConfig.model}</span></div></div><div className="profile-actions"><button className="locate-button" type="button" onClick={focusEmployee}>⌖ 定位</button><button className="phone-button" type="button" onClick={onOpenPhone}>▣ 查看手机</button>{employee.employmentStatus === 'ACTIVE' && <button className="danger-button" type="button" onClick={onRequestTermination}>解雇</button>}{employee.employmentStatus === 'TERMINATING' && <button className="ghost-button" type="button" onClick={onCancelTermination}>恢复在职</button>}</div></div></div>;
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

function genderLabel(gender: Employee['gender']): string {
  return gender === 'male' ? '男' : gender === 'female' ? '女' : '非二元';
}
