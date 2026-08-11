import { describe, expect, it } from 'vitest';
import { formatTokens } from './format';

describe('formatTokens', () => {
  it('uses compact English token units', () => {
    expect(formatTokens(999)).toBe('999');
    expect(formatTokens(1_000)).toBe('1k');
    expect(formatTokens(12_300)).toBe('12.3k');
    expect(formatTokens(1_000_000)).toBe('1m');
    expect(formatTokens(1_000_000_000)).toBe('1b');
    expect(formatTokens(1_000_000_000_000)).toBe('1t');
    expect(formatTokens(1_000_000_000_000_000)).toBe('1p');
  });

  it('promotes values that round into the next unit', () => {
    expect(formatTokens(999_999)).toBe('1m');
    expect(formatTokens(999_999_999)).toBe('1b');
  });
});
