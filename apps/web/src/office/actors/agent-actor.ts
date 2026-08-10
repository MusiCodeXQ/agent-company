import { Container, Graphics, Rectangle, Text } from 'pixi.js';
import type { ActorState, Destination, Employee, OfficeLayout } from '../../domain/types';

const STATE_LABELS: Record<ActorState, string> = {
  idle: '待命',
  walking: '移动中',
  checking_phone: '看手机',
  thinking: '思考中',
  coding: '编码中',
  designing: '设计中',
  testing: '测试中',
  reviewing: 'Review',
  meeting: '会议中',
  interviewing: '面试中',
  waiting: '等待',
  coffee: '茶水间',
  restroom: '卫生间',
  error: '异常',
  offline: '离线',
};

const STATE_COLORS: Record<ActorState, number> = {
  idle: 0x9aa7b8,
  walking: 0x72b7ff,
  checking_phone: 0xf6c85f,
  thinking: 0xc3a6ff,
  coding: 0x5ce1b8,
  designing: 0xf292c8,
  testing: 0x64d3f4,
  reviewing: 0xffa76b,
  meeting: 0x7dc4ff,
  interviewing: 0xffca7a,
  waiting: 0xb5bdc9,
  coffee: 0xd6a15b,
  restroom: 0x8392a6,
  error: 0xff6b78,
  offline: 0x5b6470,
};

const FALLBACK_COLOR = 0x6e7f93;

interface Point {
  x: number;
  y: number;
}

type ActorEmployee = Employee;

function parseColor(value: string | undefined): number {
  if (!value) return FALLBACK_COLOR;
  const parsed = Number.parseInt(value.replace('#', ''), 16);
  return Number.isFinite(parsed) ? parsed : FALLBACK_COLOR;
}

export class AgentActor extends Container {
  private employee: ActorEmployee;
  private office: OfficeLayout;
  private readonly visual = new Container();
  private readonly character = new Container();
  private readonly shadow = new Graphics();
  private readonly phone = new Graphics();
  private readonly selection = new Graphics();
  private readonly stateBadge: Text;
  private readonly nameLabel: Text;
  private path: Point[] = [];
  private state: ActorState;
  private destinationKey = '';
  private elapsedMs = 0;
  private readonly onSelect: (employeeId: string) => void;

  constructor(employee: ActorEmployee, office: OfficeLayout, onSelect: (employeeId: string) => void) {
    super();
    this.employee = employee;
    this.office = office;
    this.state = employee.currentState;
    this.onSelect = onSelect;
    this.eventMode = 'static';
    this.cursor = 'pointer';
    this.hitArea = new Rectangle(-28, -92, 56, 110);
    this.addChild(this.visual);
    this.visual.addChild(this.shadow, this.character, this.phone, this.selection);
    this.stateBadge = new Text({ text: STATE_LABELS[this.state], style: { fill: STATE_COLORS[this.state], fontSize: 11, fontWeight: '700' } });
    this.nameLabel = new Text({ text: employee.name, style: { fill: 0xeaf2fb, fontSize: 11, fontWeight: '600' } });
    this.stateBadge.anchor.set(0.5, 1);
    this.nameLabel.anchor.set(0.5, 0);
    this.addChild(this.stateBadge, this.nameLabel);
    this.on('pointertap', () => this.onSelect(this.employee.id));
    this.redrawAppearance();
    this.setPosition(this.resolveDeskPosition(employee));
    this.refreshVisuals();
  }

  syncEmployee(employee: ActorEmployee, office: OfficeLayout, selected: boolean): void {
    const stateChanged = this.state !== employee.currentState;
    const nextDestinationKey = destinationKey(employee.currentDestination);
    const destinationChanged = nextDestinationKey !== this.destinationKey;
    const appearanceChanged = this.employee.appearance.seed !== employee.appearance.seed;
    this.employee = employee;
    this.office = office;
    this.state = employee.currentState;
    this.destinationKey = nextDestinationKey;

    if (appearanceChanged) {
      this.redrawAppearance();
    }
    if ((stateChanged || destinationChanged) && this.state === 'walking' && employee.currentDestination) {
      this.path = this.routeTo(this.resolveDestination(employee.currentDestination));
    }
    if (stateChanged && this.state === 'coding' && employee.currentDestination?.kind === 'desk') {
      this.path = [];
      this.setPosition(this.resolveDestination(employee.currentDestination));
    }
    this.selection.visible = selected;
    this.refreshVisuals();
  }

  update(deltaMs: number): void {
    this.elapsedMs += deltaMs;
    const isMoving = (this.state === 'walking' || this.state === 'meeting') && this.path.length > 0;
    if (isMoving) {
      this.moveAlongPath(deltaMs / 1000);
    }
    const bobAmount = this.state === 'walking' ? 1.4 : 0.7;
    this.visual.y = Math.sin(this.elapsedMs / 260) * bobAmount;
    this.zIndex = Math.round(this.y);
  }

  private redrawAppearance(): void {
    this.character.removeChildren().forEach((child) => child.destroy());
    this.shadow.clear().ellipse(0, 16, 19, 6).fill({ color: 0x101722, alpha: 0.3 });
    const appearance = this.employee.appearance;
    const bodyWidth = appearance.bodyType === 'slim' ? 10 : appearance.bodyType === 'broad' ? 15 : 12;
    const body = new Graphics();
    body.roundRect(-bodyWidth, -35, bodyWidth * 2, 29, 7).fill(parseColor(appearance.topColor));
    body.rect(-bodyWidth + 2, -9, 7, 19).fill(parseColor(appearance.bottomColor));
    body.rect(bodyWidth - 9, -9, 7, 19).fill(parseColor(appearance.bottomColor));
    body.roundRect(-12, 8, 9, 4, 2).fill(parseColor(appearance.shoeColor));
    body.roundRect(3, 8, 9, 4, 2).fill(parseColor(appearance.shoeColor));
    body.rect(-3, -39, 6, 8).fill(parseColor(appearance.skinTone));

    const head = new Graphics();
    head.circle(0, -50, 14).fill(parseColor(appearance.skinTone));
    this.drawHair(head, appearance.hairStyle, parseColor(appearance.hairColor));
    if (appearance.beardStyle) {
      head.arc(0, -48, 8, 0.25, Math.PI - 0.25).stroke({ color: parseColor(appearance.hairColor), width: 3, cap: 'round' });
    }
    if (appearance.glasses) {
      head.circle(-5, -51, 4).stroke({ color: 0x26303d, width: 1.5 });
      head.circle(5, -51, 4).stroke({ color: 0x26303d, width: 1.5 });
      head.moveTo(-1, -51).lineTo(1, -51).stroke({ color: 0x26303d, width: 1.2 });
    }
    if (appearance.accessory === 'headphones') {
      head.arc(0, -51, 17, Math.PI + 0.2, Math.PI * 2 - 0.2).stroke({ color: 0x8ed1ff, width: 2 });
      head.roundRect(-17, -52, 4, 9, 2).fill(0x8ed1ff);
      head.roundRect(13, -52, 4, 9, 2).fill(0x8ed1ff);
    }
    this.character.addChild(body, head);
    this.character.scale.set(appearance.heightScale);
    if (appearance.accessory === 'badge') {
      const badge = new Graphics();
      badge.roundRect(5, -29, 5, 7, 1).fill(0xf5d26e);
      this.character.addChild(badge);
    }
    if (appearance.accessory === 'watch') {
      const watch = new Graphics();
      watch.circle(-11, -12, 2.5).fill(0xc8d4e3);
      this.character.addChild(watch);
    }
  }

  private drawHair(graphics: Graphics, style: string, color: number): void {
    if (style === 'bob' || style === 'wave') {
      graphics.circle(0, -55, 16).fill(color);
      graphics.roundRect(-16, -57, 5, 20, 3).fill(color);
      graphics.roundRect(11, -57, 5, 20, 3).fill(color);
      return;
    }
    if (style === 'curly') {
      graphics.circle(-8, -60, 7).fill(color);
      graphics.circle(0, -63, 8).fill(color);
      graphics.circle(8, -60, 7).fill(color);
      return;
    }
    graphics.roundRect(-14, -64, 28, 13, 7).fill(color);
    if (style === 'side_part') {
      graphics.moveTo(0, -64).lineTo(7, -54).stroke({ color: 0x9b7359, width: 1.5 });
    }
  }

  private refreshVisuals(): void {
    const label = STATE_LABELS[this.state] ?? this.state;
    this.stateBadge.text = label;
    this.stateBadge.style.fill = STATE_COLORS[this.state] ?? STATE_COLORS.idle;
    this.stateBadge.position.set(0, -70);
    this.nameLabel.position.set(0, 18);
    this.phone.clear();
    if (this.state === 'checking_phone') {
      this.phone.roundRect(10, -28, 7, 12, 2).fill(0x273342).stroke({ color: 0xf6c85f, width: 1 });
      this.phone.circle(13.5, -25, 1.5).fill(0xf6c85f);
      this.stateBadge.text = '● 看手机';
    }
    if (this.state === 'thinking') {
      this.stateBadge.text = '… 思考中';
    }
    if (this.state === 'coding') {
      this.stateBadge.text = '● 编码中';
    }
    this.selection.clear().roundRect(-24, -77, 48, 96, 9).stroke({ color: 0xf6c85f, width: 2, alpha: 0.9 });
  }

  private moveAlongPath(deltaSeconds: number): void {
    const next = this.path[0];
    if (!next) return;
    const dx = next.x - this.x;
    const dy = next.y - this.y;
    const distance = Math.hypot(dx, dy);
    const step = 300 * deltaSeconds;
    if (distance <= step) {
      this.setPosition(next);
      this.path.shift();
      return;
    }
    this.setPosition({ x: this.x + (dx / distance) * step, y: this.y + (dy / distance) * step });
  }

  private routeTo(target: Point): Point[] {
    const points: Point[] = [];
    if (target.x > 1110 && this.x < 1110) {
      points.push({ x: 1080, y: 350 }, { x: 1128, y: target.y });
    } else if (target.x < 1110 && this.x > 1110) {
      points.push({ x: 1128, y: 350 }, { x: 1080, y: 350 });
    }
    points.push(target);
    return points;
  }

  private resolveDeskPosition(employee: ActorEmployee): Point {
    const desk = this.office.desks.find((item) => item.id === employee.deskId || item.employeeId === employee.id);
    const seat = desk ? this.office.seats.find((item) => item.id === desk.seatId) : undefined;
    return seat ? { x: seat.x, y: seat.y } : { x: 500, y: 500 };
  }

  private resolveDestination(destination: Destination): Point {
    if (destination.kind === 'desk') {
      const desk = this.office.desks.find((item) => item.id === destination.id);
      const seat = desk ? this.office.seats.find((item) => item.id === desk.seatId) : undefined;
      return seat ? { x: seat.x, y: seat.y } : this.resolveDeskPosition(this.employee);
    }
    if (destination.kind === 'room') {
      const assignedSeat = this.office.seats.find((item) => item.roomId === destination.id && item.employeeId === this.employee.id);
      if (assignedSeat) return { x: assignedSeat.x, y: assignedSeat.y };
      const room = this.office.rooms.find((item) => item.id === destination.id);
      return room ? { x: room.x + room.width / 2, y: room.y + room.height / 2 } : { x: 1280, y: 245 };
    }
    return { x: 85, y: 640 };
  }

  private setPosition(point: Point): void {
    this.position.set(point.x, point.y);
  }
}

function destinationKey(destination?: Destination): string {
  return destination ? `${destination.kind}:${destination.id}` : '';
}
