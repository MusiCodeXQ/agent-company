import { describe, expect, it } from 'vitest';
import { createAppearance } from './appearance';

describe('createAppearance', () => {
  it('returns the same appearance for the same persisted seed', () => {
    const first = createAppearance(1006, 'male', 34);
    const second = createAppearance(1006, 'male', 34);
    expect(second).toEqual(first);
  });

  it('keeps the generated appearance independent from a display rerender', () => {
    const first = createAppearance(1001, 'male', 36);
    const other = createAppearance(1002, 'female', 31);
    expect(first.seed).not.toBe(other.seed);
    expect(first.topColor).not.toBe(other.topColor);
  });
});

