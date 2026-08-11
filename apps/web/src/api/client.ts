import type {
  AgentMemoryRecord,
  AgentJobSummary,
  CompanySnapshot,
  DomainEvent,
  FloorSummary,
  HiringRequestSummary,
  MessageRecord,
  ModelProfileSummary,
  OpenClawStatus,
  RuntimeWorkspaceSummary,
  Employee,
  MeetingSummary,
  TerminationPreview,
  TerminationRecordSummary,
  WorkstationSummary,
} from '../domain/types';

const COMPANY_ID = 'company-01';
export const EMPLOYEE_MESSAGE_PAGE_SIZE = 40;

export interface MessageSendResponse {
  message: MessageRecord;
  jobId: string;
  status: string;
}

export interface MeetingMessageSendResponse {
  meetingId: string;
  broadcastId: string;
  messages: MessageRecord[];
  jobIds: string[];
  status: string;
}

export interface ModelProfileInput {
  name: string;
  protocol: string;
  baseUrl: string;
  apiKey?: string;
  model: string;
  enabled: boolean;
}

export interface ModelProfileUpdate {
  name?: string;
  protocol?: string;
  baseUrl?: string;
  apiKey?: string;
  model?: string;
  enabled?: boolean;
}

export interface MeetingInput {
  title?: string;
  participantIds: string[];
  meetingType?: string;
}

export interface FloorInput {
  name: string;
  description?: string;
  mission?: string;
}

export interface FloorUpdateInput {
  name?: string;
  description?: string;
  mission?: string;
  responsibilities?: string[];
}

export interface WorkstationInput {
  x?: number;
  y?: number;
  roomId?: string;
  orientation?: string;
}

export interface HireCandidateInput {
  candidateId: string;
  workstationId?: string;
}

export interface HireCandidateResponse {
  employee: Employee;
  movement?: unknown;
  floorId: string;
  workstationId: string;
  status: string;
}

export interface TerminationInput {
  reason?: string;
  notes?: string;
  taskAction?: 'REASSIGN' | 'PAUSE' | 'CANCEL';
  workspaceAction?: 'ARCHIVE' | 'READ_ONLY';
}

async function readError(response: Response, fallback: string): Promise<Error> {
  try {
    const body = await response.json() as { error?: string };
    return new Error(body.error ?? `${fallback} (${response.status})`);
  } catch {
    return new Error(`${fallback} (${response.status})`);
  }
}

export async function fetchSnapshot(floorId?: string): Promise<CompanySnapshot> {
  const query = floorId ? `?floorId=${encodeURIComponent(floorId)}` : '';
  const response = await fetch(`/api/company/snapshot${query}`);
  if (!response.ok) throw await readError(response, 'Snapshot request failed');
  return response.json() as Promise<CompanySnapshot>;
}

export const fetchBootstrap = fetchSnapshot;

export async function createFloor(input: FloorInput): Promise<FloorSummary> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/floors`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!response.ok) throw await readError(response, 'Floor creation failed');
  return response.json() as Promise<FloorSummary>;
}

export async function updateFloor(floorId: string, input: FloorUpdateInput): Promise<FloorSummary> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/floors/${encodeURIComponent(floorId)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!response.ok) throw await readError(response, 'Floor update failed');
  return response.json() as Promise<FloorSummary>;
}

export async function deleteFloor(floorId: string): Promise<void> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/floors/${encodeURIComponent(floorId)}`, { method: 'DELETE' });
  if (!response.ok) throw await readError(response, 'Floor deletion failed');
}

export async function createWorkstation(floorId: string, input: WorkstationInput = {}): Promise<WorkstationSummary> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/floors/${encodeURIComponent(floorId)}/workstations`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!response.ok) throw await readError(response, 'Workstation creation failed');
  return response.json() as Promise<WorkstationSummary>;
}

export async function hireCandidate(requestId: string, input: HireCandidateInput): Promise<HireCandidateResponse> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/hiring-requests/${encodeURIComponent(requestId)}/hire`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!response.ok) throw await readError(response, 'Candidate hiring failed');
  return response.json() as Promise<HireCandidateResponse>;
}

export async function fetchHiringRequests(): Promise<HiringRequestSummary[]> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/hiring-requests`);
  if (!response.ok) throw await readError(response, 'Hiring request fetch failed');
  return response.json() as Promise<HiringRequestSummary[]>;
}

export async function refreshHiringCandidates(requestId: string): Promise<HiringRequestSummary> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/hiring-requests/${encodeURIComponent(requestId)}/refresh`, {
    method: 'POST',
  });
  if (!response.ok) throw await readError(response, 'Candidate refresh failed');
  return response.json() as Promise<HiringRequestSummary>;
}

export async function listModelProfiles(): Promise<ModelProfileSummary[]> {
  const response = await fetch('/api/model-profiles');
  if (!response.ok) throw await readError(response, 'Provider list request failed');
  return response.json() as Promise<ModelProfileSummary[]>;
}

export async function createModelProfile(input: ModelProfileInput): Promise<ModelProfileSummary> {
  const response = await fetch('/api/model-profiles', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!response.ok) throw await readError(response, 'Provider creation failed');
  return response.json() as Promise<ModelProfileSummary>;
}

export async function updateModelProfile(profileId: string, input: ModelProfileUpdate): Promise<ModelProfileSummary> {
  const response = await fetch(`/api/model-profiles/${encodeURIComponent(profileId)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!response.ok) throw await readError(response, 'Provider update failed');
  return response.json() as Promise<ModelProfileSummary>;
}

export async function testModelProfile(profileId: string): Promise<{ ok: boolean; message: string }> {
  const response = await fetch(`/api/model-profiles/${encodeURIComponent(profileId)}/test`, { method: 'POST' });
  if (!response.ok) throw await readError(response, 'Provider connection test failed');
  return response.json() as Promise<{ ok: boolean; message: string }>;
}

export async function fetchOpenClawStatus(): Promise<OpenClawStatus> {
  const response = await fetch('/api/openclaw/status');
  if (!response.ok) throw await readError(response, 'OpenClaw status request failed');
  return response.json() as Promise<OpenClawStatus>;
}

export async function fetchTerminationPreview(employeeId: string): Promise<TerminationPreview> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/employees/${encodeURIComponent(employeeId)}/termination-preview`);
  if (!response.ok) throw await readError(response, 'Termination preview request failed');
  return response.json() as Promise<TerminationPreview>;
}

export async function terminateEmployee(employeeId: string, input: TerminationInput): Promise<TerminationRecordSummary> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/employees/${encodeURIComponent(employeeId)}/terminate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!response.ok) throw await readError(response, 'Employee termination failed');
  return response.json() as Promise<TerminationRecordSummary>;
}

export async function provisionEmployeeRuntime(employeeId: string): Promise<Employee> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/employees/${encodeURIComponent(employeeId)}/runtime/provision`, { method: 'POST' });
  if (!response.ok) throw await readError(response, 'OpenClaw runtime provisioning failed');
  return response.json() as Promise<Employee>;
}

export async function provisionEmployeeWorkspace(employeeId: string): Promise<RuntimeWorkspaceSummary> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/employees/${encodeURIComponent(employeeId)}/runtime/workspace`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ runtimeType: 'ADK_RUST' }),
  });
  if (!response.ok) throw await readError(response, 'ADK Rust workspace provisioning failed');
  return response.json() as Promise<RuntimeWorkspaceSummary>;
}

export async function sendCompanyMessage(content: string, employeeId?: string): Promise<MessageSendResponse> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content, employeeId }),
  });
  if (!response.ok) throw await readError(response, 'Message request failed');
  return response.json() as Promise<MessageSendResponse>;
}

export async function fetchEmployeeMessages(
  employeeId: string,
  before?: string,
  limit = EMPLOYEE_MESSAGE_PAGE_SIZE,
): Promise<MessageRecord[]> {
  const params = new URLSearchParams({ limit: String(limit) });
  if (before) params.set('before', before);
  const response = await fetch(`/api/companies/${COMPANY_ID}/employees/${encodeURIComponent(employeeId)}/messages?${params.toString()}`);
  if (!response.ok) throw await readError(response, 'Conversation request failed');
  return response.json() as Promise<MessageRecord[]>;
}

export async function bindEmployeeModelProfile(employeeId: string, profileId: string | null): Promise<Employee> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/employees/${encodeURIComponent(employeeId)}/model-profile`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ profileId }),
  });
  if (!response.ok) throw await readError(response, 'Agent model binding failed');
  return response.json() as Promise<Employee>;
}

export async function sendEmployeeMessage(employeeId: string, content: string): Promise<MessageSendResponse> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/employees/${encodeURIComponent(employeeId)}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content }),
  });
  if (!response.ok) throw await readError(response, 'Agent message request failed');
  return response.json() as Promise<MessageSendResponse>;
}

export async function sendMeetingMessage(meetingId: string, content: string): Promise<MeetingMessageSendResponse> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/meetings/${encodeURIComponent(meetingId)}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content }),
  });
  if (!response.ok) throw await readError(response, 'Meeting message request failed');
  return response.json() as Promise<MeetingMessageSendResponse>;
}

export async function markEmployeeMessagesRead(employeeId: string): Promise<void> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/employees/${encodeURIComponent(employeeId)}/messages/read`, { method: 'POST' });
  if (!response.ok) throw await readError(response, 'Mark messages read failed');
}

export async function fetchEmployeeMemory(employeeId: string): Promise<AgentMemoryRecord[]> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/employees/${encodeURIComponent(employeeId)}/memory`);
  if (!response.ok) throw await readError(response, 'Memory request failed');
  return response.json() as Promise<AgentMemoryRecord[]>;
}

export async function fetchCompanyJobs(): Promise<AgentJobSummary[]> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/jobs`);
  if (!response.ok) throw await readError(response, 'Job request failed');
  return response.json() as Promise<AgentJobSummary[]>;
}

export async function createMeeting(input: MeetingInput): Promise<MeetingSummary> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/meetings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!response.ok) throw await readError(response, 'Meeting creation failed');
  return response.json() as Promise<MeetingSummary>;
}

export async function finishMeeting(meetingId: string): Promise<MeetingSummary> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/meetings/${encodeURIComponent(meetingId)}/finish`, { method: 'POST' });
  if (!response.ok) throw await readError(response, 'Meeting finish failed');
  return response.json() as Promise<MeetingSummary>;
}

export function openCompanyWebSocket(
  onEvent: (event: DomainEvent) => void,
  onStatus: (status: 'connected' | 'disconnected') => void,
): () => void {
  const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
  const socket = new WebSocket(`${protocol}://${window.location.host}/ws?companyId=${COMPANY_ID}`);
  socket.addEventListener('open', () => onStatus('connected'));
  socket.addEventListener('close', () => onStatus('disconnected'));
  socket.addEventListener('error', () => onStatus('disconnected'));
  socket.addEventListener('message', (message) => {
    try {
      const value = JSON.parse(message.data) as Partial<DomainEvent> & { type?: string };
      if (value.type === 'connection.ready' || !value.eventId || !value.companyId || !value.timestamp) return;
      onEvent(value as DomainEvent);
    } catch {
      // A malformed incremental event must not tear down the live office.
    }
  });
  return () => socket.close();
}
