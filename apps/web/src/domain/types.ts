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
  personality: PersonalityProfile;
  skills: SkillProfile[];
  appearance: AppearanceProfile;
  modelConfig: AgentModelConfig;
  modelProfileId?: string;
  modelProfile?: ModelProfileSummary;
  deskId?: string;
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

export interface CompanySnapshot {
  company: Company;
  employees: Employee[];
  goals: GoalSummary[];
  tasks: TaskSummary[];
  meetings: MeetingSummary[];
  hiringRequests: unknown[];
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
}
