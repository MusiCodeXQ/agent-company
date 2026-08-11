const TOKEN_UNITS = [
  { value: 1_000_000_000_000_000, suffix: 'p' },
  { value: 1_000_000_000_000, suffix: 't' },
  { value: 1_000_000_000, suffix: 'b' },
  { value: 1_000_000, suffix: 'm' },
  { value: 1_000, suffix: 'k' },
] as const;

/** Compact token counts use stable English units instead of locale-specific 万/亿. */
export function formatTokens(value: number): string {
  const normalized = Math.max(0, Number.isFinite(value) ? value : 0);
  const unitIndex = TOKEN_UNITS.findIndex((unit) => normalized >= unit.value);
  if (unitIndex === -1) return Math.round(normalized).toLocaleString('zh-CN');

  let selectedIndex = unitIndex;
  let scaled = normalized / TOKEN_UNITS[selectedIndex].value;
  if (scaled >= 999.5 && selectedIndex > 0) {
    selectedIndex -= 1;
    scaled = normalized / TOKEN_UNITS[selectedIndex].value;
  }
  const precision = scaled >= 100 ? 0 : 1;
  const display = scaled.toFixed(precision).replace(/\.0$/, '');
  return `${display}${TOKEN_UNITS[selectedIndex].suffix}`;
}
