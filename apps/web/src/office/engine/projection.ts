export interface ScenePoint {
  x: number;
  y: number;
}

const SCENE_PADDING = 64;
const X_SKEW = 0.16;
const Y_SCALE = 0.72;

/**
 * Backend coordinates stay as the authoritative top-down simulation space.
 * This projection is visual-only and gives the Pixi scene a stable oblique 2.5D view.
 */
export function projectPoint(point: ScenePoint): ScenePoint {
  return {
    x: SCENE_PADDING + point.x + point.y * X_SKEW,
    y: SCENE_PADDING + point.y * Y_SCALE,
  };
}

export function projectFootprint(x: number, y: number, width: number, height: number): number[] {
  const topLeft = projectPoint({ x, y });
  const topRight = projectPoint({ x: x + width, y });
  const bottomRight = projectPoint({ x: x + width, y: y + height });
  const bottomLeft = projectPoint({ x, y: y + height });
  return [
    topLeft.x, topLeft.y,
    topRight.x, topRight.y,
    bottomRight.x, bottomRight.y,
    bottomLeft.x, bottomLeft.y,
  ];
}

export function projectedOfficeSize(width: number, height: number): ScenePoint {
  return {
    x: SCENE_PADDING * 2 + width + height * X_SKEW,
    y: SCENE_PADDING * 2 + height * Y_SCALE + 26,
  };
}

/** Pixi sortableChildren uses this to interleave furniture and actors. */
export function depthFor(logicalY: number, offset = 0): number {
  return logicalY * 100 + offset;
}
