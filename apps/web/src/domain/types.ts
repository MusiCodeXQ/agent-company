export type Gender = 'male' | 'female' | 'non_binary';

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

export interface Destination {
  kind: 'desk' | 'room' | 'spawn';
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
  deskId?: string;
  hiredAt: string;
  currentState: ActorState;
  currentDestination?: Destination;
}

export interface Company {
  id: string;
  name: string;
  status: string;
  createdAt: string;
}

export interface TaskSummary {
  id: string;
  title: string;
  status: string;
  assigneeId?: string;
}

export interface MeetingSummary {
  id: string;
  title: string;
  meetingType: string;
  roomId: string;
  participantIds: string[];
  status: string;
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
}

export interface BootstrapResponse {
  company: Company;
  employees: Employee[];
  tasks: TaskSummary[];
  meetings: MeetingSummary[];
  hiringRequests: unknown[];
  office: OfficeLayout;
  recentEvents: DomainEvent[];
}

export interface CompanyStoreState {
  bootstrap: BootstrapResponse | null;
  employees: Employee[];
  events: DomainEvent[];
  selectedEmployeeId: string | null;
  loading: boolean;
  error: string | null;
  connectionStatus: 'connecting' | 'connected' | 'disconnected';
}

