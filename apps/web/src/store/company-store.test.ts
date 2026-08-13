import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useCompanyStore } from './company-store';

describe('company store phone state', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('[]', {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    })));
    useCompanyStore.setState({
      selectedEmployeeId: null,
      phoneEmployeeId: null,
      messages: [],
      messageHistory: {},
      employees: [],
      error: null,
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('keeps phone state for the same employee and clears it on another selection', async () => {
    await useCompanyStore.getState().openPhone('employee-01');
    expect(useCompanyStore.getState().phoneEmployeeId).toBe('employee-01');

    useCompanyStore.getState().selectEmployee('employee-01');
    expect(useCompanyStore.getState().phoneEmployeeId).toBe('employee-01');

    useCompanyStore.getState().selectEmployee('employee-02');
    expect(useCompanyStore.getState().phoneEmployeeId).toBeNull();
  });

  it('clears phone state when the phone panel closes', async () => {
    await useCompanyStore.getState().openPhone('employee-01');
    useCompanyStore.getState().closePhone();
    expect(useCompanyStore.getState().phoneEmployeeId).toBeNull();
  });
});
