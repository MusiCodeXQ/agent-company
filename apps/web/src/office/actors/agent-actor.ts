import { Container, Graphics, Rectangle, Text } from 'pixi.js';
import type { ActiveMovement, ActorState, Employee, OfficeLayout } from '../../domain/types';
import { depthFor, projectPoint } from '../engine/projection';
import {
  AgentAnimationController,
  type AgentAnimation,
  type AgentPose,
  type AgentRig,
} from './animation-controller';

const STATE_LABELS: Record<ActorState, string> = {
  idle: '待命',
  walking: '移动中',
  checking_phone: '看手机',
  thinking: '思考中',
  coding: '工作中',
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
  idle: 0xa5b6be,
  walking: 0x75c9ff,
  checking_phone: 0xffcf70,
  thinking: 0xcab0ff,
  coding: 0x72e1b6,
  designing: 0xf49dcc,
  testing: 0x71d7ee,
  reviewing: 0xffad76,
  meeting: 0x8bc8ff,
  interviewing: 0xffcb83,
  waiting: 0xb7c0ca,
  coffee: 0xd7a661,
  restroom: 0x8d9cab,
  error: 0xff7582,
  offline: 0x66717c,
};

const FALLBACK_COLOR = 0x6e7f93;

interface Point {
  x: number;
  y: number;
}

type TravelDirection = 'front' | 'back' | 'side';

function parseColor(value: string | undefined): number {
  if (!value) return FALLBACK_COLOR;
  const parsed = Number.parseInt(value.replace('#', ''), 16);
  return Number.isFinite(parsed) ? parsed : FALLBACK_COLOR;
}

function shade(color: number, amount: number): number {
  const clamp = (value: number): number => Math.max(0, Math.min(255, value));
  const red = clamp(((color >> 16) & 0xff) + amount);
  const green = clamp(((color >> 8) & 0xff) + amount);
  const blue = clamp((color & 0xff) + amount);
  return (red << 16) | (green << 8) | blue;
}

export class AgentActor extends Container {
  private employee: Employee;
  private office: OfficeLayout;
  private readonly selection = new Graphics();
  private readonly shadow = new Graphics();
  private readonly sprite = new Container();
  private readonly leftLeg = new Container();
  private readonly rightLeg = new Container();
  private readonly leftArm = new Container();
  private readonly torso = new Container();
  private readonly rightArm = new Container();
  private readonly head = new Container();
  private readonly phone = new Graphics();
  private backHead: Graphics | null = null;
  private readonly statusPlate = new Graphics();
  private readonly namePlate = new Graphics();
  private readonly stateBadge: Text;
  private readonly nameLabel: Text;
  private readonly rig: AgentRig;
  private state: ActorState;
  private movement: ActiveMovement | null = null;
  private logicalPosition: Point;
  private serverTimeOffsetMs = 0;
  private facing = 1;
  private travelDirection: TravelDirection = 'front';
  private readonly animation = new AgentAnimationController();
  private readonly onSelect: (employeeId: string) => void;
  private readonly onOpenPhone: (employeeId: string) => void;
  private lastTapAt = 0;
  private phoneOpen = false;

  constructor(
    employee: Employee,
    office: OfficeLayout,
    onSelect: (employeeId: string) => void,
    onOpenPhone: (employeeId: string) => void,
  ) {
    super();
    this.employee = employee;
    this.office = office;
    this.state = employee.currentState;
    this.onSelect = onSelect;
    this.onOpenPhone = onOpenPhone;
    this.logicalPosition = initialPosition(employee, office);
    this.facing = employee.appearance.seed % 2 === 0 ? 1 : -1;
    this.eventMode = 'static';
    this.cursor = 'pointer';
    this.hitArea = new Rectangle(-38, -112, 76, 138);

    this.sprite.addChild(
      this.leftLeg,
      this.rightLeg,
      this.leftArm,
      this.torso,
      this.rightArm,
      this.head,
      this.phone,
    );
    this.addChild(
      this.selection,
      this.shadow,
      this.sprite,
      this.statusPlate,
      this.namePlate,
    );
    this.stateBadge = new Text({
      text: STATE_LABELS[this.state],
      style: {
        fill: STATE_COLORS[this.state],
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
        fontSize: 10,
        fontWeight: '700',
        letterSpacing: 0.4,
      },
    });
    this.nameLabel = new Text({
      text: employee.name,
      style: {
        fill: 0xf4ead7,
        fontFamily: 'ui-sans-serif, system-ui, sans-serif',
        fontSize: 11,
        fontWeight: '700',
        dropShadow: { color: 0x050708, alpha: 0.8, blur: 2, distance: 1 },
      },
    });
    this.stateBadge.anchor.set(0.5);
    this.nameLabel.anchor.set(0.5);
    this.addChild(this.stateBadge, this.nameLabel);
    this.rig = {
      root: this.sprite,
      torso: this.torso,
      head: this.head,
      leftArm: this.leftArm,
      rightArm: this.rightArm,
      leftLeg: this.leftLeg,
      rightLeg: this.rightLeg,
      phone: this.phone,
    };

    this.on('pointertap', () => {
      const now = performance.now();
      this.onSelect(this.employee.id);
      if (now - this.lastTapAt <= 320) {
        this.onOpenPhone(this.employee.id);
        this.lastTapAt = 0;
      } else {
        this.lastTapAt = now;
      }
    });
    this.redrawAppearance();
    this.setLogicalPosition(this.logicalPosition);
    this.refreshVisuals();
  }

  syncEmployee(
    employee: Employee,
    office: OfficeLayout,
    selected: boolean,
    movement: ActiveMovement | null = null,
    serverTime?: string | null,
    phoneOpen = false,
  ): void {
    const stateChanged = this.state !== employee.currentState;
    const appearanceChanged = this.employee.appearance.seed !== employee.appearance.seed
      || this.employee.gender !== employee.gender;
    const movementChanged = this.movement?.id !== movement?.id
      || this.movement?.movementStatus !== movement?.movementStatus
      || this.movement?.estimatedArrivalAt !== movement?.estimatedArrivalAt;
    this.employee = employee;
    this.office = office;
    this.state = employee.currentState;
    this.movement = movement;
    this.phoneOpen = phoneOpen;
    if (serverTime) {
      const parsed = Date.parse(serverTime);
      if (Number.isFinite(parsed)) this.serverTimeOffsetMs = parsed - Date.now();
    }
    if (appearanceChanged) this.redrawAppearance();
    if (!movement && (stateChanged || movementChanged || employee.locationId)) {
      const position = employeePosition(employee, office);
      if (position) this.setLogicalPosition(position);
    }
    this.selection.visible = selected;
    this.refreshVisuals();
  }

  update(deltaMs: number): void {
    const progress = this.movementProgress();
    if (this.movement && progress !== null) {
      const position = interpolatePath(this.movement.path, progress);
      if (position) {
        const dx = position.x - this.logicalPosition.x;
        const dy = position.y - this.logicalPosition.y;
        if (Math.abs(dx) + Math.abs(dy) > 0.04) {
          if (Math.abs(dy) > Math.abs(dx) * 1.1) {
            this.travelDirection = dy < 0 ? 'back' : 'front';
          } else {
            this.travelDirection = 'side';
            this.facing = dx < 0 ? -1 : 1;
          }
        }
        this.setLogicalPosition(position);
      }
    }

    const walking = this.movement?.movementStatus === 'MOVING'
      || this.movement?.movementStatus === 'PENDING'
      || this.employee.movement === 'WALKING';
    this.animation.update(this.rig, this.animationMode(walking), this.poseMode(), deltaMs);
    const heightScale = this.employee.appearance.heightScale;
    this.sprite.scale.set(this.facing * heightScale, heightScale);
    if (this.backHead) this.backHead.visible = walking && this.travelDirection === 'back';
  }

  private animationMode(walking: boolean): AgentAnimation {
    if (walking) return 'walk';
    const location = this.employee.locationId
      ? this.office.locations.find((item) => item.id === this.employee.locationId)
      : undefined;
    const atSeat = location?.type === 'WORKSTATION'
      || location?.type === 'MEETING_SEAT'
      || location?.type === 'INTERVIEW_SEAT';
    return this.employee.posture === 'SITTING' && atSeat ? 'sit' : 'idle';
  }

  private poseMode(): AgentPose {
    if (this.phoneOpen || this.employee.device === 'PHONE' || this.state === 'checking_phone') return 'phone';
    if (this.employee.device === 'COMPUTER' || ['CODING', 'TESTING'].includes(this.employee.activity)) return 'typing';
    if (this.employee.activity === 'THINKING' || this.state === 'thinking') return 'thinking';
    if (this.employee.activity === 'TALKING') return 'talking';
    return 'neutral';
  }

  private redrawAppearance(): void {
    clearPart(this.leftLeg);
    clearPart(this.rightLeg);
    clearPart(this.leftArm);
    clearPart(this.rightArm);
    clearPart(this.torso);
    clearPart(this.head);

    const appearance = this.employee.appearance;
    const skin = parseColor(appearance.skinTone);
    const hair = parseColor(appearance.hairColor);
    const top = parseColor(appearance.topColor);
    const bottom = parseColor(appearance.bottomColor);
    const shoes = parseColor(appearance.shoeColor);
    const bodyWidth = appearance.bodyType === 'slim' ? 10 : appearance.bodyType === 'broad' ? 14 : 12;
    const outline = 0x172028;

    this.shadow
      .clear()
      .ellipse(3, 3, 22, 7)
      .fill({ color: 0x030506, alpha: 0.18 })
      .ellipse(1, 2, 16, 5)
      .fill({ color: 0x050708, alpha: 0.3 });

    this.drawLeg(this.leftLeg, bottom, shoes, outline, appearance.bottomStyle);
    this.drawLeg(this.rightLeg, shade(bottom, -7), shade(shoes, -6), outline, appearance.bottomStyle);
    this.drawArm(this.leftArm, shade(top, -8), skin, outline);
    this.drawArm(this.rightArm, top, skin, outline);
    this.drawTorso(bodyWidth, top, skin, outline, appearance.topStyle, appearance.accessory);
    this.drawHead(skin, hair, outline, appearance.hairStyle, appearance.glasses, appearance.beardStyle, appearance.accessory, this.employee.gender);

    this.phone
      .clear()
      .roundRect(-4, -7, 8, 14, 2)
      .fill(0x121b24)
      .stroke({ color: 0xffd672, width: 1.2 })
      .roundRect(-2.4, -5.2, 4.8, 8.2, 1)
      .fill(0x69d8d0)
      .circle(0, 5, 0.8)
      .fill(0xffe6a5);
    this.phone.visible = false;
  }

  private drawLeg(
    part: Container,
    clothColor: number,
    shoeColor: number,
    outline: number,
    style: string,
  ): void {
    const leg = new Graphics();
    const width = style === 'skirt' ? 7 : 6;
    leg
      .roundRect(-width / 2, -1, width, 17, 2)
      .fill(clothColor)
      .stroke({ color: outline, width: 1 })
      .roundRect(-width / 2 - 1, 13, width + 4, 5, 1.5)
      .fill(shoeColor)
      .stroke({ color: outline, width: 1 })
      .rect(-width / 2 + 1, 1, 1, 10)
      .fill({ color: shade(clothColor, 25), alpha: 0.38 });
    part.addChild(leg);
  }

  private drawArm(part: Container, sleeveColor: number, skinColor: number, outline: number): void {
    const arm = new Graphics();
    arm
      .roundRect(-3.2, 0, 6.4, 16, 3)
      .fill(sleeveColor)
      .stroke({ color: outline, width: 1 })
      .roundRect(-2.8, 13, 5.6, 8, 2.8)
      .fill(skinColor)
      .stroke({ color: outline, width: 0.8 })
      .rect(-1.6, 2, 1.2, 9)
      .fill({ color: 0xffffff, alpha: 0.14 });
    part.addChild(arm);
  }

  private drawTorso(
    bodyWidth: number,
    topColor: number,
    skinColor: number,
    outline: number,
    style: string,
    accessory: string,
  ): void {
    const body = new Graphics();
    body
      .roundRect(-3.5, -18, 7, 8, 2)
      .fill(skinColor)
      .poly([
        -bodyWidth, -13,
        bodyWidth, -13,
        bodyWidth + 2, 10,
        bodyWidth - 3, 14,
        -bodyWidth + 3, 14,
        -bodyWidth - 2, 10,
      ])
      .fill(topColor)
      .stroke({ color: outline, width: 1.4, join: 'round' })
      .poly([
        -bodyWidth + 2, -10,
        -bodyWidth + 6, -10,
        -bodyWidth + 5, 10,
        -bodyWidth + 1, 8,
      ])
      .fill({ color: shade(topColor, 34), alpha: 0.34 });

    if (style === 'shirt') {
      body
        .poly([-5, -13, 0, -7, 5, -13])
        .stroke({ color: shade(topColor, 45), width: 1.5 })
        .circle(0, -3, 1)
        .fill(shade(topColor, 55));
    } else if (style === 'hoodie') {
      body
        .arc(0, -10, 7, 0.25, Math.PI - 0.25)
        .stroke({ color: shade(topColor, -25), width: 2 })
        .moveTo(-2, -6)
        .lineTo(-2, 1)
        .moveTo(2, -6)
        .lineTo(2, 1)
        .stroke({ color: shade(topColor, 42), width: 1 });
    } else if (style === 'jacket') {
      body
        .moveTo(-6, -12)
        .lineTo(-1, -3)
        .lineTo(-1, 12)
        .moveTo(6, -12)
        .lineTo(1, -3)
        .lineTo(1, 12)
        .stroke({ color: shade(topColor, 45), width: 1.3 });
    } else {
      body
        .moveTo(-bodyWidth + 2, 6)
        .lineTo(bodyWidth - 2, 6)
        .stroke({ color: shade(topColor, 22), width: 1 });
    }

    if (accessory === 'badge') {
      body
        .roundRect(4, -6, 6, 8, 1)
        .fill(0xf3ca67)
        .stroke({ color: 0x6f5426, width: 0.8 })
        .rect(5.5, -4, 3, 1)
        .fill(0xfff0b7);
    }
    if (accessory === 'watch') {
      body.circle(-bodyWidth - 1, 7, 2.2).fill(0x8fd4df).stroke({ color: outline, width: 0.8 });
    }
    this.torso.addChild(body);
  }

  private drawHead(
    skinColor: number,
    hairColor: number,
    outline: number,
    style: string,
    glasses: boolean,
    beardStyle: string | undefined,
    accessory: string,
    gender: Employee['gender'],
  ): void {
    const head = new Graphics();
    if (style === 'bob' || style === 'wave') {
      head
        .ellipse(0, 0, 15.5, 17)
        .fill(shade(hairColor, -8))
        .roundRect(-15, -2, 4.5, 20, 3)
        .fill(hairColor)
        .roundRect(10.5, -2, 4.5, 20, 3)
        .fill(hairColor);
    }
    const faceHalfWidth = gender === 'male' ? 12.7 : gender === 'female' ? 11.8 : 12.2;
    const eyeColor = gender === 'female' ? 0x30282a : 0x252b30;
    head
      .circle(-faceHalfWidth, 1, 2.5)
      .fill(shade(skinColor, -8))
      .circle(faceHalfWidth, 1, 2.5)
      .fill(shade(skinColor, -8))
      .ellipse(0, 0, faceHalfWidth, 14.4)
      .fill(skinColor)
      .stroke({ color: outline, width: 1.2 })
      .moveTo(-8.2, -4)
      .quadraticCurveTo(-5.3, -5.6, -2.7, -4.1)
      .moveTo(2.7, -4.1)
      .quadraticCurveTo(5.3, -5.6, 8.2, -4)
      .stroke({ color: shade(hairColor, -18), width: gender === 'male' ? 1.35 : 0.9, cap: 'round' })
      .ellipse(-5.2, 0, 2.45, 1.75)
      .fill(0xf5eee6)
      .ellipse(5.2, 0, 2.45, 1.75)
      .fill(0xf5eee6)
      .circle(-5.2, 0.1, 1.25)
      .fill(eyeColor)
      .circle(5.2, 0.1, 1.25)
      .fill(eyeColor)
      .circle(-4.8, -0.35, 0.38)
      .fill(0xffffff)
      .circle(5.6, -0.35, 0.38)
      .fill(0xffffff)
      .moveTo(0, 1.4)
      .quadraticCurveTo(-1.2, 4.3, 0.8, 4.5)
      .stroke({ color: shade(skinColor, -38), width: 0.85, cap: 'round' })
      .moveTo(-3.4, 8)
      .quadraticCurveTo(0, 10, 3.6, 7.8)
      .stroke({ color: shade(skinColor, -58), width: 1.05, cap: 'round' })
      .circle(-8.5, 5.5, 2.4)
      .fill({ color: 0xf09b93, alpha: 0.14 })
      .circle(8.5, 5.5, 2.4)
      .fill({ color: 0xf09b93, alpha: 0.14 });

    if (gender === 'female') {
      head
        .moveTo(-7.7, -0.8).lineTo(-9.2, -1.7)
        .moveTo(7.7, -0.8).lineTo(9.2, -1.7)
        .stroke({ color: eyeColor, width: 0.85, cap: 'round' });
    } else if (gender === 'male') {
      head
        .moveTo(-8.5, 9.5).quadraticCurveTo(0, 14.6, 8.5, 9.5)
        .stroke({ color: shade(skinColor, -25), width: 0.65, alpha: 0.5 });
    }

    this.drawHair(head, style, hairColor);
    if (beardStyle) {
      head
        .arc(0, 4, 8.4, 0.28, Math.PI - 0.28)
        .stroke({ color: hairColor, width: beardStyle === 'stubble' ? 1.5 : 2.5, alpha: 0.78 });
    }
    if (glasses) {
      head
        .roundRect(-10, -3, 8, 7, 2)
        .stroke({ color: 0x26323c, width: 1.2 })
        .roundRect(2, -3, 8, 7, 2)
        .stroke({ color: 0x26323c, width: 1.2 })
        .moveTo(-2, 0)
        .lineTo(2, 0)
        .stroke({ color: 0x26323c, width: 1 })
        .rect(-8, -1, 3, 1)
        .fill({ color: 0xffffff, alpha: 0.35 });
    }
    if (accessory === 'headphones') {
      head
        .arc(0, -1, 16.5, Math.PI + 0.18, Math.PI * 2 - 0.18)
        .stroke({ color: 0x77cfee, width: 2 })
        .roundRect(-17, -2, 4, 10, 2)
        .fill(0x77cfee)
        .roundRect(13, -2, 4, 10, 2)
        .fill(0x77cfee);
    }
    const backHead = new Graphics()
      .ellipse(0, 0, 13.3, 15)
      .fill(shade(hairColor, -5))
      .stroke({ color: outline, width: 1.2 })
      .arc(0, -1, 10.2, Math.PI + 0.3, Math.PI * 2 - 0.3)
      .stroke({ color: shade(hairColor, 28), width: 2, alpha: 0.5 })
      .rect(-8, 8, 16, 4)
      .fill({ color: shade(hairColor, -18), alpha: 0.9 });
    if (style === 'bob' || style === 'wave') {
      backHead
        .roundRect(-16, -3, 5, 22, 3)
        .fill(hairColor)
        .roundRect(11, -3, 5, 22, 3)
        .fill(hairColor);
    }
    backHead.visible = false;
    this.backHead = backHead;
    this.head.addChild(head, backHead);
  }

  private drawHair(graphics: Graphics, style: string, color: number): void {
    if (style === 'curly') {
      graphics
        .circle(-9, -9, 6.5).fill(color)
        .circle(-3, -14, 7).fill(color)
        .circle(5, -14, 7.5).fill(color)
        .circle(10, -8, 6).fill(color)
        .circle(0, -9, 7).fill(color);
      return;
    }
    if (style === 'bob' || style === 'wave') {
      graphics
        // Keep the front fringe above the brows.  A wide arc centered at y=-1
        // extends into the eye band (y≈0), especially after Pixi scaling.
        .roundRect(-12.5, -15, 25, 6, 3)
        .fill(color)
        .roundRect(-13.5, -11, 4.5, 7, 2.2)
        .fill(color)
        .roundRect(9, -11, 4.5, 7, 2.2)
        .fill(color);
      if (style === 'wave') graphics.circle(12, -5.5, 3.2).fill(color);
      return;
    }
    graphics
      .roundRect(-13, -14, 26, 10, 5)
      .fill(color)
      .poly([-13, -8, -7, -13, -4, -4, -13, -1])
      .fill(color);
    if (style === 'side_part') {
      graphics.moveTo(-2, -14).lineTo(5, -5).stroke({ color: shade(color, 35), width: 1.2 });
    }
  }

  private refreshVisuals(): void {
    const fallbackLabel = STATE_LABELS[this.state] ?? this.state;
    const label = this.movement
      ? '移动中'
      : this.employee.device === 'PHONE'
        ? '看手机'
        : activityLabel(this.employee.activity, fallbackLabel);
    const color = this.movement
      ? STATE_COLORS.walking
      : activityColor(this.employee.activity, STATE_COLORS[this.state] ?? STATE_COLORS.idle);
    const prefix = this.movement
      ? '↗ '
      : this.employee.activity === 'THINKING'
        ? '… '
        : this.employee.device === 'PHONE'
          ? '● '
          : '';
    this.stateBadge.text = `${prefix}${label}`;
    this.stateBadge.style.fill = color;
    this.stateBadge.position.set(0, -93);
    const statusWidth = Math.max(50, this.stateBadge.width + 17);
    this.statusPlate
      .clear()
      .roundRect(-statusWidth / 2, -101, statusWidth, 17, 6)
      .fill({ color: 0x101820, alpha: 0.9 })
      .stroke({ color, width: 1, alpha: 0.42 })
      .circle(-statusWidth / 2 + 7, -92.5, 2)
      .fill({ color, alpha: 0.9 });

    this.nameLabel.text = this.employee.name;
    this.nameLabel.position.set(0, 20);
    const nameWidth = Math.max(42, this.nameLabel.width + 16);
    this.namePlate
      .clear()
      .roundRect(-nameWidth / 2, 12, nameWidth, 17, 6)
      .fill({ color: 0x101519, alpha: 0.82 })
      .stroke({ color: 0xd8bd83, width: 0.8, alpha: 0.28 });

    this.selection
      .clear()
      .ellipse(0, 2, 27, 10)
      .fill({ color: 0xf3c86a, alpha: 0.09 })
      .stroke({ color: 0xf3c86a, width: 2, alpha: 0.95 })
      .ellipse(0, 2, 20, 7)
      .stroke({ color: 0xffe4a6, width: 1, alpha: 0.38 });
  }

  private movementProgress(): number | null {
    if (!this.movement) return null;
    const started = Date.parse(this.movement.startedAt);
    const arrival = Date.parse(this.movement.estimatedArrivalAt);
    if (!Number.isFinite(started) || !Number.isFinite(arrival) || arrival <= started) return 1;
    const now = Date.now() + this.serverTimeOffsetMs;
    return Math.max(0, Math.min(1, (now - started) / (arrival - started)));
  }

  private setLogicalPosition(point: Point): void {
    this.logicalPosition = point;
    const projected = projectPoint(point);
    this.position.set(projected.x, projected.y);
    this.zIndex = depthFor(point.y);
  }
}

function clearPart(part: Container): void {
  part.removeChildren().forEach((child) => child.destroy());
}

function initialPosition(employee: Employee, office: OfficeLayout): Point {
  return employeePosition(employee, office) ?? { x: 500, y: 500 };
}

function employeePosition(employee: Employee, office: OfficeLayout): Point | null {
  if (Number.isFinite(employee.x) && Number.isFinite(employee.y) && (employee.x !== 0 || employee.y !== 0)) {
    return { x: employee.x, y: employee.y };
  }
  const location = employee.locationId ? office.locations.find((item) => item.id === employee.locationId) : undefined;
  if (location) return { x: location.x, y: location.y };
  const desk = office.desks.find((item) => item.id === employee.deskId || item.employeeId === employee.id);
  const seat = desk ? office.seats.find((item) => item.id === desk.seatId) : undefined;
  return seat ? { x: seat.x, y: seat.y } : null;
}

function interpolatePath(path: Point[], progress: number): Point | null {
  if (path.length === 0) return null;
  if (path.length === 1 || progress <= 0) return path[0];
  if (progress >= 1) return path[path.length - 1];
  const lengths = path.slice(1).map((point, index) => Math.hypot(point.x - path[index].x, point.y - path[index].y));
  const total = lengths.reduce((sum, length) => sum + length, 0);
  if (total <= 0) return path[path.length - 1];
  let remaining = total * progress;
  for (let index = 0; index < lengths.length; index += 1) {
    const segmentLength = lengths[index];
    if (remaining <= segmentLength) {
      const start = path[index];
      const end = path[index + 1];
      const ratio = segmentLength === 0 ? 1 : remaining / segmentLength;
      return { x: start.x + (end.x - start.x) * ratio, y: start.y + (end.y - start.y) * ratio };
    }
    remaining -= segmentLength;
  }
  return path[path.length - 1];
}

function activityLabel(activity: string, fallback: string): string {
  return {
    IDLE: '待命',
    THINKING: '思考中',
    READING: '阅读中',
    CODING: '工作中',
    TESTING: '测试中',
    TALKING: '交谈中',
    MEETING: '会议中',
    INTERVIEWING: '面试中',
    WAITING: '等待',
    ERROR: '异常',
  }[activity] ?? fallback;
}

function activityColor(activity: string, fallback: number): number {
  return {
    THINKING: STATE_COLORS.thinking,
    READING: STATE_COLORS.checking_phone,
    CODING: STATE_COLORS.coding,
    TESTING: STATE_COLORS.testing,
    TALKING: STATE_COLORS.meeting,
    MEETING: STATE_COLORS.meeting,
    INTERVIEWING: STATE_COLORS.interviewing,
    WAITING: STATE_COLORS.waiting,
    ERROR: STATE_COLORS.error,
  }[activity] ?? fallback;
}
