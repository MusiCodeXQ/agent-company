import { Application, Container, Graphics, Text } from 'pixi.js';
import type {
  ActiveMovement,
  Employee,
  FurnitureDefinition,
  OfficeLayout,
  RoomDefinition,
} from '../../domain/types';
import { AgentActor } from '../actors/agent-actor';
import { depthFor, projectFootprint, projectedOfficeSize, projectPoint } from './projection';

interface DeskView {
  screen: Graphics;
  screenLabel: Text;
  badgeName: Text;
  badgeTitle: Text;
  employeeId: string;
  anchorX: number;
  anchorY: number;
}

interface CameraPoint {
  x: number;
  y: number;
}

interface PrismPalette {
  top: number;
  front: number;
  side: number;
  edge: number;
}

interface RoomPalette {
  floor: number;
  border: number;
  accent: number;
  light: number;
}

const ROOM_PALETTES: Record<string, RoomPalette> = {
  workspace: { floor: 0x24323a, border: 0x67818a, accent: 0x76c7b4, light: 0x8bd4c0 },
  meeting: { floor: 0x293b42, border: 0x7794a0, accent: 0x75bde8, light: 0x86cfff },
  interview: { floor: 0x3a303c, border: 0x8e758a, accent: 0xd89fbf, light: 0xf0b7d4 },
  office: { floor: 0x303b32, border: 0x768b77, accent: 0xa7bd7f, light: 0xc8d59c },
  reception: { floor: 0x40362f, border: 0x947d69, accent: 0xe0aa72, light: 0xf2c58f },
  pantry: { floor: 0x3a352c, border: 0x8c8065, accent: 0xd2a75f, light: 0xf1c978 },
  restroom: { floor: 0x2d3940, border: 0x718994, accent: 0x82b7ca, light: 0xa9ddec },
};

const DESK_PALETTE: PrismPalette = {
  top: 0x76583a,
  front: 0x49331f,
  side: 0x34251a,
  edge: 0xc49965,
};

export class OfficeEngine {
  private readonly app: Application;
  private readonly world = new Container();
  private readonly floorLayer = new Container();
  private readonly roomLayer = new Container();
  private readonly depthLayer = new Container();
  private readonly effectLayer = new Container();
  private readonly overlayLayer = new Container();
  private readonly actors = new Map<string, AgentActor>();
  private readonly staticDepthItems = new Set<Container>();
  private readonly deskViews = new Map<string, DeskView>();
  private office: OfficeLayout | null = null;
  private employees: Employee[] = [];
  private movements = new Map<string, ActiveMovement>();
  private serverTime: string | null = null;
  private selectedEmployeeId: string | null = null;
  private scale = 1;
  private dragging = false;
  private lastPointer = { x: 0, y: 0 };
  private cameraTarget: CameraPoint | null = null;
  private readonly onSelect: (employeeId: string) => void;
  private readonly onOpenPhone: (employeeId: string) => void;
  private readonly onMeetingTableClick: () => void;
  private readonly onWheel: (event: WheelEvent) => void;
  private readonly onPointerDown: (event: PointerEvent) => void;
  private readonly onPointerMove: (event: PointerEvent) => void;
  private readonly onPointerUp: () => void;
  private readonly onFocus: (event: Event) => void;

  constructor(
    app: Application,
    onSelect: (employeeId: string) => void,
    onOpenPhone: (employeeId: string) => void,
    onMeetingTableClick: () => void = () => undefined,
  ) {
    this.app = app;
    this.onSelect = onSelect;
    this.onOpenPhone = onOpenPhone;
    this.onMeetingTableClick = onMeetingTableClick;
    this.depthLayer.sortableChildren = true;
    this.world.addChild(
      this.floorLayer,
      this.roomLayer,
      this.depthLayer,
      this.effectLayer,
      this.overlayLayer,
    );
    this.app.stage.addChild(this.world);

    this.onWheel = (event) => this.handleWheel(event);
    this.onPointerDown = (event) => this.handlePointerDown(event);
    this.onPointerMove = (event) => this.handlePointerMove(event);
    this.onPointerUp = () => this.handlePointerUp();
    this.onFocus = (event) => {
      const customEvent = event as CustomEvent<string>;
      if (customEvent.detail) this.focusEmployee(customEvent.detail);
    };
    this.app.canvas.addEventListener('wheel', this.onWheel, { passive: false });
    this.app.canvas.addEventListener('pointerdown', this.onPointerDown);
    this.app.canvas.addEventListener('pointermove', this.onPointerMove);
    this.app.canvas.addEventListener('pointerup', this.onPointerUp);
    this.app.canvas.addEventListener('pointerleave', this.onPointerUp);
    window.addEventListener('office:focus', this.onFocus);
    this.app.ticker.add((ticker) => this.update(ticker.deltaMS));
  }

  setOffice(office: OfficeLayout): void {
    if (this.office === office) return;
    this.office = office;
    this.redrawOffice();
    this.fitCamera();
  }

  setEmployees(employees: Employee[]): void {
    if (!this.office) return;
    const visibleEmployees = this.office.floorId
      ? employees.filter((employee) => employee.floorId === this.office?.floorId && employee.employmentStatus !== 'TERMINATED')
      : employees.filter((employee) => employee.employmentStatus !== 'TERMINATED');
    this.employees = visibleEmployees;
    const incomingIds = new Set(visibleEmployees.map((employee) => employee.id));
    for (const [employeeId, actor] of this.actors) {
      if (!incomingIds.has(employeeId)) {
        actor.destroy({ children: true });
        this.actors.delete(employeeId);
      }
    }
    for (const employee of visibleEmployees) {
      const existing = this.actors.get(employee.id);
      if (existing) {
        existing.syncEmployee(
          employee,
          this.office,
          employee.id === this.selectedEmployeeId,
          this.movements.get(employee.id) ?? null,
          this.serverTime,
        );
      } else {
        const actor = new AgentActor(employee, this.office, this.onSelect, this.onOpenPhone);
        actor.syncEmployee(
          employee,
          this.office,
          employee.id === this.selectedEmployeeId,
          this.movements.get(employee.id) ?? null,
          this.serverTime,
        );
        this.actors.set(employee.id, actor);
        this.depthLayer.addChild(actor);
      }
    }
    this.updateDeskMonitors();
  }

  setMovements(movements: ActiveMovement[], serverTime: string | null): void {
    this.movements = new Map(movements.map((movement) => [movement.agentId, movement]));
    this.serverTime = serverTime;
    if (!this.office) return;
    for (const employee of this.employees) {
      this.actors.get(employee.id)?.syncEmployee(
        employee,
        this.office,
        employee.id === this.selectedEmployeeId,
        this.movements.get(employee.id) ?? null,
        this.serverTime,
      );
    }
  }

  setSelectedEmployee(employeeId: string | null): void {
    this.selectedEmployeeId = employeeId;
    if (!this.office) return;
    for (const employee of this.employees) {
      this.actors.get(employee.id)?.syncEmployee(
        employee,
        this.office,
        employee.id === employeeId,
        this.movements.get(employee.id) ?? null,
        this.serverTime,
      );
    }
  }

  focusEmployee(employeeId: string): void {
    const actor = this.actors.get(employeeId);
    if (actor) this.cameraTarget = { x: actor.x, y: actor.y };
  }

  resize(): void {
    this.fitCamera();
  }

  destroy(): void {
    this.app.canvas.removeEventListener('wheel', this.onWheel);
    this.app.canvas.removeEventListener('pointerdown', this.onPointerDown);
    this.app.canvas.removeEventListener('pointermove', this.onPointerMove);
    this.app.canvas.removeEventListener('pointerup', this.onPointerUp);
    this.app.canvas.removeEventListener('pointerleave', this.onPointerUp);
    window.removeEventListener('office:focus', this.onFocus);
    for (const actor of this.actors.values()) actor.destroy({ children: true });
    this.actors.clear();
    this.clearStaticDepthItems();
  }

  private update(deltaMs: number): void {
    for (const actor of this.actors.values()) actor.update(deltaMs);
    this.updateCamera();
  }

  private redrawOffice(): void {
    if (!this.office) return;
    clearContainer(this.floorLayer);
    clearContainer(this.roomLayer);
    clearContainer(this.effectLayer);
    clearContainer(this.overlayLayer);
    this.clearStaticDepthItems();
    this.deskViews.clear();

    this.drawFloorPlate();
    this.drawFloorGrid();
    for (const room of this.office.rooms) this.drawRoom(room);
    this.drawSharedSeats();
    for (const desk of this.office.desks) this.drawDesk(desk);
    for (const furniture of this.office.furniture) this.drawFurniture(furniture);
    for (const door of this.office.doors) this.drawDoor(door);
    this.drawLegend();
    this.updateDeskMonitors();
  }

  private drawFloorPlate(): void {
    if (!this.office) return;
    const topLeft = projectPoint({ x: 0, y: 0 });
    const topRight = projectPoint({ x: this.office.width, y: 0 });
    const bottomRight = projectPoint({ x: this.office.width, y: this.office.height });
    const bottomLeft = projectPoint({ x: 0, y: this.office.height });
    const depth = 24;
    const floor = new Graphics();
    floor
      .poly([
        bottomLeft.x, bottomLeft.y,
        bottomRight.x, bottomRight.y,
        bottomRight.x, bottomRight.y + depth,
        bottomLeft.x, bottomLeft.y + depth,
      ])
      .fill(0x0b1014)
      .poly([
        topRight.x, topRight.y,
        bottomRight.x, bottomRight.y,
        bottomRight.x, bottomRight.y + depth,
        topRight.x, topRight.y + depth,
      ])
      .fill(0x11181d)
      .poly(projectFootprint(0, 0, this.office.width, this.office.height))
      .fill(0x172128)
      .stroke({ color: 0x66747a, width: 2, alpha: 0.38 });
    this.floorLayer.addChild(floor);

    const horizon = new Graphics();
    horizon
      .poly([
        topLeft.x - 20, topLeft.y - 32,
        topRight.x + 80, topRight.y - 32,
        bottomRight.x + 60, bottomRight.y + 20,
        bottomLeft.x - 40, bottomLeft.y + 20,
      ])
      .fill({ color: 0x5f97a2, alpha: 0.025 });
    this.floorLayer.addChild(horizon);
  }

  private drawFloorGrid(): void {
    if (!this.office) return;
    const major = new Graphics();
    const minor = new Graphics();
    for (let x = 0; x <= this.office.width; x += 80) {
      const start = projectPoint({ x, y: 0 });
      const end = projectPoint({ x, y: this.office.height });
      const target = x % 240 === 0 ? major : minor;
      target.moveTo(start.x, start.y).lineTo(end.x, end.y);
    }
    for (let y = 0; y <= this.office.height; y += 80) {
      const start = projectPoint({ x: 0, y });
      const end = projectPoint({ x: this.office.width, y });
      const target = y % 240 === 0 ? major : minor;
      target.moveTo(start.x, start.y).lineTo(end.x, end.y);
    }
    minor.stroke({ color: 0x82939a, width: 1, alpha: 0.045 });
    major.stroke({ color: 0x9cadad, width: 1, alpha: 0.08 });
    this.floorLayer.addChild(minor, major);
  }

  private drawRoom(room: RoomDefinition): void {
    const palette = ROOM_PALETTES[room.type] ?? ROOM_PALETTES.workspace;
    const topLeft = projectPoint({ x: room.x, y: room.y });
    const topRight = projectPoint({ x: room.x + room.width, y: room.y });
    const bottomLeft = projectPoint({ x: room.x, y: room.y + room.height });
    const wallHeight = room.type === 'workspace' ? 20 : 25;
    const roomGraphic = new Graphics();
    roomGraphic
      .poly(projectFootprint(room.x, room.y, room.width, room.height))
      .fill({ color: palette.floor, alpha: 0.98 })
      .stroke({ color: palette.border, width: 2, alpha: 0.7 })
      .poly([
        topLeft.x, topLeft.y,
        topRight.x, topRight.y,
        topRight.x, topRight.y - wallHeight,
        topLeft.x, topLeft.y - wallHeight,
      ])
      .fill({ color: shadeColor(palette.floor, -24), alpha: 0.98 })
      .stroke({ color: palette.border, width: 1.2, alpha: 0.6 })
      .poly([
        topLeft.x, topLeft.y,
        bottomLeft.x, bottomLeft.y,
        bottomLeft.x, bottomLeft.y - 10,
        topLeft.x, topLeft.y - wallHeight,
      ])
      .fill({ color: shadeColor(palette.floor, -34), alpha: 0.76 })
      .stroke({ color: palette.border, width: 1, alpha: 0.42 });
    this.roomLayer.addChild(roomGraphic);
    this.drawRoomTexture(room, palette);

    const center = projectPoint({ x: room.x + room.width / 2, y: room.y + room.height / 2 });
    const light = new Graphics()
      .ellipse(center.x, center.y - 5, Math.max(45, room.width * 0.26), Math.max(25, room.height * 0.2))
      .fill({ color: palette.light, alpha: 0.035 })
      .ellipse(center.x, center.y - 5, Math.max(20, room.width * 0.13), Math.max(12, room.height * 0.1))
      .fill({ color: palette.light, alpha: 0.035 });
    this.roomLayer.addChild(light);

    const labelAnchor = projectPoint({ x: room.x + 15, y: room.y + 20 });
    const labelContainer = new Container();
    labelContainer.position.set(labelAnchor.x, labelAnchor.y - wallHeight - 4);
    const label = new Text({
      text: room.label,
      style: {
        fill: 0xf1e7d4,
        fontSize: 14,
        fontWeight: '800',
        letterSpacing: 0.5,
        dropShadow: { color: 0x030506, alpha: 0.8, blur: 2, distance: 1 },
      },
    });
    const plateWidth = label.width + 29;
    const plate = new Graphics()
      .roundRect(0, -3, plateWidth, 24, 5)
      .fill({ color: 0x11171a, alpha: 0.82 })
      .stroke({ color: palette.accent, width: 1, alpha: 0.4 })
      .rect(0, 2, 4, 14)
      .fill(palette.accent);
    label.position.set(13, 1);
    labelContainer.addChild(plate, label);
    if (room.type === 'workspace') {
      const subtitle = new Text({
        text: 'LIVE AGENT FLOOR',
        style: { fill: palette.accent, fontSize: 8, fontWeight: '700', letterSpacing: 1.5 },
      });
      subtitle.position.set(plateWidth + 10, 5);
      labelContainer.addChild(subtitle);
    }
    this.overlayLayer.addChild(labelContainer);
  }

  private drawRoomTexture(room: RoomDefinition, palette: RoomPalette): void {
    const texture = new Graphics();
    const step = room.type === 'workspace' ? 64 : 48;
    for (let y = room.y + step; y < room.y + room.height; y += step) {
      const start = projectPoint({ x: room.x + 5, y });
      const end = projectPoint({ x: room.x + room.width - 5, y });
      texture.moveTo(start.x, start.y).lineTo(end.x, end.y);
    }
    texture.stroke({ color: palette.border, width: 1, alpha: 0.09 });
    this.roomLayer.addChild(texture);
  }

  private drawDesk(desk: OfficeLayout['desks'][number]): void {
    if (!this.office) return;
    const seat = this.office.seats.find((item) => item.id === desk.seatId);
    if (seat) this.drawChair(seat.x, seat.y, 0x45606a, -8);

    const view = new Container();
    const table = new Graphics();
    drawPrism(table, desk.x, desk.y, desk.width, desk.height, 17, DESK_PALETTE);
    const keyboard = new Graphics()
      .poly(projectFootprint(desk.x + 53, desk.y + 54, 56, 13))
      .fill(0x1b252a)
      .stroke({ color: 0x829297, width: 0.8, alpha: 0.55 });
    const keyboardLines = new Graphics();
    for (let index = 0; index < 5; index += 1) {
      const start = projectPoint({ x: desk.x + 59 + index * 10, y: desk.y + 58 });
      const end = projectPoint({ x: desk.x + 59 + index * 10, y: desk.y + 64 });
      keyboardLines.moveTo(start.x, start.y).lineTo(end.x, end.y);
    }
    keyboardLines.stroke({ color: 0x9fb2b6, width: 0.7, alpha: 0.4 });

    const monitorAnchor = projectPoint({ x: desk.monitorX + 22, y: desk.monitorY + 22 });
    const monitor = new Graphics();
    drawMonitor(monitor, monitorAnchor.x, monitorAnchor.y, 'idle', 0x5ce1b8);
    const screenLabel = new Text({
      text: 'NEWS',
      style: {
        fill: 0xd7fbf4,
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
        fontSize: 6.5,
        fontWeight: '800',
        letterSpacing: 0.5,
      },
    });
    screenLabel.position.set(monitorAnchor.x - 16, monitorAnchor.y - 32);
    const deskLamp = new Graphics()
      .circle(monitorAnchor.x + 48, monitorAnchor.y - 6, 12)
      .fill({ color: 0xffce74, alpha: 0.04 })
      .moveTo(monitorAnchor.x + 48, monitorAnchor.y - 2)
      .lineTo(monitorAnchor.x + 52, monitorAnchor.y - 22)
      .stroke({ color: 0x8b765a, width: 2 })
      .circle(monitorAnchor.x + 53, monitorAnchor.y - 24, 4)
      .fill(0xd6a85e);

    const badge = new Container();
    const badgeSurface = new Graphics()
      .roundRect(0, 0, 88, 28, 4)
      .fill({ color: 0xf1d58d, alpha: 0.96 })
      .stroke({ color: 0x6d522a, width: 1, alpha: 0.95 })
      .roundRect(3, 3, 82, 22, 2)
      .fill({ color: 0x18252b, alpha: 0.94 })
      .stroke({ color: 0xf0ca70, width: 0.7, alpha: 0.72 })
      .circle(8, 8, 2)
      .fill(0x77dfc7)
      .rect(7, 19, 16, 1)
      .fill({ color: 0xf0ca70, alpha: 0.5 });
    const badgeName = new Text({
      text: '未分配',
      style: {
        fill: 0xfff1c6,
        fontFamily: 'ui-sans-serif, system-ui, sans-serif',
        fontSize: 9,
        fontWeight: '800',
      },
    });
    badgeName.position.set(14, 4);
    const badgeTitle = new Text({
      text: 'WORKSTATION',
      style: {
        fill: 0x9fc6c5,
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
        fontSize: 6.5,
        fontWeight: '700',
        letterSpacing: 0.15,
      },
    });
    badgeTitle.position.set(14, 15);
    badge.addChild(badgeSurface, badgeName, badgeTitle);
    const badgeAnchor = projectPoint({ x: desk.x + 12, y: desk.y + 50 });
    badge.position.set(badgeAnchor.x, badgeAnchor.y - 12);

    view.addChild(table, keyboard, keyboardLines, deskLamp, monitor, screenLabel, badge);
    // Use the furniture's real front edge for depth. Sorting by the seat made
    // the whole workstation render over its owner even when the owner was in front.
    this.addStaticDepthItem(view, desk.y + desk.height, 8);
    this.deskViews.set(desk.id, {
      screen: monitor,
      screenLabel,
      badgeName,
      badgeTitle,
      employeeId: desk.employeeId,
      anchorX: monitorAnchor.x,
      anchorY: monitorAnchor.y,
    });
  }

  private drawSharedSeats(): void {
    if (!this.office) return;
    for (const location of this.office.locations) {
      if (location.type === 'MEETING_SEAT') {
        this.drawChair(location.x, location.y, 0x49677a, -8);
      } else if (location.type === 'INTERVIEW_SEAT') {
        this.drawChair(location.x, location.y, 0x745467, -8);
      }
    }
  }

  private drawChair(x: number, y: number, color: number, depthOffset: number): void {
    const point = projectPoint({ x, y });
    const chair = new Container();
    const graphic = new Graphics()
      .ellipse(point.x + 2, point.y + 5, 18, 6)
      .fill({ color: 0x050708, alpha: 0.22 })
      .moveTo(point.x - 10, point.y + 1)
      .lineTo(point.x - 12, point.y + 13)
      .moveTo(point.x + 10, point.y + 1)
      .lineTo(point.x + 12, point.y + 13)
      .stroke({ color: 0x252d31, width: 2 })
      .roundRect(point.x - 15, point.y - 13, 30, 19, 7)
      .fill(shadeColor(color, -12))
      .stroke({ color: shadeColor(color, 32), width: 1.2 })
      .roundRect(point.x - 13, point.y - 29, 26, 18, 7)
      .fill(color)
      .stroke({ color: shadeColor(color, 38), width: 1.2 })
      .rect(point.x - 9, point.y - 25, 3, 10)
      .fill({ color: 0xffffff, alpha: 0.09 });
    chair.addChild(graphic);
    this.addStaticDepthItem(chair, y, depthOffset);
  }

  private drawFurniture(furniture: FurnitureDefinition): void {
    const item = new Container();
    const graphic = new Graphics();
    const center = projectPoint({
      x: furniture.x + furniture.width / 2,
      y: furniture.y + furniture.height / 2,
    });
    let depthOffset = 0;

    if (furniture.kind === 'meeting-table' || furniture.kind === 'interview-table') {
      const meeting = furniture.kind === 'meeting-table';
      drawPrism(graphic, furniture.x, furniture.y, furniture.width, furniture.height, 20, {
        top: meeting ? 0x5a4b42 : 0x604755,
        front: meeting ? 0x342a25 : 0x392833,
        side: meeting ? 0x28211e : 0x2b2028,
        edge: meeting ? 0xc49c70 : 0xc691ae,
      });
      graphic
        .ellipse(center.x, center.y - 2, 9, 4)
        .fill(meeting ? 0xd8b36c : 0xd19aae)
        .ellipse(center.x, center.y - 2, 3, 1.5)
        .fill(0xffedc2);
      const runner = new Graphics()
        .poly(projectFootprint(
          furniture.x + furniture.width * 0.18,
          furniture.y + furniture.height * 0.41,
          furniture.width * 0.64,
          furniture.height * 0.18,
        ))
        .fill({ color: meeting ? 0x83b5b2 : 0xb589a3, alpha: 0.18 });
      item.addChild(graphic, runner);
      depthOffset = 4;
      if (meeting) {
        item.eventMode = 'static';
        item.cursor = 'pointer';
        item.on('pointertap', () => this.onMeetingTableClick());
      }
    } else if (furniture.kind === 'reception-counter' || furniture.kind === 'pantry-counter') {
      drawPrism(graphic, furniture.x, furniture.y, furniture.width, furniture.height, 24, {
        top: 0x785b40,
        front: 0x453222,
        side: 0x30251c,
        edge: 0xc49b6f,
      });
      item.addChild(graphic);
    } else if (furniture.kind === 'cabinet') {
      drawPrism(graphic, furniture.x, furniture.y, furniture.width, furniture.height, 33, {
        top: 0x4b6258,
        front: 0x293b35,
        side: 0x21312d,
        edge: 0x99b9a0,
      });
      for (let index = 1; index < 4; index += 1) {
        const divider = projectPoint({ x: furniture.x + index * furniture.width / 4, y: furniture.y + furniture.height });
        graphic.moveTo(divider.x, divider.y + 2).lineTo(divider.x, divider.y + 29);
      }
      graphic.stroke({ color: 0xa5c1ad, width: 1, alpha: 0.45 });
      item.addChild(graphic);
    } else if (furniture.kind === 'coffee-machine') {
      const anchor = projectPoint({ x: furniture.x + furniture.width / 2, y: furniture.y + furniture.height });
      graphic
        .ellipse(anchor.x, anchor.y + 4, 21, 6)
        .fill({ color: 0x030405, alpha: 0.25 })
        .roundRect(anchor.x - 20, anchor.y - 43, 40, 45, 6)
        .fill(0x263138)
        .stroke({ color: 0xd8b768, width: 1.5 })
        .roundRect(anchor.x - 14, anchor.y - 36, 28, 13, 3)
        .fill(0x10171b)
        .circle(anchor.x + 9, anchor.y - 30, 2.5)
        .fill(0xf1c55e)
        .rect(anchor.x - 10, anchor.y - 18, 20, 4)
        .fill(0x78939a)
        .roundRect(anchor.x - 7, anchor.y - 12, 14, 11, 2)
        .stroke({ color: 0xe4d4b7, width: 1.5 });
      item.addChild(graphic);
    } else if (furniture.kind === 'water-dispenser') {
      const anchor = projectPoint({ x: furniture.x + furniture.width / 2, y: furniture.y + furniture.height });
      graphic
        .ellipse(anchor.x, anchor.y + 4, 18, 5)
        .fill({ color: 0x030405, alpha: 0.22 })
        .roundRect(anchor.x - 15, anchor.y - 35, 30, 37, 5)
        .fill(0x526f78)
        .stroke({ color: 0xa9d9df, width: 1.2 })
        .ellipse(anchor.x, anchor.y - 38, 12, 17)
        .fill({ color: 0x8fd2df, alpha: 0.78 })
        .stroke({ color: 0xd3f3f5, width: 1 })
        .circle(anchor.x - 5, anchor.y - 17, 2)
        .fill(0x6dc4df)
        .circle(anchor.x + 5, anchor.y - 17, 2)
        .fill(0xf08f7b);
      item.addChild(graphic);
    } else if (furniture.kind === 'restroom-sinks') {
      drawPrism(graphic, furniture.x, furniture.y, furniture.width, furniture.height, 15, {
        top: 0x566d73,
        front: 0x31454b,
        side: 0x273a40,
        edge: 0x9fc2c7,
      });
      for (let index = 0; index < 3; index += 1) {
        const sink = projectPoint({ x: furniture.x + 20 + index * 32, y: furniture.y + 18 });
        graphic.ellipse(sink.x, sink.y, 8, 4).fill(0x9ed6dd).stroke({ color: 0xd5f0ef, width: 0.8 });
      }
      item.addChild(graphic);
    }

    if (furniture.label) {
      const label = new Text({
        text: furniture.label,
        style: {
          fill: 0xf4d7a4,
          fontSize: 9,
          fontWeight: '800',
          letterSpacing: 1.2,
          dropShadow: { color: 0x080a0b, alpha: 0.8, blur: 2, distance: 1 },
        },
      });
      label.anchor.set(0.5);
      label.position.set(center.x, center.y - 5);
      item.addChild(label);
    }
    this.addStaticDepthItem(item, furniture.y + furniture.height, depthOffset);
  }

  private drawDoor(door: OfficeLayout['doors'][number]): void {
    const threshold = new Graphics()
      .poly(projectFootprint(door.x, door.y, door.width, door.height))
      .fill({ color: 0xb7d8c5, alpha: 0.22 })
      .stroke({ color: 0xcbe8d5, width: 1.3, alpha: 0.62 });
    this.roomLayer.addChild(threshold);

    const left = projectPoint({ x: door.x, y: door.y + door.height });
    const right = projectPoint({ x: door.x + door.width, y: door.y + door.height });
    const frame = new Container();
    const graphic = new Graphics()
      .roundRect(left.x - 3, left.y - 31, 5, 34, 2)
      .fill(0x9fb9a9)
      .roundRect(right.x - 2, right.y - 31, 5, 34, 2)
      .fill(0x9fb9a9)
      .moveTo(left.x - 1, left.y - 29)
      .lineTo(right.x + 1, right.y - 29)
      .stroke({ color: 0xc9ded0, width: 4, cap: 'round' })
      .circle(right.x - 6, right.y - 15, 1.7)
      .fill(0xf3cf72);
    frame.addChild(graphic);
    this.addStaticDepthItem(frame, door.y + door.height, 18);
  }

  private drawLegend(): void {
    if (!this.office) return;
    const position = projectPoint({ x: 310, y: this.office.height - 42 });
    const legend = new Container();
    legend.position.set(position.x, position.y);
    const panel = new Graphics()
      .roundRect(0, 0, 476, 46, 8)
      .fill({ color: 0x0e1519, alpha: 0.9 })
      .stroke({ color: 0x8c826c, width: 1, alpha: 0.42 })
      .rect(0, 0, 4, 46)
      .fill(0xd3ac67);
    legend.addChild(panel);
    const title = new Text({
      text: 'LIVE OFFICE',
      style: { fill: 0xe7d9bd, fontSize: 9, fontWeight: '800', letterSpacing: 1.4 },
    });
    title.position.set(15, 8);
    legend.addChild(title);
    const items: Array<[string, number]> = [
      ['待命', 0xa5b6be],
      ['看手机', 0xffcf70],
      ['会议中', 0x8bc8ff],
      ['编码中', 0x72e1b6],
    ];
    items.forEach(([labelText, color], index) => {
      const x = 117 + index * 88;
      const dot = new Graphics()
        .circle(x, 23, 5)
        .fill({ color, alpha: 0.18 })
        .circle(x, 23, 2.5)
        .fill(color);
      const label = new Text({ text: labelText, style: { fill: 0xbfc8c9, fontSize: 10 } });
      label.position.set(x + 10, 16);
      legend.addChild(dot, label);
    });
    this.overlayLayer.addChild(legend);
  }

  private updateDeskMonitors(): void {
    const employeeById = new Map(this.employees.map((employee) => [employee.id, employee]));
    for (const view of this.deskViews.values()) {
      const employee = employeeById.get(view.employeeId);
      const working = employee
        ? employee.activity === 'CODING' || employee.activity === 'TESTING' || employee.device === 'COMPUTER'
        : false;
      const color = employee?.activity === 'TESTING'
        ? 0x71d7ee
        : employee?.activity === 'THINKING'
          ? 0xcab0ff
          : 0x72e1b6;
      drawMonitor(view.screen, view.anchorX, view.anchorY, working ? 'work' : 'idle', color);
      view.screenLabel.text = working
        ? employee?.activity === 'TESTING' ? 'TEST' : 'CODE'
        : 'NEWS';
      view.screenLabel.style.fill = working ? color : 0xd7fbf4;
      view.badgeName.text = employee?.name ?? '未分配';
      view.badgeTitle.text = employee?.title || employee?.role || 'WORKSTATION';
    }
  }

  private addStaticDepthItem(item: Container, logicalY: number, offset = 0): void {
    item.zIndex = depthFor(logicalY, offset);
    this.staticDepthItems.add(item);
    this.depthLayer.addChild(item);
  }

  private clearStaticDepthItems(): void {
    for (const item of this.staticDepthItems) {
      item.parent?.removeChild(item);
      item.destroy({ children: true });
    }
    this.staticDepthItems.clear();
  }

  private fitCamera(): void {
    if (!this.office) return;
    const width = this.app.screen.width || this.app.canvas.clientWidth || 900;
    const height = this.app.screen.height || this.app.canvas.clientHeight || 700;
    const scene = projectedOfficeSize(this.office.width, this.office.height);
    this.scale = Math.min((width - 28) / scene.x, (height - 28) / scene.y, 1.2);
    this.world.scale.set(this.scale);
    this.world.position.set((width - scene.x * this.scale) / 2, (height - scene.y * this.scale) / 2);
  }

  private handleWheel(event: WheelEvent): void {
    if (!this.office) return;
    event.preventDefault();
    const rect = this.app.canvas.getBoundingClientRect();
    const pointer = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const worldBefore = {
      x: (pointer.x - this.world.x) / this.scale,
      y: (pointer.y - this.world.y) / this.scale,
    };
    const nextScale = Math.max(0.55, Math.min(1.85, this.scale * (event.deltaY > 0 ? 0.9 : 1.1)));
    this.scale = nextScale;
    this.world.scale.set(nextScale);
    this.world.position.set(
      pointer.x - worldBefore.x * nextScale,
      pointer.y - worldBefore.y * nextScale,
    );
  }

  private handlePointerDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    this.dragging = true;
    this.lastPointer = { x: event.clientX, y: event.clientY };
  }

  private handlePointerMove(event: PointerEvent): void {
    if (!this.dragging) return;
    const dx = event.clientX - this.lastPointer.x;
    const dy = event.clientY - this.lastPointer.y;
    this.world.position.x += dx;
    this.world.position.y += dy;
    this.lastPointer = { x: event.clientX, y: event.clientY };
  }

  private handlePointerUp(): void {
    this.dragging = false;
  }

  private updateCamera(): void {
    if (!this.cameraTarget) return;
    const width = this.app.screen.width || this.app.canvas.clientWidth || 900;
    const height = this.app.screen.height || this.app.canvas.clientHeight || 700;
    const targetX = width / 2 - this.cameraTarget.x * this.scale;
    const targetY = height / 2 - this.cameraTarget.y * this.scale;
    this.world.position.x += (targetX - this.world.position.x) * 0.08;
    this.world.position.y += (targetY - this.world.position.y) * 0.08;
    if (Math.abs(targetX - this.world.position.x) < 1 && Math.abs(targetY - this.world.position.y) < 1) {
      this.cameraTarget = null;
    }
  }
}

function drawPrism(
  graphics: Graphics,
  x: number,
  y: number,
  width: number,
  height: number,
  extrusion: number,
  palette: PrismPalette,
): void {
  const topLeft = projectPoint({ x, y });
  const topRight = projectPoint({ x: x + width, y });
  const bottomRight = projectPoint({ x: x + width, y: y + height });
  const bottomLeft = projectPoint({ x, y: y + height });
  const shadow = projectFootprint(x + 5, y + 8, width, height);
  graphics
    .poly(shadow)
    .fill({ color: 0x020304, alpha: 0.18 })
    .poly([
      topRight.x, topRight.y,
      bottomRight.x, bottomRight.y,
      bottomRight.x, bottomRight.y + extrusion,
      topRight.x, topRight.y + extrusion,
    ])
    .fill(palette.side)
    .poly([
      bottomLeft.x, bottomLeft.y,
      bottomRight.x, bottomRight.y,
      bottomRight.x, bottomRight.y + extrusion,
      bottomLeft.x, bottomLeft.y + extrusion,
    ])
    .fill(palette.front)
    .stroke({ color: shadeColor(palette.front, 22), width: 1, alpha: 0.65 })
    .poly(projectFootprint(x, y, width, height))
    .fill(palette.top)
    .stroke({ color: palette.edge, width: 1.3, alpha: 0.78 })
    .moveTo(topLeft.x + 4, topLeft.y + 3)
    .lineTo(topRight.x - 5, topRight.y + 3)
    .stroke({ color: 0xffffff, width: 1, alpha: 0.08 });
}

function drawMonitor(
  graphics: Graphics,
  x: number,
  y: number,
  mode: 'idle' | 'work',
  activeColor: number,
): void {
  const working = mode === 'work';
  const screenColor = working ? activeColor : 0x69d6d3;
  graphics.clear();
  graphics
    .roundRect(x - 30, y - 44, 60, 40, 7)
    .fill({ color: screenColor, alpha: working ? 0.1 : 0.075 })
    .ellipse(x, y - 15, 39, 24)
    .fill({ color: screenColor, alpha: working ? 0.045 : 0.035 });
  graphics
    .roundRect(x - 23, y - 39, 46, 31, 4)
    .fill(0x172129)
    .stroke({ color: screenColor, width: working ? 1.5 : 1.2, alpha: 0.95 })
    .roundRect(x - 19, y - 35, 38, 23, 2)
    .fill(working ? shadeColor(activeColor, -80) : 0x23404a)
    .rect(x - 3, y - 8, 6, 9)
    .fill(0x4d5551)
    .roundRect(x - 13, y, 26, 3, 1.5)
    .fill(0x5d625a);
  if (working) {
    graphics
      .rect(x - 15, y - 31, 16, 2)
      .fill(0xe8fff7)
      .rect(x - 15, y - 26, 27, 2)
      .fill({ color: 0xe8fff7, alpha: 0.8 })
      .rect(x - 15, y - 21, 10, 2)
      .fill({ color: activeColor, alpha: 0.95 })
      .rect(x - 2, y - 21, 14, 2)
      .fill({ color: 0xe8fff7, alpha: 0.52 })
      .circle(x + 16, y - 32, 1.4)
      .fill(activeColor);
  } else {
    // Static leisure screen: a news/video thumbnail, play button and progress bar.
    // It is deliberately drawn once per state update; there is no frame animation.
    graphics
      .roundRect(x - 16, y - 31, 26, 13, 2)
      .fill({ color: 0x4c7480, alpha: 0.94 })
      .stroke({ color: 0x9ce8df, width: 0.6, alpha: 0.7 })
      .poly([x - 5, y - 28, x - 5, y - 21, x + 2, y - 24.5])
      .fill(0xe6fff5)
      .rect(x - 14, y - 15, 27, 1.5)
      .fill({ color: 0xb9f7e9, alpha: 0.82 })
      .rect(x - 14, y - 15, 10, 1.5)
      .fill(0xf4d27a)
      .circle(x + 15, y - 30, 1.3)
      .fill(0xff8076)
      .rect(x - 14, y - 19, 17, 1.5)
      .fill({ color: 0xe6fff5, alpha: 0.55 })
      .rect(x + 6, y - 19, 8, 1.5)
      .fill({ color: 0xe6fff5, alpha: 0.35 });
  }
}

function shadeColor(color: number, amount: number): number {
  const clamp = (value: number): number => Math.max(0, Math.min(255, value));
  const red = clamp(((color >> 16) & 0xff) + amount);
  const green = clamp(((color >> 8) & 0xff) + amount);
  const blue = clamp((color & 0xff) + amount);
  return (red << 16) | (green << 8) | blue;
}

function clearContainer(container: Container): void {
  container.removeChildren().forEach((child) => child.destroy({ children: true }));
}
