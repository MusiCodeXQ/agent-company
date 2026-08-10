import { describe, expect, it } from 'vitest';
import { depthFor, projectedOfficeSize, projectPoint } from './projection';

describe('office 2.5D projection', () => {
  it('projects logical coordinates deterministically without changing backend state', () => {
    expect(projectPoint({ x: 0, y: 0 })).toEqual({ x: 64, y: 64 });
    expect(projectPoint({ x: 100, y: 100 })).toEqual({ x: 180, y: 136 });
  });

  it('keeps scene bounds large enough for the projected office', () => {
    expect(projectedOfficeSize(1600, 900)).toEqual({ x: 1872, y: 802 });
  });

  it('sorts actors and furniture by authoritative logical depth', () => {
    expect(depthFor(320)).toBeGreaterThan(depthFor(170));
    expect(depthFor(288, 8)).toBeGreaterThan(depthFor(288));
    expect(depthFor(180 + 82, 8)).toBeLessThan(depthFor(288));
  });
});
