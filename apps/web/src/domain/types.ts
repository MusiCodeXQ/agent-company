export type Gender = 'male' | 'female' | 'non_binary';

/** Pixi 使用稳定的前端状态名；后端状态统一使用大写枚举。 */
export type ActorState =
  | 'idle'
  | 'walking'
  | 'checking_phone'
  | 'thinking'
  | 'coding'
  | 'designing'
  | 'testing'
  | 'reviewing'
  | 'meeting'
  | 'interviewing'
  | 'waiting'
  | 'coffee'
  | 'restroom'
  | 'error'
  | 'offline';

export type AgentActivity =
  | 'IDLE'
  | 'THINKING'
  | 'READING'
  | 'CODING'
  | 'TESTING'
  | 'TALKING'
  | 'MEETING'
  | 'INTERVIEWING'
  | 'WAITING'
  | 'ERROR'
  | string;

export type AgentMovement = 'STATIONARY' | 'PREPARING' | 'WALKING' | 'ARRIVED' | string;
export type AgentDevice = 'NONE' | 'PHONE' | 'COMPUTER' | string;
export type AgentPosture = 'STANDING' | 'WALKING' | 'SITTING' | string;
export type AgentPresence = 'AVAILABLE' | 'BUSY' | 'OFFLINE' | string;

const BACKEND_STATE_MAP: Record<string, ActorState> = {
  IDLE: 'idle',
  WALKING: 'walking',
  CHECK_PHONE: 'checking_phone',
  THINKING: 'thinking',
  WORKING: 'coding',
  MEETING: 'meeting',
  WAITING: 'waiting',
  ERROR: 'error',
};

export function normalizeActorState(value: string | undefined): ActorState {
  if (!value) return 'idle';
  return BACKEND_STATE_MAP[value.toUpperCase()] ?? (value.toLowerCase() as ActorState);
}

export interface PersonalityProfile {
  openness: number;
  conscientiousness: number;
  extraversion: number;
  agreeableness: number;
  emotionalStability: number;
  traits: string[];
  communicationStyle: string;
  decisionStyle: string;
  speechHabits: string[];
  behaviorHabits: string[];
}

export interface SkillProfile {
  name: string;
  level: number;
}

export interface AppearanceProfile {
  seed: number;
  skinTone: string;
  hairStyle: string;
  hairColor: string;
  topStyle: string;
  topColor: string;
  bottomStyle: string;
  bottomColor: string;
  shoeColor: string;
  bodyType: 'slim' | 'regular' | 'broad';
  heightScale: number;
  glasses: boolean;
  beardStyle?: string;
  accessory: 'watch' | 'headphones' | 'badge' | 'none';
}

export interface AgentModelConfig {
  provider: string;
  model: string;
  mode: 'mock' | 'real' | string;
}

export interface TokenUsageSummary {
  inputTokens: number;
  outputTokens: number;
  cacheTokens: number;
  totalTokens: number;
}

export interface ModelProfileSummary {
  id: string;
  name: string;
  protocol: string;
  baseUrl: string;
  apiKeyConfigured: boolean;
  apiKeyHint?: string | null;
  model: string;
  enabled: boolean;
}

export interface RuntimeBindingSummary {
  id: string;
  employeeId: string;
  runtimeType: string;
  runtimeAgentId: string;
  runtimeProfileId?: string | null;
  workspacePath: string;
  agentDir?: string | null;
  provisionStatus: string;
  runtimeStatus: string;
  environmentPath?: string | null;
  artifactsPath?: string | null;
  lastHealthAt?: string | null;
  lastError?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ContainerRuntimeStatus {
  enabled: boolean;
  available: boolean;
  engineVersion?: string | null;
  defaultImage: string;
  lastError?: string | null;
}

export interface SystemLogEntry {
  id: string;
  timestamp: string;
  level: 'ERROR' | 'WARN' | 'INFO' | string;
  source: 'JOB' | 'RUNTIME' | 'RUNTIME_EVENT' | 'MODEL' | string;
  employeeId?: string | null;
  employeeName?: string | null;
  message: string;
  details?: string | null;
  jobId?: string | null;
  runtimeRunId?: string | null;
  eventType?: string | null;
  model?: string | null;
}

export interface SystemLogsResponse {
  entries: SystemLogEntry[];
  containerRuntime: ContainerRuntimeStatus;
  page: number;
  pageSize: number;
  hasMore: boolean;
}

export interface RuntimeWorkspaceSummary {
  id: string;
  companyId: string;
  employeeId: string;
  hostRootPath: string;
  workspacePath: string;
  statePath: string;
  artifactsPath: string;
  environmentPath: string;
  status: string;
}

export interface RuntimeEnvironmentSuggestionSummary {
  id: string;
  companyId: string;
  employeeId: string;
  runtimeRunId?: string | null;
  ecosystem: string;
  packageName: string;
  requestedSpec?: string | null;
  sourceCommand: string;
  status: string;
  createdAt: string;
  updatedAt: string;
}

export interface OpenClawStatus {
  enabled: boolean;
  configured: boolean;
  gatewayUrl: string;
  reachable: boolean;
  protocol?: number | null;
  version?: string | null;
  lastError?: string | null;
  checkedAt?: string | null;
}

export interface Destination {
  kind: 'desk' | 'room' | 'spawn' | 'location';
  id: string;
}

export interface Employee {
  id: string;
  companyId: string;
  name: string;
  age: number;
  gender: Gender;
  role: string;
  department: string;
  title: string;
  status: string;
  employmentStatus: 'ACTIVE' | 'SUSPENDED' | 'TERMINATING' | 'TERMINATED' | string;
  isFloorAdministrator: boolean;
  personality: PersonalityProfile;
  skills: SkillProfile[];
  workStyle: string;
  appearance: AppearanceProfile;
  modelConfig: AgentModelConfig;
  modelProfileId?: string;
  modelProfile?: ModelProfileSummary;
  runtimeBinding?: RuntimeBindingSummary | null;
  deskId?: string;
  floorId?: string | null;
  hiredAt: string;
  currentState: ActorState;
  currentDestination?: Destination;
  activity: AgentActivity;
  movement: AgentMovement;
  locationId?: string | null;
  x: number;
  y: number;
  roomId?: string | null;
  device: AgentDevice;
  posture: AgentPosture;
  presence: AgentPresence;
  activeMovementId?: string | null;
  unreadCount: number;
  tokenUsage: TokenUsageSummary;
}

export interface Company {
  id: string;
  name: string;
  status: string;
  createdAt: string;
}

export interface GoalSummary {
  id: string;
  title: string;
  description: string;
  status: string;
}

export interface TaskSummary {
  id: string;
  title: string;
  description: string;
  taskType: string;
  status: string;
  assigneeId?: string;
  goalId?: string;
  previousAssigneeId?: string | null;
  handoverStatus: string;
}

export interface TerminationTaskSummary {
  id: string;
  title: string;
  status: string;
  goalId?: string | null;
}

export interface TerminationPreview {
  employee: Employee;
  activeTaskCount: number;
  runningJobCount: number;
  activeGoalCount: number;
  activeMeetingCount: number;
  artifactCount: number;
  workspacePath?: string | null;
  workspaceStatus: string;
  runtimeType: string;
  runtimeStatus: string;
  workstationId?: string | null;
  unreadCount: number;
  tasks: TerminationTaskSummary[];
  activeTerminationId?: string | null;
}

export interface TerminationRecordSummary {
  id: string;
  companyId: string;
  employeeId: string;
  initiatedBy: string;
  reason: string;
  notes: string;
  status: string;
  workspaceAction: string;
  taskHandoverStatus: string;
  runtimeShutdownStatus: string;
  workstationReleaseStatus: string;
  createdAt: string;
  effectiveAt?: string | null;
  completedAt?: string | null;
}

export interface MeetingSummary {
  id: string;
  title: string;
  meetingType: string;
  roomId: string;
  participantIds: string[];
  participants: MeetingParticipantSummary[];
  status: string;
}

export interface MeetingParticipantSummary {
  agentId: string;
  seatLocationId: string;
  status: string;
}

export interface MessageRecord {
  id: string;
  companyId: string;
  fromType: string;
  fromId: string;
  toType: string;
  toId: string;
  content: string;
  createdAt: string;
  readAt?: string | null;
  jobId?: string | null;
  meetingId?: string | null;
  broadcastId?: string | null;
}

export interface A2aFaceToFaceSession {
  id: string;
  companyId: string;
  contextId: string;
  initiatorId: string;
  recipientId: string;
  initiatorLocationId: string;
  recipientLocationId: string;
  roomId: string;
  status: string;
  maxTurns: number;
  turnsUsed: number;
  tokenBudget: number;
  tokensUsed: number;
  companyTaskId?: string | null;
  expiresAt: string;
  createdAt: string;
  activatedAt?: string | null;
  closedAt?: string | null;
  closeReason?: string | null;
}

export interface A2aCommunicationMessage {
  id: string;
  sessionId: string;
  contextId: string;
  taskId?: string | null;
  senderId: string;
  recipientId: string;
  role: string;
  parts: unknown;
  metadata: unknown;
  direction: string;
  createdAt: string;
}

export interface A2aCommunicationTask {
  id: string;
  contextId: string;
  sessionId: string;
  senderId: string;
  recipientId: string;
  companyTaskId?: string | null;
  agentJobId?: string | null;
  state: string;
  createdAt: string;
  updatedAt: string;
  completedAt?: string | null;
  error?: string | null;
}

export interface A2aCommunicationAudit {
  id: string;
  sessionId?: string | null;
  taskId?: string | null;
  actorId?: string | null;
  action: string;
  outcome: string;
  details: unknown;
  createdAt: string;
}

export interface A2aCommunicationDetail {
  session: A2aFaceToFaceSession;
  messages: A2aCommunicationMessage[];
  tasks: A2aCommunicationTask[];
  audit: A2aCommunicationAudit[];
}

export interface RuntimeToolApprovalSummary {
  id: string;
  companyId: string;
  runId: string;
  agentJobId: string;
  employeeId: string;
  functionCallId: string;
  toolName: string;
  argumentsRedacted: string;
  argumentsFingerprint: string;
  status: string;
  decidedBy?: string | null;
  decisionReason?: string | null;
  createdAt: string;
  expiresAt: string;
  decidedAt?: string | null;
}

export interface EmployeeMessageHistoryState {
  oldestCreatedAt: string | null;
  hasMore: boolean;
  loading: boolean;
}

export interface AgentMemoryRecord {
  id: string;
  employeeId: string;
  kind: string;
  content: string;
  createdAt: string;
  updatedAt: string;
}

export interface AgentJobSummary {
  id: string;
  jobType: string;
  status: string;
  employeeId: string;
  messageId?: string;
  goalId?: string;
  taskId?: string;
  meetingId?: string | null;
  attempts: number;
  result?: string | null;
  error?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface DomainEvent<TPayload = Record<string, unknown>> {
  eventId: string;
  companyId: string;
  type: string;
  timestamp: string;
  actorId?: string;
  correlationId?: string;
  payload: TPayload;
}

export interface RoomDefinition {
  id: string;
  type: 'workspace' | 'meeting' | 'interview' | 'office' | 'reception' | 'pantry' | 'restroom' | string;
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface FurnitureDefinition {
  id: string;
  kind: string;
  x: number;
  y: number;
  width: number;
  height: number;
  label?: string;
}

export interface DeskDefinition {
  id: string;
  employeeId: string;
  x: number;
  y: number;
  width: number;
  height: number;
  seatId: string;
  monitorX: number;
  monitorY: number;
}

export interface SeatDefinition {
  id: string;
  kind: string;
  x: number;
  y: number;
  roomId?: string;
  employeeId?: string;
  deskId?: string;
}

export interface DoorDefinition {
  id: string;
  roomId: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SpawnPoint {
  id: string;
  x: number;
  y: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface LocationDefinition {
  id: string;
  name: string;
  type: string;
  roomId?: string | null;
  x: number;
  y: number;
}

export interface MovementConfig {
  baseSpeed: number;
  gridCellSize: number;
}

export interface MovementPoint {
  x: number;
  y: number;
}

export interface ActiveMovement {
  id: string;
  companyId: string;
  agentId: string;
  fromLocationId: string;
  toLocationId: string;
  path: MovementPoint[];
  startedAt: string;
  estimatedArrivalAt: string;
  movementStatus: 'PENDING' | 'MOVING' | 'ARRIVED' | 'CANCELLED' | 'FAILED' | string;
  createdAt: string;
  completedAt?: string | null;
}

export interface OfficeLayout {
  floorId?: string | null;
  floorName?: string | null;
  width: number;
  height: number;
  rooms: RoomDefinition[];
  furniture: FurnitureDefinition[];
  desks: DeskDefinition[];
  seats: SeatDefinition[];
  doors: DoorDefinition[];
  spawnPoints: SpawnPoint[];
  navigationObstacles: Rect[];
  locations: LocationDefinition[];
  movement: MovementConfig;
}

export interface FloorSummary {
  id: string;
  companyId: string;
  floorNumber: number;
  name: string;
  description: string;
  mission: string;
  responsibilities: string[];
  roles: FloorRoleSummary[];
  status: string;
  employeeCount: number;
  workstationCount: number;
  availableWorkstations: number;
  createdAt: string;
  updatedAt: string;
}

export interface FloorRoleSummary {
  id: string;
  name: string;
  responsibilities: string[];
  skills: string[];
  preferences: string[];
}

export interface WorkstationSummary {
  id: string;
  companyId: string;
  floorId: string;
  locationId: string;
  roomId?: string | null;
  x: number;
  y: number;
  orientation: string;
  status: string;
  assignedEmployeeId?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface HiringPositionSummary {
  id: string;
  roleDefinitionId?: string | null;
  title: string;
  headcount: number;
  requirements: string[];
  preferences: string[];
  status: string;
}

export interface CandidateSummary {
  id: string;
  hiringRequestId: string;
  positionId: string;
  name: string;
  age: number;
  gender: Gender;
  title: string;
  personality: PersonalityProfile;
  skills: SkillProfile[];
  workStyle: string;
  strengths: string[];
  weaknesses: string[];
  expertise: string[];
  expectedModel: string;
  agentRuntime: string;
  appearance: AppearanceProfile;
  matchScore: number;
  interviewEvaluation: string;
  status: string;
  hiredEmployeeId?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface HiringRequestSummary {
  id: string;
  companyId: string;
  floorId: string;
  requesterId?: string | null;
  summary: string;
  status: string;
  sourceMessageId?: string | null;
  positions: HiringPositionSummary[];
  candidates: CandidateSummary[];
  createdAt: string;
  updatedAt: string;
}

export interface CompanySnapshot {
  company: Company;
  employees: Employee[];
  goals: GoalSummary[];
  tasks: TaskSummary[];
  meetings: MeetingSummary[];
  floors: FloorSummary[];
  activeFloorId: string;
  workstations: WorkstationSummary[];
  hiringRequests: HiringRequestSummary[];
  messages: MessageRecord[];
  unreadCounts: Record<string, number>;
  jobs: AgentJobSummary[];
  modelProfiles: ModelProfileSummary[];
  office: OfficeLayout;
  officeState: Array<{ employeeId: string; currentState: string; destination?: Destination | null }>;
  activeMovements: ActiveMovement[];
  tokenUsage: TokenUsageSummary;
  serverTime: string;
  recentEvents: DomainEvent[];
  openclaw: OpenClawStatus;
  containerRuntime: ContainerRuntimeStatus;
  environmentSuggestions: RuntimeEnvironmentSuggestionSummary[];
}

export type BootstrapResponse = CompanySnapshot;

export interface CompanyStoreState {
  bootstrap: CompanySnapshot | null;
  employees: Employee[];
  events: DomainEvent[];
  messages: MessageRecord[];
  messageHistory: Record<string, EmployeeMessageHistoryState>;
  activeMovements: ActiveMovement[];
  serverTime: string | null;
  memories: Record<string, AgentMemoryRecord[]>;
  selectedEmployeeId: string | null;
  phoneEmployeeId: string | null;
  loading: boolean;
  error: string | null;
  connectionStatus: 'connecting' | 'connected' | 'disconnected';
  activeFloorId: string | null;
  terminationPreview: TerminationPreview | null;
  a2aSessions: A2aFaceToFaceSession[];
  a2aCommunications: Record<string, A2aCommunicationDetail>;
  a2aLoading: boolean;
  a2aCommunicationLoading: Record<string, boolean>;
  runtimeApprovals: RuntimeToolApprovalSummary[];
  runtimeApprovalsLoading: boolean;
}
