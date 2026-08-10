import { create } from 'zustand';
import { fetchBootstrap, openCompanyWebSocket, sendCompanyMessage, triggerDemo } from '../api/client';
import type { ActorState, BootstrapResponse, CompanyStoreState, Destination, DomainEvent, Employee } from '../domain/types';

interface CompanyStoreActions {
  loadBootstrap: () => Promise<void>;
  connectWebSocket: () => () => void;
  applyEvent: (event: DomainEvent) => void;
  selectEmployee: (employeeId: string | null) => void;
  runDemo: () => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
}

type CompanyStore = CompanyStoreState & CompanyStoreActions;

function updateEmployee(employees: Employee[], employeeId: string, update: (employee: Employee) => Employee): Employee[] {
  return employees.map((employee) => (employee.id === employeeId ? update(employee) : employee));
}

export const useCompanyStore = create<CompanyStore>((set, get) => ({
  bootstrap: null,
  employees: [],
  events: [],
  selectedEmployeeId: null,
  loading: true,
  error: null,
  connectionStatus: 'connecting',

  loadBootstrap: async () => {
    try {
      const bootstrap = await fetchBootstrap();
      set({
        bootstrap,
        employees: bootstrap.employees,
        events: bootstrap.recentEvents,
        loading: false,
        error: null,
      });
    } catch (error) {
      set({ loading: false, error: error instanceof Error ? error.message : 'Unable to load company' });
    }
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

      if (event.type === 'employee.state.changed' && event.actorId && typeof payload.to === 'string') {
        const nextState = payload.to as ActorState;
        employees = updateEmployee(employees, event.actorId, (employee) => ({
          ...employee,
          currentState: nextState,
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

      return { employees, events: nextEvents };
    });
  },

  selectEmployee: (employeeId) => set({ selectedEmployeeId: employeeId }),

  runDemo: async () => {
    try {
      await triggerDemo();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to start demo' });
    }
  },

  sendMessage: async (content) => {
    try {
      const event = await sendCompanyMessage(content);
      get().applyEvent(event);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Unable to send message' });
    }
  },
}));

export function selectedEmployee(employees: Employee[], selectedEmployeeId: string | null): Employee | null {
  return employees.find((employee) => employee.id === selectedEmployeeId) ?? employees[0] ?? null;
}

export type { BootstrapResponse };

