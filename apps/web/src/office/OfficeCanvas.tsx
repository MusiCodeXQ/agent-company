import { Application } from 'pixi.js';
import { useEffect, useRef } from 'react';
import type { ReactElement } from 'react';
import { defaultOffice } from './layout/default-office';
import { OfficeEngine } from './engine/office-engine';
import { useCompanyStore } from '../store/company-store';

export function OfficeCanvas({ onMeetingTableClick }: { onMeetingTableClick?: () => void }): ReactElement {
  const containerRef = useRef<HTMLDivElement>(null);
  const engineRef = useRef<OfficeEngine | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return undefined;
    let disposed = false;
    let initialized = false;
    let unsubscribe: (() => void) | undefined;
    let resizeObserver: ResizeObserver | undefined;
    let engine: OfficeEngine | undefined;
    const app = new Application();

    const initialize = async (): Promise<void> => {
      try {
        await app.init({
          width: Math.max(container.clientWidth, 1),
          height: Math.max(container.clientHeight, 1),
          background: '#0b1114',
          antialias: true,
          autoDensity: true,
          resolution: Math.min(window.devicePixelRatio || 1, 2),
        });
        initialized = true;
        if (disposed) {
          app.destroy(true);
          return;
        }

        container.appendChild(app.canvas);
        engine = new OfficeEngine(
          app,
          (employeeId) => {
            useCompanyStore.getState().selectEmployee(employeeId);
          },
          (employeeId) => {
            void useCompanyStore.getState().openPhone(employeeId);
          },
          () => onMeetingTableClick?.(),
        );
        engineRef.current = engine;

        const resize = (): void => {
          if (!initialized || disposed) return;
          app.renderer.resize(Math.max(container.clientWidth, 1), Math.max(container.clientHeight, 1));
          engine?.resize();
        };
        resizeObserver = new ResizeObserver(resize);
        resizeObserver.observe(container);
        resize();

        const state = useCompanyStore.getState();
        engine.setOffice(state.bootstrap?.office ?? defaultOffice);
        engine.setEmployees(state.employees);
        engine.setMovements(state.activeMovements, state.serverTime);
        engine.setSelectedEmployee(state.selectedEmployeeId);
        engine.setPhoneEmployee(state.phoneEmployeeId);

        unsubscribe = useCompanyStore.subscribe((nextState) => {
          engine?.setOffice(nextState.bootstrap?.office ?? defaultOffice);
          engine?.setEmployees(nextState.employees);
          engine?.setMovements(nextState.activeMovements, nextState.serverTime);
          engine?.setSelectedEmployee(nextState.selectedEmployeeId);
          engine?.setPhoneEmployee(nextState.phoneEmployeeId);
        });
      } catch (error) {
        if (!disposed) console.error('Unable to initialize the office renderer', error);
      }
    };

    void initialize();
    return () => {
      disposed = true;
      resizeObserver?.disconnect();
      unsubscribe?.();
      engine?.destroy();
      engineRef.current = null;
      if (initialized) {
        try {
          app.destroy(true);
        } catch (error) {
          console.warn('Office renderer cleanup failed', error);
        }
      }
    };
  }, []);

  return <div ref={containerRef} className="office-canvas" aria-label="Procedurally rendered virtual office" />;
}
