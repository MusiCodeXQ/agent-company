import type { AppearanceProfile, Gender } from './types';

class SeededRng {
  private state: number;

  constructor(seed: number) {
    this.state = ((seed >>> 0) * 1664525 + 1013904223) >>> 0;
  }

  next(): number {
    let state = this.state;
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    this.state = state >>> 0;
    return this.state;
  }

  pick<T>(values: readonly T[]): T {
    return values[this.next() % values.length];
  }
}

export function createAppearance(seed: number, gender: Gender, age: number): AppearanceProfile {
  const rng = new SeededRng(seed);
  const hairStyles = gender === 'female'
    ? (age >= 32 ? ['bob', 'wave', 'side_part', 'bob'] : ['bob', 'wave', 'curly', 'side_part', 'wave'])
    : gender === 'male'
      ? (age >= 32 ? ['short', 'side_part', 'short', 'side_part'] : ['short', 'side_part', 'curly', 'short'])
      : ['short', 'side_part', 'bob', 'wave', 'curly'];
  const bottomStyles = gender === 'female'
    ? ['trousers', 'jeans', 'skirt', 'trousers']
    : gender === 'male'
      ? ['trousers', 'jeans', 'trousers', 'jeans']
      : ['trousers', 'jeans', 'skirt'];
  const beardStyle = gender === 'male' && age >= 30 && rng.next() % 100 < 36
    ? rng.pick(['short', 'neat', 'stubble'])
    : undefined;

  return {
    seed,
    skinTone: rng.pick(['#F3C7A8', '#D99B76', '#B97852', '#8D573D', '#F0B98D']),
    hairStyle: rng.pick(hairStyles),
    hairColor: rng.pick(['#241A17', '#3A2821', '#5A3928', '#1B2430', '#7B4F35']),
    topStyle: rng.pick(['shirt', 'hoodie', 'sweater', 'jacket']),
    topColor: rng.pick(['#3E5C76', '#6B4E71', '#2E7866', '#C2704B', '#556B8A', '#9A6B3F']),
    bottomStyle: rng.pick(bottomStyles),
    bottomColor: rng.pick(['#26364A', '#39424E', '#5A4038', '#2C514B', '#6E5D45']),
    shoeColor: rng.pick(['#1D2026', '#F2EEE8', '#A35D3B', '#334B64']),
    bodyType: rng.pick(['slim', 'regular', 'broad']),
    heightScale: 0.94 + (rng.next() % 13) / 100,
    glasses: rng.next() % 100 < 30,
    beardStyle,
    accessory: rng.pick(['watch', 'headphones', 'badge', 'none']),
  };
}
