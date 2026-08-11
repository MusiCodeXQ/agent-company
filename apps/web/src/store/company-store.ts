import { create } from 'zustand';
import {
  createFloor as createFloorApi,
  cancelEmployeeTermination as cancelEmployeeTerminationApi,
  createModelProfile,
  createWorkstation as createWorkstationApi,
  bindEmployeeModelProfile,
  createMeeting,
  deleteFloor as deleteFloorApi,
  EMPLOYEE_MESSAGE_PAGE_SIZE,
  fetchEmployeeMemory,
  fetchEmployeeMessages,
  fetchSnapshot,
  finishMeeting,
  hireCandidate as hireCandidateApi,
  markEmployeeMessagesRead,
  openCompanyWebSocket,
  sendMeetingMessage,
  sendCompanyMessage,
  sendEmployeeMessage,
  fetchOpenClawStatus,
  fetchTerminationPreview,
  provisionEmployeeRuntime as provisionEmployeeRuntimeApi,
  provisionEmployeeWorkspace as provisionEmployeeWorkspaceApi,
  refreshHiringCandidates as refreshHiringCandidatesApi,
  terminateEmployee as terminateEmployeeApi,
  testModelProfile,
  updateFloor as updateFloorApi,
  updateModelProfile,
  type MessageSendResponse,
  type FloorInput,
  type FloorUpdateInput,
  type HireCandidateResponse,
  type ModelProfileInput,
  type ModelProfileUpdate,
  type TerminationInput,
} from '../api/client';
import {
  normalizeActorState,
  type ActiveMovement,
  type AgentMemoryRecord,
  type AgentJobSummary,
  type BootstrapResponse,
  type CompanyStoreState,
  type Destination,
  type DomainEvent,
  type Employee,
  type FloorSummary,
  type HiringRequestSummary,
  type MessageRecord,
  type ModelProfileSummary,
  type OpenClawStatus,
  type RuntimeEnvironmentSuggestionSummary,
  type TokenUsageSummary,
} from '../domain/types';

interface CompanyStoreActions {
  loadBootstrap: (floorId?: string) => Promise<void>;
  loadFloor: (floorId: string) => Promise<void>;
  connectWebSocket: () => () => void;
  applyEvent: (event: DomainEvent) => void;
  selectEmployee: (employeeId: string | null) => void;
  openPhone: (employeeId: string) => Promise<void>;
  closePhone: () => void;
  loadConversation: (employeeId: string) => Promise<void>;
  loadOlderConversation: (employeeId: string) => Promise<boolean>;
  sendAgentMessage: (employeeId: string, content: string) => Promise<void>;
  sendMeetingMessage: (meetingId: string, content: string) => Promise<void>;
  markRead: (employeeId: string) => Promise<void>;
  loadMemory: (employeeId: string) => Promise<void>;
  saveProvider: (input: ModelProfileInput, profileId?: string) => Promise<void>;
  testProvider: (profileId: string) => Promise<string>;
  bindEmployeeModel: (employeeId: string, profileId: string | null) => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
  startMeeting: (participantIds: string[]) => Promise<void>;
  finishMeeting: (meetingId: string) => Promise<void>;
  createFloor: (input: FloorInput) => Promise<void>;
  updateFloor: (floorId: string, input: FloorUpdateInput) => Promise<void>;
  deleteFloor: (floorId: string) => Promise<void>;
  createWorkstation: (floorId: string) => Promise<void>;
  hireCandidate: (requestId: string, candidateId: string, workstationId?: string) => Promise<HireCandidateResponse>;
  refreshHiringCandidates: (requestId: string) => Promise<void>;
  loadTerminationPreview: (employeeId: string) => Promise<void>;
  clearTerminationPreview: () => void;
  terminateEmployee: (employeeId: string, input: TerminationInput) => Promise<void>;
  cancelEmployeeTermination: (employeeId: string) => Promise<void>;
  testOpenClaw: () => Promise<OpenClawStatus>;
  provisionEmployeeRuntime: (employeeId: string) => Promise<void>;
  provisionEmployeeWorkspace: (employeeId: string) => Promise<void>;
}

type CompanyStore = CompanyStoreState & CompanyStoreActions;

function updateEmployee(employees: Employee[], employeeId: string, update: (employee: Employee) => Employee): Employee[] {
  return employees.map((employee) => (employee.id === employeeId ? update(employee) : employee));
}

function normalizeEmployees(employees: Employee[]): Employee[] {
  return employees.map((employee) => ({
    ...employee,
    employmentStatus: employee.employmentStatus ?? 'ACTIVE',
    currentState: normalizeActorState(employee.currentState),
    activity: employee.activity ?? 'IDLE',
    movement: employee.movement ?? 'STATIONARY',
    x: employee.x ?? 0,
    y: employee.y ?? 0,
    device: employee.device ?? 'NONE',
    posture: employee.posture ?? 'SITTING',
    presence: employee.presence ?? 'AVAILABLE',
    unreadCount: employee.unreadCount ?? 0,
    tokenUsage: normalizeTokenUsage(employee.tokenUsage),
  }));
}

function normalizeTokenUsage(value: Partial<TokenUsageSummary> | null | undefined): TokenUsageSummary {
  return {
    inputTokens: value?.inputTokens ?? 0,
    outputTokens: value?.outputTokens ?? 0,
    cacheTokens: value?.cacheTokens ?? 0,
    totalTokens: value?.totalTokens ?? 0,
  };
}

function tokenUsageFromPayload(value: unknown): TokenUsageSummary | null {
  if (!value || typeof value !== 'object') return null;
  const usage = value as Record<string, unknown>;
  if (
    typeof usage.inputTokens !== 'number'
    || typeof usage.outputTokens !== 'number'
    || typeof usage.cacheTokens !== 'number'
    || typeof usage.totalTokens !== 'number'
  ) return null;
  return {
    inputTokens: usage.inputTokens,
    outputTokens: usage.outputTokens,
    cacheTokens: usage.cacheTokens,
    totalTokens: usage.totalTokens,
  };
}

function mergeMessages(current: MessageRecord[], incoming: MessageRecord[]): MessageRecord[] {
  const byId = new Map(current.map((message) => [message.id, message]));
  incoming.forEach((message) => byId.set(message.id, message));
  return [...byId.values()].sort((left, right) => left.createdAt.localeCompare(right.createdAt));
}

function messageFromEvent(event: DomainEvent): MessageRecord | null {
  const payload = event.payload as Record<string, unknown>;
  if (typeof payload.messageId !== 'string' || typeof payload.content !== 'string') return null;
  return {
    id: payload.messageId,
    companyId: event.companyId,
    fromType: typeof payload.fromType === 'string' ? payload.fromType : event.type === 'message.received' ? 'employee' : 'user',
    fromId: typeof payload.fromId === 'string' ? payload.fromId : event.actorId ?? 'user',
    toType: typeof payload.toType === 'string' ? payload.toType : event.type === 'message.received' ? 'user' : 'employee',
    toId: typeof payload.toId === 'string' ? payload.toId : 'user',
    content: payload.content,
    createdAt: event.timestamp,
    readAt: undefined,
    jobId: typeof payload.jobId === 'string' ? payload.jobId : event.correlationId,
    meetingId: typeof payload.meetingId === 'string' ? payload.meetingId : null,
    broadcastId: typeof payload.broadcastId === 'string' ? payload.broadcastId : null,
  };
}

function updateJobs(jobs: AgentJobSummary[], event: DomainEvent): AgentJobSummary[] {
  const payload = event.payload as Record<string, unknown>;
  if (typeof payload.jobId !== 'string' || typeof payload.status !== 'string') return jobs;
  const existing = jobs.find((job) => job.id === payload.jobId);
  const next: AgentJobSummary = existing ?? {
    id: payload.jobId,
    jobType: typeof payload.jobType === 'string' ? payload.jobType : 'MESSAGE_RESPONSE',
    status: payload.status,
    employeeId: event.actorId ?? '',
    meetingId: typeof payload.meetingId === 'string' ? payload.meetingId : null,
    attempts: typeof payload.attempts === 'number' ? payload.attempts : 0,
    createdAt: event.timestamp,
    updatedAt: event.timestamp,
  };
  const updated = {
    ...next,
    status: payload.status,
    attempts: typeof payload.attempts === 'number' ? payload.attempts : next.attempts,
    updatedAt: event.timestamp,
  };
  return existing ? jobs.map((job) => (job.id === updated.id ? updated : job)) : [...jobs, updated];
}

function replaceProvider(snapshot: BootstrapResponse | null, provider: ModelProfileSummary): BootstrapResponse | null {
  if (!snapshot) return snapshot;
  const profiles = snapshot.modelProfiles.some((item) => item.id === provider.id)
    ? snapshot.modelProfiles.map((item) => (item.id === provider.id ? provider : item))
    : [...snapshot.modelProfiles, provider];
  return { ...snapshot, modelProfiles: profiles };
}

function movementFromEvent(event: DomainEvent): ActiveMovement | null {
  const payload = event.payload as Record<string, unknown>;
  const raw = payload.movement;
  if (!raw || typeof raw !== 'object') return null;
  const movement = raw as Partial<ActiveMovement>;
  if (
    typeof movement.id !== 'string'
    || typeof movement.agentId !== 'string'
    || !Array.isArray(movement.path)
    || typeof movement.startedAt !== 'string'
    || typeof movement.estimatedArrivalAt !== 'string'
  ) return null;
  return movement as ActiveMovement;
}

function dimensionsForState(value: string): { activity: string; device: string } {
  switch (value.toUpperCase()) {
    case 'CHECK_PHONE': return { activity: 'READING', device: 'PHONE' };
    case 'THINKING': return { activity: 'THINKING', device: 'NONE' };
    case 'WORKING': return { activity: 'CODING', device: 'COMPUTER' };
    case 'MEETING': return { activity: 'MEETING', device: 'NONE' };
    case 'WAITING': return { activity: 'WAITING', device: 'NONE' };
    case 'ERROR': return { activity: 'ERROR', device: 'NONE' };
    default: return { activity: 'IDLE', device: 'NONE' };
  }
}

export const useCompanyStore = create<CompanyStore>((set, get) => ({
  bootstrap: null,
  employees: [],
  events: [],
  messages: [],
  messageHistory: {},
  activeMovements: [],
  serverTime: null,
  memories: {},
  selectedEmployeeId: null,
  phoneEmployeeId: null,
  loading: true,
  error: null,
  connectionStatus: 'connecting',
  activeFloorId: null,
  terminationPreview: null,

  loadBootstrap: async (floorId) => {
    try {
      const snapshot = await fetchSnapshot(floorId);
      const employees = normalizeEmployees(snapshot.employees);
      const normalizedSnapshot = {
        ...snapshot,
        employees,
        tokenUsage: normalizeTokenUsage(snapshot.tokenUsage),
      };
      set({
        bootstrap: normalizedSnapshot,
        employees,
        messages: snapshot.messages,
        messageHistory: {},
        activeMovements: snapshot.activeMovements ?? [],
        serverTime: snapshot.serverTime ?? new Date().toISOString(),
        events: snapshot.recentEvents,
        activeFloorId: snapshot.activeFloorId,
        loading: false,
        error: null,
      });
    } catch (error) {
      set({ loading: false, error: error instanceof Error ? error.message : 'Unable to load company snapshot' });
    }
  },

  loadFloor: async (floorId) => {
    await get().loadBootstrap(floorId);
  },

  connectWebSocket: () => openCompanyWebSocket(
    (event) => get().applyEvent(event),
    (connectionStatus) => set({ connectionStatus }),
  ),

  applyEvent: (event) => {
    set((state) => {
      const nextEvents = state.events.some((item) => item.eventId === event.eventId)
        ? state.events
        : [...state.events, event].slice(-120);
      const payload = event.payload as Record<string, unknown>;
      let employees = state.employees;
      let messages = state.messages;
      let jobs = state.bootstrap?.jobs ?? [];
      let activeMovements = state.activeMovements;
      let meetings = state.bootstrap?.meetings ?? [];
      let tokenUsage = state.bootstrap?.tokenUsage;
      let floors = state.bootstrap?.floors ?? [];
      let workstations = state.bootstrap?.workstations ?? [];
      let hiringRequests = state.bootstrap?.hiringRequests ?? [];
      let environmentSuggestions = state.bootstrap?.environmentSuggestions ?? [];

      if (event.type === 'employee.state.changed' && event.actorId && typeof payload.to === 'string') {
        const nextState = normalizeActorState(payload.to);
        const dimensions = dimensionsForState(payload.to);
        employees = updateEmployee(employees, event.actorId, (employee) => ({
          ...employee,
          currentState: nextState,
          activity: dimensions.activity,
          device: dimensions.device,
          posture: employee.movement === 'WALKING' || employee.movement === 'PREPARING' ? 'WALKING' : 'SITTING',
        }));
      }

      if (event.type === 'employee.destination.changed' && event.actorId) {
        const rawDestination = payload.destination as Partial<Destination> | undefined;
        if (rawDestination?.id && rawDestination.kind) {
          employees = updateEmployee(employees, event.actorId, (employee) => ({
            ...employee,
            currentDestination: rawDestination as Destination,
          }));
        }
      }

      if (event.type === 'agent.model.profile.changed' && event.actorId) {
        const profileId = typeof payload.modelProfileId === 'string' ? payload.modelProfileId : undefined;
        const profile = profileId ? state.bootstrap?.modelProfiles.find((item) => item.id === profileId) : undefined;
        employees = updateEmployee(employees, event.actorId, (employee) => ({
          ...employee,
          modelProfileId: profileId,
          modelProfile: profile,
        }));
      }

      if (event.type === 'message.sent' || event.type === 'message.received') {
        const message = messageFromEvent(event);
        if (message && !messages.some((item) => item.id === message.id)) {
          messages = mergeMessages(messages, [message]);
          if (event.type === 'message.received' && payload.unread === true && event.actorId) {
            employees = updateEmployee(employees, event.actorId, (employee) => ({
              ...employee,
              unreadCount: employee.unreadCount + 1,
            }));
          }
        }
      }

      if (event.type === 'message.read' && typeof payload.employeeId === 'string') {
        employees = updateEmployee(employees, payload.employeeId, (employee) => ({ ...employee, unreadCount: 0 }));
      }

      if (event.type === 'llm.usage.recorded' && event.actorId) {
        const employeeUsage = tokenUsageFromPayload(payload.employeeUsage);
        const companyUsage = tokenUsageFromPayload(payload.companyUsage);
        if (employeeUsage) {
          employees = updateEmployee(employees, event.actorId, (employee) => ({
            ...employee,
            tokenUsage: employeeUsage,
          }));
        }
        if (companyUsage) tokenUsage = companyUsage;
      }

      if (event.type === 'agent.job.created' || event.type === 'agent.job.state.changed') {
        jobs = updateJobs(jobs, event);
      }

      const movement = movementFromEvent(event);
      if (movement && (event.type === 'AGENT_MOVE_REQUESTED' || event.type === 'AGENT_MOVE_STARTED')) {
        const nextMovement = {
          ...movement,
          movementStatus: event.type === 'AGENT_MOVE_STARTED' ? 'MOVING' : movement.movementStatus,
        };
        activeMovements = activeMovements.some((item) => item.id === nextMovement.id)
          ? activeMovements.map((item) => (item.id === nextMovement.id ? nextMovement : item))
          : [...activeMovements, nextMovement];
        employees = updateEmployee(employees, nextMovement.agentId, (employee) => ({
          ...employee,
          movement: event.type === 'AGENT_MOVE_STARTED' ? 'WALKING' : 'PREPARING',
          posture: event.type === 'AGENT_MOVE_STARTED' ? 'WALKING' : 'STANDING',
          activeMovementId: nextMovement.id,
          currentDestination: { kind: 'location', id: nextMovement.toLocationId },
        }));
      }

      if (event.type === 'AGENT_ARRIVED' && event.actorId) {
        const locationId = typeof payload.locationId === 'string' ? payload.locationId : movement?.toLocationId;
        const location = locationId ? state.bootstrap?.office.locations.find((item) => item.id === locationId) : undefined;
        activeMovements = movement
          ? activeMovements.filter((item) => item.id !== movement.id)
          : activeMovements.filter((item) => item.agentId !== event.actorId);
        employees = updateEmployee(employees, event.actorId, (employee) => ({
          ...employee,
          movement: 'ARRIVED',
          posture: typeof payload.posture === 'string' ? payload.posture : employee.posture,
          activeMovementId: null,
          currentDestination: undefined,
          locationId: locationId ?? employee.locationId,
          x: location?.x ?? employee.x,
          y: location?.y ?? employee.y,
          roomId: location?.roomId ?? employee.roomId,
        }));
      }

      if (event.type === 'AGENT_LOCATION_CHANGED' && event.actorId && typeof payload.locationId === 'string') {
        const location = state.bootstrap?.office.locations.find((item) => item.id === payload.locationId);
        employees = updateEmployee(employees, event.actorId, (employee) => ({
          ...employee,
          locationId: payload.locationId as string,
          x: location?.x ?? employee.x,
          y: location?.y ?? employee.y,
          roomId: typeof payload.roomId === 'string' ? payload.roomId : location?.roomId ?? employee.roomId,
        }));
      }

      if (event.type === 'AGENT_SAT_DOWN' && event.actorId) {
        employees = updateEmployee(employees, event.actorId, (employee) => ({ ...employee, posture: 'SITTING' }));
      }

      if (event.type === 'MEETING_STARTED' && typeof payload.meetingId === 'string') {
        meetings = meetings.map((meeting) => (meeting.id === payload.meetingId ? { ...meeting, status: 'IN_PROGRESS' } : meeting));
      }
      if (event.type === 'MEETING_FINISHED' && typeof payload.meetingId === 'string') {
        meetings = meetings.map((meeting) => (meeting.id === payload.meetingId ? { ...meeting, status: 'FINISHED' } : meeting));
      }
      if (event.type === 'MEETING_PARTICIPANT_ON_THE_WAY' || event.type === 'MEETING_PARTICIPANT_ARRIVED') {
        const meetingId = typeof payload.meetingId === 'string' ? payload.meetingId : undefined;
        const agentId = typeof payload.agentId === 'string' ? payload.agentId : event.actorId;
        const status = event.type === 'MEETING_PARTICIPANT_ARRIVED' ? 'WAITING' : 'ON_THE_WAY';
        meetings = meetings.map((meeting) => meeting.id !== meetingId || !agentId
          ? meeting
          : {
            ...meeting,
            participants: meeting.participants.map((participant) => participant.agentId === agentId
              ? { ...participant, status }
              : participant),
          });
      }

      if ((event.type === 'floor.created' || event.type === 'floor.updated') && payload.floor && typeof payload.floor === 'object') {
        const floor = payload.floor as FloorSummary;
        floors = floors.some((item) => item.id === floor.id)
          ? floors.map((item) => (item.id === floor.id ? floor : item))
          : [...floors, floor].sort((left, right) => left.floorNumber - right.floorNumber);
      }
      if (event.type === 'floor.deleted' && typeof payload.floorId === 'string') {
        floors = floors.filter((floor) => floor.id !== payload.floorId);
      }
      if (event.type === 'workstation.created' && payload.workstation && typeof payload.workstation === 'object') {
        const workstation = payload.workstation as typeof workstations[number];
        if (workstation.floorId === state.activeFloorId) {
          workstations = workstations.some((item) => item.id === workstation.id)
            ? workstations.map((item) => (item.id === workstation.id ? workstation : item))
            : [...workstations, workstation];
        }
      }
      if ((event.type === 'hiring.request.created' || event.type === 'hiring.candidates.ready' || event.type === 'hiring.candidates.refreshed') && payload.request && typeof payload.request === 'object') {
        const request = payload.request as HiringRequestSummary;
        hiringRequests = hiringRequests.some((item) => item.id === request.id)
          ? hiringRequests.map((item) => (item.id === request.id ? request : item))
          : [request, ...hiringRequests];
      }
      if (event.type === 'employee.hired' && payload.employee && typeof payload.employee === 'object') {
        const hiredEmployee = normalizeEmployees([payload.employee as Employee])[0];
        employees = employees.some((employee) => employee.id === hiredEmployee.id)
          ? employees.map((employee) => (employee.id === hiredEmployee.id ? hiredEmployee : employee))
          : [...employees, hiredEmployee];
      }
      if (event.type === 'runtime.environment.suggested' && event.actorId && Array.isArray(payload.changes)) {
        const incoming = payload.changes.flatMap((raw, index): RuntimeEnvironmentSuggestionSummary[] => {
          if (!raw || typeof raw !== 'object') return [];
          const change = raw as Record<string, unknown>;
          if (typeof change.ecosystem !== 'string' || typeof change.packageName !== 'string') return [];
          return [{
            id: `${event.eventId}-${index}`,
            companyId: event.companyId,
            employeeId: event.actorId ?? '',
            runtimeRunId: event.correlationId,
            ecosystem: change.ecosystem,
            packageName: change.packageName,
            requestedSpec: typeof change.requestedSpec === 'string' ? change.requestedSpec : null,
            sourceCommand: '',
            status: 'SUGGESTED',
            createdAt: event.timestamp,
            updatedAt: event.timestamp,
          }];
        });
        const existingKeys = new Set(environmentSuggestions.map((item) => `${item.employeeId}:${item.ecosystem}:${item.packageName}`));
        environmentSuggestions = [...incoming.filter((item) => !existingKeys.has(`${item.employeeId}:${item.ecosystem}:${item.packageName}`)), ...environmentSuggestions];
      }

      const terminationEvents = new Set([
        'EMPLOYEE_TERMINATION_STARTED',
        'EMPLOYEE_RUNTIME_DRAINING',
        'EMPLOYEE_PERMISSIONS_REVOKED',
        'EMPLOYEE_LEAVING_OFFICE',
        'EMPLOYEE_LEFT_OFFICE',
        'EMPLOYEE_TERMINATED',
      ]);
      if (terminationEvents.has(event.type) && event.actorId) {
        employees = updateEmployee(employees, event.actorId, (employee) => ({
          ...employee,
          employmentStatus: event.type === 'EMPLOYEE_TERMINATED' || event.type === 'EMPLOYEE_LEFT_OFFICE' ? 'TERMINATED' : 'TERMINATING',
          status: event.type === 'EMPLOYEE_TERMINATED' || event.type === 'EMPLOYEE_LEFT_OFFICE' ? 'offline' : 'offboarding',
          presence: event.type === 'EMPLOYEE_TERMINATED' || event.type === 'EMPLOYEE_LEFT_OFFICE' ? 'OFFLINE' : 'BUSY',
        }));
      }
      if (event.type === 'EMPLOYEE_TERMINATION_CANCELLED' && event.actorId) {
        employees = updateEmployee(employees, event.actorId, (employee) => ({
          ...employee,
          employmentStatus: 'ACTIVE',
          status: 'online',
          presence: 'AVAILABLE',
          currentState: 'idle',
          activity: 'IDLE',
          movement: 'STATIONARY',
        }));
      }

      const bootstrap = state.bootstrap
        ? {
          ...state.bootstrap,
          employees,
          messages,
          jobs,
          activeMovements,
          meetings,
          floors,
          workstations,
          hiringRequests,
          environmentSuggestions,
          tokenUsage: tokenUsage ?? state.bootstrap.tokenUsage,
          serverTime: event.timestamp,
        }
        : state.bootstrap;
      return { bootstrap, employees, messages, activeMovements, serverTime: event.timestamp, events: nextEvents };
    });
  },

  selectEmployee: (employeeId) => set({ selectedEmployeeId: employeeId }),

  openPhone: async (employeeId) => {
    set({ selectedEmployeeId: employeeId, phoneEmployeeId: employeeId });
    await Promise.all([get().loadConversation(employeeId), get().markRead(employeeId)]);
  },

  closePhone: () => set({ phoneEmployeeId: null }),

  loadConversation: async (employeeId) => {
    set((state) => {
      const previous = state.messageHistory[employeeId];
      return {
        messageHistory: {
          ...state.messageHistory,
          [employeeId]: {
            oldestCreatedAt: previous?.oldestCreatedAt ?? null,
            hasMore: previous?.hasMore ?? true,
            loading: true,
          },
        },
      };
    });
    try {
      const messages = await fetchEmployeeMessages(employeeId);
      set((state) => {
        const mergedMessages = mergeMessages(state.messages, messages);
        return {
          messages: mergedMessages,
          bootstrap: state.bootstrap ? { ...state.bootstrap, messages: mergedMessages } : state.bootstrap,
          messageHistory: {
            ...state.messageHistory,
            [employeeId]: {
              oldestCreatedAt: messages[0]?.createdAt ?? null,
              hasMore: messages.length === EMPLOYEE_MESSAGE_PAGE_SIZE,
              loading: false,
            },
          },
        };
      });
    } catch (error) {
      set((state) => ({
        error: error instanceof Error ? error.message : 'Unable to load conversation',
        messageHistory: {
          ...state.messageHistory,
          [employeeId]: {
            ...(state.messageHistory[employeeId] ?? { oldestCreatedAt: null, hasMore: true }),
            loading: false,
          },
        },
      }));
    }
  },

  loadOlderConversation: async (employeeId) => {
    const history = get().messageHistory[employeeId];
    if (!history?.hasMore || history.loading || !history.oldestCreatedAt) return false;
    set((state) => ({
      messageHistory: {
        ...state.messageHistory,
        [employeeId]: { ...history, loading: true },
      },
    }));
    try {
      const messages = await fetchEmployeeMessages(employeeId, history.oldestCreatedAt);
      set((state) => {
        const mergedMessages = mergeMessages(state.messages, messages);
        const current = state.messageHistory[employeeId] ?? history;
        return {
          messages: mergedMessages,
          bootstrap: state.bootstrap ? { ...state.bootstrap, messages: mergedMessages } : state.bootstrap,
          messageHistory: {
            ...state.messageHistory,
            [employeeId]: {
              oldestCreatedAt: messages[0]?.createdAt ?? current.oldestCreatedAt,
              hasMore: messages.length === EMPLOYEE_MESSAGE_PAGE_SIZE,
              loading: false,
            },
          },
        };
      });
      return messages.length > 0;
    } catch (error) {
      set((state) => ({
        error: error instanceof Error ? error.message : 'Unable to load older messages',
        messageHistory: {
          ...state.messageHistory,
          [employeeId]: { ...history, loading: false },
        },
      }));
      return false;
    }
  },

  sendAgentMessage: async (employeeId, content) => {
    try {
      const response: MessageSendResponse = await sendEmployeeMessage(employeeId, content);
      set((state) => ({
        messages: mergeMessages(state.messages, [response.message]),
        bootstrap: state.bootstrap ? { ...state.bootstrap, messages: mergeMessages(state.bootstrap.messages, [response.message]) } : state.bootstrap,
      }));
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to send agent message' });
      throw error;
    }
  },

  sendMeetingMessage: async (meetingId, content) => {
    try {
      const response = await sendMeetingMessage(meetingId, content);
      set((state) => {
        const messages = mergeMessages(state.messages, response.messages);
        return {
          messages,
          bootstrap: state.bootstrap ? { ...state.bootstrap, messages } : state.bootstrap,
        };
      });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to send meeting message' });
      throw error;
    }
  },

  markRead: async (employeeId) => {
    try {
      await markEmployeeMessagesRead(employeeId);
      set((state) => {
        const employees = updateEmployee(state.employees, employeeId, (employee) => ({ ...employee, unreadCount: 0 }));
        const messages = state.messages.map((message) => (
          message.fromType === 'employee' && message.fromId === employeeId && message.toId === 'user'
            ? { ...message, readAt: message.readAt ?? new Date().toISOString() }
            : message
        ));
        return { employees, messages, bootstrap: state.bootstrap ? { ...state.bootstrap, employees, messages } : state.bootstrap };
      });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to mark messages read' });
    }
  },

  loadMemory: async (employeeId) => {
    try {
      const memory = await fetchEmployeeMemory(employeeId);
      set((state) => ({ memories: { ...state.memories, [employeeId]: memory } }));
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to load agent memory' });
    }
  },

  saveProvider: async (input, profileId) => {
    try {
      const updateInput: ModelProfileUpdate = input.apiKey?.trim()
        ? input
        : (({ apiKey: _apiKey, ...withoutKey }) => withoutKey)(input);
      const provider = profileId
        ? await updateModelProfile(profileId, updateInput)
        : await createModelProfile(input);
      set((state) => {
        const bootstrap = replaceProvider(state.bootstrap, provider);
        const employees = state.employees.map((employee) => (
          employee.modelProfileId === provider.id ? { ...employee, modelProfile: provider } : employee
        ));
        return { bootstrap, employees };
      });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to save model provider' });
      throw error;
    }
  },

  testProvider: async (profileId) => {
    try {
      const result = await testModelProfile(profileId);
      return result.message;
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to test model provider' });
      throw error;
    }
  },

  bindEmployeeModel: async (employeeId, profileId) => {
    try {
      const employee = await bindEmployeeModelProfile(employeeId, profileId);
      set((state) => {
        const employees = state.employees.map((item) => (item.id === employee.id ? {
          ...employee,
          currentState: normalizeActorState(employee.currentState),
          unreadCount: employee.unreadCount ?? 0,
        } : item));
        return { employees, bootstrap: state.bootstrap ? { ...state.bootstrap, employees } : state.bootstrap };
      });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to bind model provider' });
      throw error;
    }
  },

  sendMessage: async (content) => {
    try {
      const response = await sendCompanyMessage(content);
      set((state) => ({ messages: mergeMessages(state.messages, [response.message]) }));
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to send company message' });
    }
  },

  startMeeting: async (participantIds) => {
    try {
      await createMeeting({
        title: '楼层协作会议',
        participantIds,
        meetingType: 'TEAM',
      });
      await get().loadBootstrap();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to create meeting' });
      throw error;
    }
  },

  finishMeeting: async (meetingId) => {
    try {
      await finishMeeting(meetingId);
      await get().loadBootstrap();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to finish meeting' });
      throw error;
    }
  },

  createFloor: async (input) => {
    try {
      const floor = await createFloorApi(input);
      await get().loadFloor(floor.id);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to create floor' });
      throw error;
    }
  },

  updateFloor: async (floorId, input) => {
    try {
      await updateFloorApi(floorId, input);
      await get().loadFloor(get().activeFloorId ?? floorId);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to update floor' });
      throw error;
    }
  },

  deleteFloor: async (floorId) => {
    try {
      await deleteFloorApi(floorId);
      const nextFloorId = get().bootstrap?.floors.find((floor) => floor.id !== floorId)?.id;
      await get().loadBootstrap(nextFloorId);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to delete floor' });
      throw error;
    }
  },

  createWorkstation: async (floorId) => {
    try {
      await createWorkstationApi(floorId);
      await get().loadFloor(floorId);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to create workstation' });
      throw error;
    }
  },

  hireCandidate: async (requestId, candidateId, workstationId) => {
    try {
      const result = await hireCandidateApi(requestId, { candidateId, workstationId });
      await get().loadFloor(result.floorId);
      return result;
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to hire candidate' });
      throw error;
    }
  },

  refreshHiringCandidates: async (requestId) => {
    try {
      const request = await refreshHiringCandidatesApi(requestId);
      set((state) => {
        if (!state.bootstrap) return state;
        const hiringRequests = state.bootstrap.hiringRequests.map((item) => (
          item.id === request.id ? request : item
        ));
        return { bootstrap: { ...state.bootstrap, hiringRequests } };
      });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to refresh candidates' });
      throw error;
    }
  },

  loadTerminationPreview: async (employeeId) => {
    try {
      const preview = await fetchTerminationPreview(employeeId);
      set({ terminationPreview: preview });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to load termination preview' });
      throw error;
    }
  },

  clearTerminationPreview: () => set({ terminationPreview: null }),

  terminateEmployee: async (employeeId, input) => {
    try {
      await terminateEmployeeApi(employeeId, input);
      set({ terminationPreview: null });
      await get().loadBootstrap(get().activeFloorId ?? undefined);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to terminate employee' });
      throw error;
    }
  },

  cancelEmployeeTermination: async (employeeId) => {
    try {
      await cancelEmployeeTerminationApi(employeeId);
      await get().loadBootstrap(get().activeFloorId ?? undefined);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to cancel termination' });
      throw error;
    }
  },

  testOpenClaw: async () => {
    try {
      const status = await fetchOpenClawStatus();
      set((state) => ({
        bootstrap: state.bootstrap ? { ...state.bootstrap, openclaw: status } : state.bootstrap,
      }));
      return status;
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to test OpenClaw Gateway' });
      throw error;
    }
  },

  provisionEmployeeRuntime: async (employeeId) => {
    try {
      const employee = normalizeEmployees([await provisionEmployeeRuntimeApi(employeeId)])[0];
      set((state) => {
        const employees = state.employees.map((item) => (item.id === employee.id ? employee : item));
        return { employees, bootstrap: state.bootstrap ? { ...state.bootstrap, employees } : state.bootstrap };
      });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to provision OpenClaw runtime' });
      throw error;
    }
  },

  provisionEmployeeWorkspace: async (employeeId) => {
    try {
      await provisionEmployeeWorkspaceApi(employeeId);
      await get().loadBootstrap(get().activeFloorId ?? undefined);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to provision employee workspace' });
      throw error;
    }
  },
}));

export function selectedEmployee(employees: Employee[], selectedEmployeeId: string | null): Employee | null {
  return employees.find((employee) => employee.id === selectedEmployeeId) ?? employees[0] ?? null;
}

export type { AgentMemoryRecord, BootstrapResponse };
