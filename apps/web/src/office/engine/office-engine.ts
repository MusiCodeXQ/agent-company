import { Application, Container, Graphics, Rectangle, Text } from 'pixi.js';
import { AgentActor } from '../actors/agent-actor';
import type { ActorState, Employee, FurnitureDefinition, OfficeLayout, RoomDefinition } from '../../domain/types';

interface DeskView {
  screen: Graphics;
  employeeId: string;
  monitorX: number;
  monitorY: number;
}

interface CameraPoint {
  x: number;
  y: number;
}

const ROOM_COLORS: Record<string, number> = {
  workspace: 0x1f2b3a,
  meeting: 0x263c4b,
  interview: 0x3c3148,
  office: 0x344034,
  reception: 0x493b35,
  pantry: 0x423a2e,
  restroom: 0x303a46,
};

const ACTIVE_STATES: ActorState[] = ['coding', 'designing', 'testing', 'reviewing'];

export class OfficeEngine {
  private readonly app: Application;
  private readonly world = new Container();
  private readonly floorLayer = new Container();
  private readonly roomLayer = new Container();
  private readonly furnitureLayer = new Container();
  private readonly agentLayer = new Container();
  private readonly overlayLayer = new Container();
  private readonly actors = new Map<string, AgentActor>();
  private readonly deskViews = new Map<string, DeskView>();
  private office: OfficeLayout | null = null;
  private employees: Employee[] = [];
  private selectedEmployeeId: string | null = null;
  private scale = 1;
  private dragging = false;
  private dragMoved = false;
  private lastPointer = { x: 0, y: 0 };
  private cameraTarget: CameraPoint | null = null;
  private readonly onSelect: (employeeId: string) => void;
  private readonly onWheel: (event: WheelEvent) => void;
  private readonly onPointerDown: (event: PointerEvent) => void;
  private readonly onPointerMove: (event: PointerEvent) => void;
  private readonly onPointerUp: () => void;
  private readonly onFocus: (event: Event) => void;

  constructor(app: Application, onSelect: (employeeId: string) => void) {
    this.app = app;
    this.onSelect = onSelect;
    this.world.sortableChildren = true;
    this.world.addChild(this.floorLayer, this.roomLayer, this.furnitureLayer, this.agentLayer, this.overlayLayer);
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
    this.employees = employees;
    const incomingIds = new Set(employees.map((employee) => employee.id));
    for (const [employeeId, actor] of this.actors) {
      if (!incomingIds.has(employeeId)) {
        actor.destroy({ children: true });
        this.actors.delete(employeeId);
      }
    }
    for (const employee of employees) {
      const existing = this.actors.get(employee.id);
      if (existing) {
        existing.syncEmployee(employee, this.office, employee.id === this.selectedEmployeeId);
      } else {
        const actor = new AgentActor(employee, this.office, this.onSelect);
        actor.syncEmployee(employee, this.office, employee.id === this.selectedEmployeeId);
        this.actors.set(employee.id, actor);
        this.agentLayer.addChild(actor);
      }
    }
    this.updateDeskMonitors();
  }

  setSelectedEmployee(employeeId: string | null): void {
    this.selectedEmployeeId = employeeId;
    for (const employee of this.employees) {
      this.actors.get(employee.id)?.syncEmployee(employee, this.office as OfficeLayout, employee.id === employeeId);
    }
  }

  focusEmployee(employeeId: string): void {
    const actor = this.actors.get(employeeId);
    if (actor) {
      this.cameraTarget = { x: actor.x, y: actor.y };
    }
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
  }

  private update(deltaMs: number): void {
    for (const actor of this.actors.values()) actor.update(deltaMs);
    this.updateCamera();
  }

  private redrawOffice(): void {
    if (!this.office) return;
    clearContainer(this.floorLayer);
    clearContainer(this.roomLayer);
    clearContainer(this.furnitureLayer);
    clearContainer(this.overlayLayer);
    this.deskViews.clear();

    this.floorLayer.addChild(new Graphics().rect(0, 0, this.office.width, this.office.height).fill(0x111821));
    this.drawFloorGrid();
    for (const room of this.office.rooms) this.drawRoom(room);
    for (const desk of this.office.desks) this.drawDesk(desk);
    for (const furniture of this.office.furniture) this.drawFurniture(furniture);
    for (const door of this.office.doors) {
      const doorGraphic = new Graphics();
      doorGraphic.roundRect(door.x, door.y, door.width, door.height, 4).fill(0x9bc4bd);
      this.overlayLayer.addChild(doorGraphic);
    }
    this.drawLegend();
    this.updateDeskMonitors();
  }

  private drawFloorGrid(): void {
    if (!this.office) return;
    const grid = new Graphics();
    for (let x = 0; x <= this.office.width; x += 40) {
      grid.moveTo(x, 0).lineTo(x, this.office.height);
    }
    for (let y = 0; y <= this.office.height; y += 40) {
      grid.moveTo(0, y).lineTo(this.office.width, y);
    }
    grid.stroke({ color: 0x26313e, width: 1, alpha: 0.28 });
    this.floorLayer.addChild(grid);
  }

  private drawRoom(room: RoomDefinition): void {
    const color = ROOM_COLORS[room.type] ?? 0x2a3542;
    const roomGraphic = new Graphics();
    roomGraphic.roundRect(room.x, room.y, room.width, room.height, 12).fill({ color, alpha: 0.95 });
    roomGraphic.roundRect(room.x, room.y, room.width, room.height, 12).stroke({ color: 0x718295, width: 2, alpha: 0.75 });
    this.roomLayer.addChild(roomGraphic);

    const label = new Text({ text: room.label, style: { fill: 0xaebdcb, fontSize: 16, fontWeight: '700' } });
    label.position.set(room.x + 16, room.y + 13);
    this.overlayLayer.addChild(label);
    if (room.type === 'workspace') {
      const subtitle = new Text({ text: 'AGENT FLOOR  /  7 WORKSPACES', style: { fill: 0x64788a, fontSize: 10, letterSpacing: 1.5 } });
      subtitle.position.set(room.x + 17, room.y + 38);
      this.overlayLayer.addChild(subtitle);
    }
  }

  private drawDesk(desk: OfficeLayout['desks'][number]): void {
    const view = new Container();
    const table = new Graphics();
    table.roundRect(desk.x, desk.y, desk.width, desk.height, 8).fill(0x886b4e).stroke({ color: 0xb28e67, width: 1.5 });
    table.roundRect(desk.x + 11, desk.y + desk.height - 7, 10, 20, 3).fill(0x4c3d31);
    table.roundRect(desk.x + desk.width - 21, desk.y + desk.height - 7, 10, 20, 3).fill(0x4c3d31);
    const monitor = new Graphics();
    monitor.roundRect(desk.monitorX, desk.monitorY, 44, 30, 4).fill(0x263545).stroke({ color: 0x91a5b9, width: 1 });
    monitor.rect(desk.monitorX + 5, desk.monitorY + 5, 34, 19).fill(0x344354);
    monitor.rect(desk.monitorX + 19, desk.monitorY + 30, 7, 7).fill(0x8b7259);
    monitor.roundRect(desk.monitorX + 10, desk.monitorY + 37, 25, 3, 2).fill(0x8b7259);
    const chair = new Graphics();
    chair.roundRect(desk.x + desk.width / 2 - 17, desk.y + desk.height + 31, 34, 13, 5).fill(0x42546b);
    chair.roundRect(desk.x + desk.width / 2 - 13, desk.y + desk.height + 14, 26, 20, 7).fill(0x526985);
    view.addChild(table, monitor, chair);
    this.furnitureLayer.addChild(view);
    this.deskViews.set(desk.id, {
      screen: monitor,
      employeeId: desk.employeeId,
      monitorX: desk.monitorX,
      monitorY: desk.monitorY,
    });
  }

  private drawFurniture(furniture: FurnitureDefinition): void {
    const item = new Graphics();
    if (furniture.kind === 'meeting-table' || furniture.kind === 'interview-table') {
      item.roundRect(furniture.x, furniture.y, furniture.width, furniture.height, 18).fill(0x6a5360).stroke({ color: 0xd19a86, width: 2 });
      item.ellipse(furniture.x + furniture.width / 2, furniture.y + furniture.height / 2, 7, 7).fill(0xe0b36c);
    } else if (furniture.kind === 'reception-counter' || furniture.kind === 'pantry-counter') {
      item.roundRect(furniture.x, furniture.y, furniture.width, furniture.height, 8).fill(0x886d56).stroke({ color: 0xc29c72, width: 1.5 });
    } else if (furniture.kind === 'cabinet') {
      item.roundRect(furniture.x, furniture.y, furniture.width, furniture.height, 5).fill(0x485d55).stroke({ color: 0x9cbca2, width: 1 });
      for (let index = 1; index < 4; index += 1) item.moveTo(furniture.x + index * 32, furniture.y + 5).lineTo(furniture.x + index * 32, furniture.y + furniture.height - 5);
      item.stroke({ color: 0x9cbca2, width: 1 });
    } else if (furniture.kind === 'coffee-machine') {
      item.roundRect(furniture.x, furniture.y, furniture.width, furniture.height, 6).fill(0x29343f).stroke({ color: 0xe0b36c, width: 1.5 });
      item.circle(furniture.x + 23, furniture.y + 13, 4).fill(0xf0c36b);
      item.rect(furniture.x + 10, furniture.y + 23, 26, 5).fill(0x6c8a91);
    } else if (furniture.kind === 'water-dispenser') {
      item.roundRect(furniture.x, furniture.y + 9, furniture.width, furniture.height - 9, 5).fill(0x6da5bd).stroke({ color: 0xb7e5ed, width: 1 });
      item.circle(furniture.x + furniture.width / 2, furniture.y + 7, 9).fill(0xa7dbe5);
    } else if (furniture.kind === 'restroom-sinks') {
      item.roundRect(furniture.x, furniture.y, furniture.width, furniture.height, 7).fill(0x455c6a).stroke({ color: 0x8eafba, width: 1 });
      for (let index = 0; index < 3; index += 1) item.circle(furniture.x + 20 + index * 32, furniture.y + 19, 7).fill(0xa7dbe5);
    }
    this.furnitureLayer.addChild(item);
    if (furniture.label) {
      const label = new Text({ text: furniture.label, style: { fill: 0xf5ddb2, fontSize: 10, fontWeight: '700', letterSpacing: 1 } });
      label.position.set(furniture.x + 10, furniture.y + furniture.height / 2 - 6);
      this.overlayLayer.addChild(label);
    }
  }

  private drawLegend(): void {
    const legend = new Container();
    legend.position.set(280, 735);
    const panel = new Graphics().roundRect(0, 0, 510, 52, 10).fill({ color: 0x101821, alpha: 0.9 }).stroke({ color: 0x4d6275, width: 1 });
    legend.addChild(panel);
    const items: Array<[string, number]> = [['待命', 0x9aa7b8], ['看手机', 0xf6c85f], ['会议中', 0x7dc4ff], ['编码中', 0x5ce1b8]];
    items.forEach(([labelText, color], index) => {
      const dot = new Graphics().circle(18 + index * 124, 26, 5).fill(color);
      const label = new Text({ text: labelText, style: { fill: 0xc3d0dc, fontSize: 11 } });
      label.position.set(29 + index * 124, 19);
      legend.addChild(dot, label);
    });
    this.overlayLayer.addChild(legend);
  }

  private updateDeskMonitors(): void {
    const employeeById = new Map(this.employees.map((employee) => [employee.id, employee]));
    for (const view of this.deskViews.values()) {
      const employee = employeeById.get(view.employeeId);
      const active = employee ? ACTIVE_STATES.includes(employee.currentState) : false;
      const color = employee?.currentState === 'testing' ? 0x64d3f4 : employee?.currentState === 'designing' ? 0xf292c8 : 0x5ce1b8;
      view.screen.clear().roundRect(view.monitorX, view.monitorY, 44, 30, 4).fill(0x263545).stroke({ color: 0x91a5b9, width: 1 });
      view.screen.rect(view.monitorX + 5, view.monitorY + 5, 34, 19).fill(active ? color : 0x344354);
      if (active) {
        view.screen.rect(view.monitorX + 9, view.monitorY + 9, 15, 2).fill(0xffffff);
        view.screen.rect(view.monitorX + 9, view.monitorY + 14, 24, 2).fill(0xffffff);
        view.screen.rect(view.monitorX + 9, view.monitorY + 19, 10, 2).fill(0xffffff);
      }
    }
  }

  private fitCamera(): void {
    if (!this.office) return;
    const width = this.app.screen.width || this.app.canvas.clientWidth || 900;
    const height = this.app.screen.height || this.app.canvas.clientHeight || 700;
    this.scale = Math.min((width - 32) / this.office.width, (height - 32) / this.office.height, 1.2);
    this.world.scale.set(this.scale);
    this.world.position.set((width - this.office.width * this.scale) / 2, (height - this.office.height * this.scale) / 2);
  }

  private handleWheel(event: WheelEvent): void {
    if (!this.office) return;
    event.preventDefault();
    const rect = this.app.canvas.getBoundingClientRect();
    const pointer = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const worldBefore = { x: (pointer.x - this.world.x) / this.scale, y: (pointer.y - this.world.y) / this.scale };
    const nextScale = Math.max(0.6, Math.min(1.8, this.scale * (event.deltaY > 0 ? 0.9 : 1.1)));
    this.scale = nextScale;
    this.world.scale.set(nextScale);
    this.world.position.set(pointer.x - worldBefore.x * nextScale, pointer.y - worldBefore.y * nextScale);
  }

  private handlePointerDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    this.dragging = true;
    this.dragMoved = false;
    this.lastPointer = { x: event.clientX, y: event.clientY };
  }

  private handlePointerMove(event: PointerEvent): void {
    if (!this.dragging) return;
    const dx = event.clientX - this.lastPointer.x;
    const dy = event.clientY - this.lastPointer.y;
    if (Math.abs(dx) + Math.abs(dy) > 2) this.dragMoved = true;
    this.world.position.x += dx;
    this.world.position.y += dy;
    this.lastPointer = { x: event.clientX, y: event.clientY };
  }

  private handlePointerUp(): void {
    this.dragging = false;
  }

  private updateCamera(): void {
    if (!this.cameraTarget || !this.office) return;
    const width = this.app.screen.width || this.app.canvas.clientWidth || 900;
    const height = this.app.screen.height || this.app.canvas.clientHeight || 700;
    const targetX = width / 2 - this.cameraTarget.x * this.scale;
    const targetY = height / 2 - this.cameraTarget.y * this.scale;
    this.world.position.x += (targetX - this.world.position.x) * 0.08;
    this.world.position.y += (targetY - this.world.position.y) * 0.08;
    if (Math.abs(targetX - this.world.position.x) < 1 && Math.abs(targetY - this.world.position.y) < 1) this.cameraTarget = null;
  }
}

function clearContainer(container: Container): void {
  container.removeChildren().forEach((child) => child.destroy());
}
