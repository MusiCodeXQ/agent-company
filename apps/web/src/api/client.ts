import type { BootstrapResponse, DomainEvent } from '../domain/types';

const COMPANY_ID = 'company-01';

export async function fetchBootstrap(): Promise<BootstrapResponse> {
  const response = await fetch('/api/bootstrap');
  if (!response.ok) {
    throw new Error(`Bootstrap request failed (${response.status})`);
  }
  return response.json() as Promise<BootstrapResponse>;
}

export async function triggerDemo(): Promise<void> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/demo`, { method: 'POST' });
  if (!response.ok && response.status !== 409) {
    throw new Error(`Demo request failed (${response.status})`);
  }
}

export async function sendCompanyMessage(content: string): Promise<DomainEvent> {
  const response = await fetch(`/api/companies/${COMPANY_ID}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content }),
  });
  if (!response.ok) {
    throw new Error(`Message request failed (${response.status})`);
  }
  return response.json() as Promise<DomainEvent>;
}

export function openCompanyWebSocket(onEvent: (event: DomainEvent) => void, onStatus: (status: 'connected' | 'disconnected') => void): () => void {
  const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
  const socket = new WebSocket(`${protocol}://${window.location.host}/ws?companyId=${COMPANY_ID}`);
  socket.addEventListener('open', () => onStatus('connected'));
  socket.addEventListener('close', () => onStatus('disconnected'));
  socket.addEventListener('message', (message) => {
    try {
      const value = JSON.parse(message.data) as Partial<DomainEvent> & { type?: string };
      if (value.type === 'connection.ready' || !value.eventId || !value.companyId || !value.timestamp) {
        return;
      }
      onEvent(value as DomainEvent);
    } catch {
      // Ignore malformed messages so one bad event cannot disconnect the office view.
    }
  });
  return () => socket.close();
}

