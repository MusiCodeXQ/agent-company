# 可视化 AI 公司（Virtual AI Company）产品与技术实施规格

> 目标读者：Codex / 其他 Coding Agent
>
> 文档用途：把本文件放入项目 `docs/AI_COMPANY_PRODUCT_SPEC.md`，由 Coding Agent 依据本文直接创建第一版可运行系统。
>
> 项目代号：`ai-company`
>
> 第一版原则：先把“真实可运行 + 可观察 + 可扩展”做通，不追求电影级画面，不引入不必要的重型基础设施。

---

## 0. 最高优先级指令

这是一个“多 AI Agent 组成的虚拟公司”，不是普通聊天页面，也不是纯游戏。

系统必须同时具备两层：

1. **真实 Agent 工作层**：员工 Agent 能接收任务、思考、沟通、开会、招聘、面试、开发、测试、评审，后续可接入 Shell/Git/浏览器/MCP 等工具。
2. **办公室可视化层**：每一个 Agent 都是办公室里的一个独立小人。真实工作状态会驱动人物动作和位置。例如收到消息时看手机，开发时走回工位并点亮电脑，开会时走入会议室，面试时进入面试室。

不要把小人动作交给 LLM 逐帧决定。走路、坐下、去茶水间、随机休息等必须是本地状态机/规则系统，不能浪费 Token。

第一版要做到：即使没有任何 LLM API Key，也能通过 Mock Agent 模式完整演示招聘、面试、开会、写代码状态、测试状态等流程。

---

# 1. 产品愿景

用户打开网页后看到的是一家俯视/斜 45 度视角的软件公司。办公室里有工位、电脑、会议室、面试室、前台、HR 区、茶水间和卫生间。每个 AI Agent 都有独立身份、角色、名字、年龄、性别、性格、技能、模型配置和长期状态，并有稳定不变的外观。

用户相当于这家公司的老板/CEO。用户不是分别去跟七个机器人聊天，而是直接对“公司”下达目标，例如：

> 做一个支持语音视频、多人通话、强插、强拆的企业调度系统，后端要求 Rust，数据库兼容 SQLite/PostgreSQL。

系统中的主管（Supervisor）和产品经理会拆解需求，组织会议并派发任务。HR 可以根据团队缺口发起招聘。如果用户直接说：

> 我需要一个懂 Rust、WebRTC、音视频和 TURN/STUN 的高级架构师。

HR 应自动创建招聘需求、生成/检索候选 Agent、组织面试、邀请技术面试官、输出评分和录用建议。被录用的人会成为一个新的持久化 Agent，进入办公室，分配工位，并可以参与后续真实工作。

这套产品的核心价值不是“动画”，而是把 Agent 的工作过程做成一个**可视化的多 Agent 操作系统和调试器**。

---

# 2. 第一版默认公司组织

系统首次启动时创建一家公司和 7 名默认员工。以下人物均为虚构 Agent，数据用于 Demo，可以由用户修改。

| ID | 姓名 | 年龄 | 性别 | 职位 | 核心性格 | 默认职责 |
|---|---|---:|---|---|---|---|
| supervisor-01 | 林屿 | 36 | 男 | 主管 / Supervisor | 冷静、克制、结果导向 | 调度、仲裁、验收、Review |
| hr-01 | 苏晴 | 31 | 女 | HR | 外向、细致、善于观察 | 招聘、面试组织、人员档案 |
| product-01 | 周启 | 33 | 男 | 产品经理 | 好奇、结构化、表达直接 | PRD、需求拆解、验收标准 |
| ui-01 | 沈知夏 | 28 | 女 | UI/UX 设计师 | 开放、审美敏锐、温和 | 页面结构、交互、视觉规范 |
| frontend-01 | 陈默 | 30 | 男 | 前端工程师 | 专注、务实、低社交 | Web/UI 代码实现 |
| backend-01 | 顾言 | 34 | 男 | 后端工程师 | 严谨、谨慎、系统思维 | API、数据库、服务端代码 |
| qa-01 | 唐宁 | 27 | 女 | 测试工程师 | 怀疑精神强、耐心 | 测试、回归、缺陷验证 |

默认员工数量不是限制。系统后续可招聘 DevOps、DBA、安全工程师、算法工程师、架构师、Android、iOS、运营等任意新角色。

**重要：年龄、性别只用于角色身份、人物外观和叙事，不得作为招聘评分项。招聘评分只能依据技能匹配、经验、沟通表现、任务适配度和团队工作方式。**

---

# 3. HR 招聘与面试系统

## 3.1 用户入口

左侧 CEO Console 支持自然语言输入。以下表达都应触发招聘意图：

- “招一个 Rust 架构师。”
- “我还需要一个 Android 工程师，懂 Kotlin 和 RN。”
- “找一个擅长音视频的后端。”
- “团队缺一个 DevOps。”

系统将消息先交给 Supervisor/Intent Router。判断为 `HIRING_REQUEST` 后转给 HR。

## 3.2 招聘流程

招聘使用明确状态机，不允许完全依赖自由对话：

```text
DRAFT
  -> SOURCING
  -> CANDIDATES_READY
  -> INTERVIEW_SCHEDULED
  -> INTERVIEWING
  -> EVALUATING
  -> OFFER_PENDING
  -> HIRED | REJECTED | CANCELLED
```

### SOURCING（找人）

V1 不抓取真实世界求职者，不使用真实个人信息。HR 根据 Job Requirement 生成 3~5 个**虚构候选 Agent**。

每个候选人至少包含：

```json
{
  "id": "candidate-uuid",
  "name": "许川",
  "age": 35,
  "gender": "male",
  "role": "Realtime Media Architect",
  "yearsOfExperience": 11,
  "skills": [
    {"name": "Rust", "level": 92},
    {"name": "WebRTC", "level": 95},
    {"name": "TURN/STUN", "level": 89},
    {"name": "PostgreSQL", "level": 78}
  ],
  "personality": {
    "openness": 78,
    "conscientiousness": 91,
    "extraversion": 42,
    "agreeableness": 66,
    "emotionalStability": 84,
    "traits": ["系统思维", "谨慎", "不喜欢无效会议"],
    "communicationStyle": "先确认约束，再提出方案"
  },
  "resumeSummary": "...",
  "expectedContribution": "...",
  "risks": ["对 UI 领域经验较少"],
  "appearance": {},
  "source": "synthetic"
}
```

## 3.3 面试可视化

面试必须发生在办公室中的 `interview-room`。

流程：

1. 候选人从前台/访客出生点出现。
2. HR 从自己的工位起身，走到前台或面试室。
3. 候选人走进面试室并坐下。
4. 根据岗位自动邀请 1 名技术面试官；高级岗位可额外邀请 Supervisor。
5. 面试开始，人物头顶可以短暂显示说话气泡；右侧面板显示完整文字记录。
6. 面试结束后候选人离开会议室。
7. 面试官分别打分。
8. HR 汇总评分并建议录用/拒绝。
9. 若录用，则候选记录转为 Employee，产生稳定 employee ID，分配工位，外观保持不变。
10. 新员工从前台进入公司，走到新工位，桌面电脑点亮一次作为入职动画。

## 3.4 面试角色

默认至少三个视角：

- HR：沟通、工作方式、岗位动机（虚拟 Agent 场景中的“岗位动机”指工作偏好）。
- Role Interviewer：技术能力、问题解决能力。
- Supervisor：高级岗位/关键岗位的架构判断和团队协作判断。

## 3.5 面试评分模型

总分 100，建议默认：

```text
技术/专业能力        45
任务匹配度            20
问题解决能力          15
沟通与协作            10
角色工作方式匹配       10
```

年龄、性别、外貌不得进入评分公式。

每个评分必须附带 evidence（证据），指向面试记录中的具体回答。

## 3.6 候选人不是一次性 Prompt

面试期间 Candidate 必须拥有临时 Agent Session：

```text
Candidate Profile
+ Resume
+ Personality
+ Job Requirement
+ Interview Transcript
= Candidate Agent Context
```

这样同一个候选人在连续追问时应保持一致，不允许每一问人格重置。

---

# 4. 员工 Agent 数据模型

每个员工都必须是持久化实体，而不是 UI 上临时写死的对象。

建议 TypeScript 领域模型：

```ts
export interface Employee {
  id: string;
  companyId: string;
  name: string;
  age: number;
  gender: 'male' | 'female' | 'non_binary';
  role: string;
  department: string;
  title: string;
  status: EmployeeStatus;
  personality: PersonalityProfile;
  skills: SkillProfile[];
  appearance: AppearanceProfile;
  modelConfig: AgentModelConfig;
  workspace?: WorkspaceBinding;
  deskId?: string;
  hiredAt: string;
}
```

性格建议采用 5 个稳定数值 + 文字特征：

```ts
export interface PersonalityProfile {
  openness: number;              // 0..100
  conscientiousness: number;     // 0..100
  extraversion: number;          // 0..100
  agreeableness: number;         // 0..100
  emotionalStability: number;    // 0..100
  traits: string[];
  communicationStyle: string;
  decisionStyle: string;
}
```

性格必须“弱影响”Agent 表现，例如：

- 外向高：会议中更容易主动发言。
- 尽责性高：更倾向于检查验收条件。
- 开放性高：更愿意提出替代方案。
- 稳定性低：面对连续失败时文字可以略显急躁，但不允许破坏任务质量。

性格不能成为随机失控机制。所有 Agent 仍必须遵循角色职责和系统规则。

---

# 5. 每个人必须长得不一样：外观系统

## 5.1 不要求第一版准备美术资源

第一版直接使用 PixiJS `Graphics` 程序化绘制人物。不要因为缺 PNG 素材而阻塞项目。

每个人由多层组合：

```text
shadow
legs
body / clothes
neck
head
hair_back
face
hair_front
accessories
held_item(phone/cup)
```

第一版人物可以是简洁的 2D/2.5D 卡通矢量风格，不需要像素级写实。

## 5.2 AppearanceProfile

```ts
export interface AppearanceProfile {
  seed: number;
  skinTone: string;
  hairStyle: string;
  hairColor: string;
  topStyle: string;
  topColor: string;
  bottomStyle: string;
  bottomColor: string;
  shoeColor: string;
  bodyType: 'slim' | 'regular' | 'broad';
  heightScale: number; // 0.92 ~ 1.08，仅用于视觉差异
  glasses: boolean;
  beardStyle?: string;
  accessory?: 'watch' | 'headphones' | 'badge' | 'none';
}
```

`appearance.seed` 一经入职必须持久化。刷新网页后不能换脸。

## 5.3 外观生成方法

实现函数：

```ts
createAppearance(seed: number, gender: Gender, age: number): AppearanceProfile
```

使用 Seeded RNG，从预设池中稳定选择发型、服装、配色、配饰。

年龄只影响很轻的视觉特征，例如发型候选池、胡须概率、服装风格；不能做夸张或冒犯性的年龄刻板化。

## 5.4 后续替换正式素材

程序化人物和正式 Sprite 必须共享同一个 `AppearanceProfile`。未来可增加：

```text
renderer = procedural | spritesheet | skeletal
```

业务代码、员工 ID、状态机不变，只更换渲染器。

---

# 6. 办公室界面不需要人工“画数据”

办公室本身必须由数据驱动。

创建：

`apps/web/src/office/layout/default-office.ts`

或者 JSON：

`apps/web/src/office/layout/default-office.json`

建议内部逻辑坐标：`1600 x 900`，Canvas 自适应缩放。

核心数据结构：

```ts
export interface OfficeLayout {
  width: number;
  height: number;
  rooms: RoomDefinition[];
  furniture: FurnitureDefinition[];
  desks: DeskDefinition[];
  seats: SeatDefinition[];
  doors: DoorDefinition[];
  spawnPoints: SpawnPoint[];
  navigationObstacles: Rect[];
}
```

示例：

```ts
export const defaultOffice: OfficeLayout = {
  width: 1600,
  height: 900,
  rooms: [
    { id: 'open-office', type: 'workspace', x: 260, y: 120, width: 850, height: 580 },
    { id: 'meeting-a', type: 'meeting', x: 1130, y: 120, width: 390, height: 250 },
    { id: 'interview-room', type: 'interview', x: 1130, y: 390, width: 390, height: 210 },
    { id: 'hr-office', type: 'office', x: 40, y: 120, width: 190, height: 260 },
    { id: 'reception', type: 'reception', x: 40, y: 410, width: 190, height: 180 },
    { id: 'pantry', type: 'pantry', x: 1130, y: 620, width: 220, height: 200 },
    { id: 'restroom', type: 'restroom', x: 1370, y: 620, width: 150, height: 200 }
  ],
  furniture: [],
  desks: [],
  seats: [],
  doors: [],
  spawnPoints: [
    { id: 'visitor-entry', x: 85, y: 640 }
  ],
  navigationObstacles: []
};
```

**Codex 要直接把办公室“画出来”**：

- floor：Graphics 矩形。
- walls：Graphics line/rect。
- desks：圆角矩形 + 桌腿。
- computer：显示器矩形 + 屏幕状态。
- chairs：小椅子图形。
- meeting table：大圆角矩形。
- interview room：桌 + 3~4 个座位。
- pantry：咖啡机/饮水机图标化绘制。
- restroom：只显示门、隔间/图标，不需要人物内部细节。
- doors：保留通行点。

第一版画面质量目标：类似管理模拟游戏的简洁原型，而不是方框后台管理系统。

---

# 7. 前端技术方案

## 7.1 技术栈

必须采用：

```text
React
TypeScript
Vite
PixiJS 8
Zustand
原生 WebSocket
```

React 负责普通 DOM UI：CEO Console、右侧员工详情、任务面板、招聘面板、面试记录。

PixiJS 只负责办公室 Canvas。

**第一版优先直接使用纯 PixiJS API，而不是把整个应用做成 React-Pixi 组件树。** React 中只创建一个 `OfficeCanvas` 组件并挂载 Pixi `Application`。

## 7.2 Canvas 结构

```text
OfficeCanvas
  └── Pixi Application
      └── WorldContainer
          ├── FloorLayer
          ├── RoomLayer
          ├── FurnitureLayer
          ├── AgentLayer
          ├── EffectLayer
          └── OverlayLayer
```

AgentLayer 内每个员工对应一个 `AgentActor`。

## 7.3 镜头

第一版：

- 鼠标滚轮缩放。
- 鼠标拖动画布平移。
- 双击员工或点击右侧“定位”按钮，镜头平滑移动到员工。
- 支持 0.6x ~ 1.8x 缩放。
- 内部世界坐标固定，DOM 不要跟着 Canvas 每帧重排。

---

# 8. Agent Actor 与动画状态

统一视觉状态：

```ts
export type ActorState =
  | 'idle'
  | 'walking'
  | 'checking_phone'
  | 'thinking'
  | 'coding'
  | 'designing'
  | 'testing'
  | 'reviewing'
  | 'meeting'
  | 'interviewing'
  | 'waiting'
  | 'coffee'
  | 'restroom'
  | 'error'
  | 'offline';
```

动作映射：

| 状态 | 可视化表现 |
|---|---|
| idle | 工位/公共区轻微 idle 动画 |
| walking | 角色朝目标点移动 |
| checking_phone | 手持手机，手机亮起 |
| thinking | 头顶 `...`，短暂思考图标 |
| coding | 坐在分配工位，电脑屏幕亮起，键盘动作 |
| designing | UI 工位显示设计画板/窗口 |
| testing | QA 屏幕快速滚动测试条目 |
| reviewing | 电脑旁显示 diff/review 图标 |
| meeting | 坐在会议室，参与说话轮次 |
| interviewing | 坐在面试室 |
| coffee | 走到茶水间停留 |
| restroom | 走到卫生间门口后角色隐藏短时间，再出现 |
| error | 屏幕出现错误标记，角色停留 |

卫生间、喝咖啡、普通走动属于 ambient behavior。不要调用 LLM。

---

# 9. 移动与寻路

不要由服务端持续推送人物 `x,y`。

服务端只发送语义目标：

```json
{
  "type": "actor.destination.changed",
  "employeeId": "backend-01",
  "destination": {
    "kind": "desk",
    "id": "desk-backend-01"
  }
}
```

浏览器根据 OfficeLayout 解析目标坐标并本地寻路。

V1 可先实现简化 waypoint 路径；如果障碍已经明显影响走路，再实现网格 A*。

要求：角色不能穿墙、不能穿会议桌、不能穿工位。

---

# 10. 真实状态驱动动画

前端禁止随意伪造“正在开发”的状态。

后端 Agent Runtime 必须发送领域事件：

```json
{
  "eventId": "evt-...",
  "type": "employee.state.changed",
  "timestamp": "2026-08-10T09:00:00Z",
  "employeeId": "backend-01",
  "payload": {
    "from": "thinking",
    "to": "coding",
    "taskId": "task-123"
  }
}
```

前端收到后：

```text
coding
=> destination = employee.deskId
=> walking
=> 到达工位
=> sit
=> monitor.power = on
=> typing animation
```

这样动画就是后端状态的表现层。

---

# 11. 消息系统

用户和 Agent、Agent 与 Agent 之间都通过统一消息实体：

```ts
interface CompanyMessage {
  id: string;
  companyId: string;
  projectId?: string;
  fromType: 'user' | 'employee' | 'system';
  fromId: string;
  toType: 'company' | 'employee' | 'group';
  toId: string;
  content: string;
  createdAt: string;
}
```

消息到达员工时：

1. 事件 `message.received`。
2. 员工状态切为 `checking_phone` 1~3 秒（纯 UI/规则延迟）。
3. 进入 `thinking`。
4. Agent Runtime 决定后续行为。

不要给每条内部系统事件制造聊天气泡。气泡只显示真正的自然语言消息。

## 11.1 受治理的面对面 A2A

Agent 与 Agent 可以使用 A2A HTTP+JSON 协议交换 `Message`、创建有生命周期的 `Task` 并返回 `Artifact`，但员工 Runtime 不得彼此直连。正式通信必须经过 Company A2A Gateway，并与公司的 Message、Agent Job、Goal/Task 和审计记录关联持久化。

面对面 A2A 的强制流程：

```text
创建 FaceToFaceSession
-> 后端预约同一房间内的两个交流座位
-> Spatial Simulation 驱动双方移动
-> 后端确认双方到场后激活 A2A contextId
-> 使用短期 capability token 进行 message:send
-> 任一方离开、超时、超轮次或超 Token 预算时拒绝后续通信
```

前端动画不是权限依据。每一次 A2A 发送都必须由 Rust 后端重新校验双方的 `location_id`、`room_id` 和 `active_movement_id`。会话必须限制交替发言、最大轮数、单个在途 Task、Token 总预算和有效期；明文 capability token 只在创建时返回，数据库只保存摘要。

---

# 12. 会议系统

会议是持久化实体：

```ts
interface Meeting {
  id: string;
  title: string;
  type: 'planning' | 'design_review' | 'technical' | 'standup' | 'interview' | 'retrospective';
  roomId: string;
  participantIds: string[];
  status: 'scheduled' | 'gathering' | 'in_progress' | 'finished' | 'cancelled';
  agenda: string[];
  transcript: MeetingUtterance[];
  decisions: string[];
  actionItems: ActionItem[];
}
```

会议开始前：

```text
scheduled
-> gathering
-> 所有员工获取 meeting seat destination
-> 人员到齐/超时
-> in_progress
```

每人按顺序发言，不允许 6 个 LLM 同时无限互聊。会议必须设置：

- 最大轮数，例如 8~16 turns。
- 最大 Token 预算。
- 明确 agenda。
- 结束条件。
- Supervisor/meeting host 总结 decisions 和 action items。

---

# 13. Agent Runtime

## 13.1 后端技术栈

```text
Rust stable
Axum
Tokio
Serde / serde_json
SQLx
SQLite WAL
tracing
reqwest
```

V1 不使用 Redis、Kafka、NATS、Kubernetes。

未来需要水平扩展时再替换事件总线/任务队列。

## 13.2 核心模块

建议：

```text
apps/server/src/
  main.rs
  app_state.rs
  config.rs
  api/
  ws/
  db/
  domain/
    company/
    employee/
    hiring/
    meeting/
    task/
    message/
    event/
  agent/
    runtime.rs
    orchestrator.rs
    prompt.rs
    model.rs
    memory.rs
    tool_policy.rs
    mock.rs
  llm/
    mod.rs
    openai_compatible.rs
  office/
    coordinator.rs
```

## 13.3 Orchestrator 原则

业务流程由代码状态机负责，LLM 负责语义工作。

代码负责：

- 谁应该收到消息。
- 是否需要创建会议。
- 哪个会议室空闲。
- 谁应该参加面试。
- 最大会议轮数。
- 招聘状态迁移。
- 任务依赖关系。
- Token/费用预算。
- 超时和错误恢复。

LLM 负责：

- 需求理解。
- 生成 PRD/方案。
- 候选人资料生成。
- 面试回答和追问。
- 技术方案讨论。
- 代码任务规划。
- Review 意见。

---

# 14. LLM 接入

第一版实现 `OpenAICompatibleClient`，不要把业务代码绑定单一供应商。

配置：

```env
LLM_ENABLED=false
LLM_BASE_URL=https://example.com/v1
LLM_API_KEY=
LLM_MODEL=
LLM_TIMEOUT_SECONDS=120
```

接口：

```rust
#[async_trait]
pub trait LlmClient {
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse>;
}
```

后续再增加 Anthropic/Gemini adapter。

## 14.1 必须支持 Mock 模式

`LLM_ENABLED=false` 时：

- HR 能生成固定但看起来真实的 4 个候选人。
- 面试有预制回答。
- Supervisor 能创建一个 Demo 项目。
- 各员工状态能完整流转。

前端开发和自动测试不能依赖真实 API Key。

---

# 15. Agent Prompt 结构

不要把几十页公司规则重复塞给每一个 Agent。

建议运行时 Prompt：

```text
SYSTEM BASE
+ Company Rules
+ Employee Identity
+ Role Responsibilities
+ Personality Profile
+ Current Project Context (摘要)
+ Current Task
+ Relevant Memory
+ Current Conversation/Meeting Transcript (裁剪)
```

每个 Agent 输出尽量使用结构化 JSON：

```json
{
  "thoughtSummary": "需要先确认接口边界",
  "action": "request_meeting",
  "targets": ["product-01", "frontend-01"],
  "message": "建议确认登录状态和错误码规范。",
  "artifact": null
}
```

不要暴露隐藏 chain-of-thought。`thoughtSummary` 只能是简短、可展示的工作摘要。

---

# 16. 任务系统

```ts
interface Task {
  id: string;
  projectId: string;
  title: string;
  description: string;
  type: 'product' | 'design' | 'frontend' | 'backend' | 'test' | 'review' | 'ops';
  status: 'todo' | 'ready' | 'in_progress' | 'blocked' | 'review' | 'done' | 'failed';
  assigneeId?: string;
  dependsOn: string[];
  acceptanceCriteria: string[];
  workspaceId?: string;
}
```

员工办公室状态和 Task 状态相关联，但两者不要等同。

例如：Task 为 `in_progress`，Employee 可以临时处于 `meeting`。

---

# 17. 代码开发工作区（第二阶段即可接入，但结构第一阶段要预留）

后续 coding Agent 必须绑定真实 workspace。

推荐：

```text
workspaces/
  project-001/
    main/
    employee-backend-01/
    employee-frontend-01/
    employee-qa-01/
```

每个开发 Agent 使用 Git worktree / 独立 branch，避免互相覆盖。

例如：

```text
agent/backend-01/task-123
agent/frontend-01/task-124
```

工具层预留：

```text
read_file
search_code
write_file
apply_patch
run_command
run_tests
git_status
git_diff
git_commit
browser_open
browser_screenshot
```

真正运行 Shell 时必须加 sandbox，不得默认给宿主机无限权限。

---

# 18. 后端 API

V1 至少提供：

```text
GET    /api/health
GET    /api/bootstrap
GET    /api/companies/:companyId
GET    /api/companies/:companyId/employees
GET    /api/companies/:companyId/tasks
GET    /api/companies/:companyId/meetings
GET    /api/companies/:companyId/hiring-requests

POST   /api/companies/:companyId/messages
POST   /api/companies/:companyId/hiring-requests
POST   /api/hiring-requests/:id/generate-candidates
POST   /api/candidates/:id/schedule-interview
POST   /api/interviews/:id/start
POST   /api/interviews/:id/hire
POST   /api/interviews/:id/reject

GET    /api/events?after=<eventId>
GET    /ws
```

`GET /api/bootstrap` 返回前端首屏所需聚合数据：

```json
{
  "company": {},
  "employees": [],
  "tasks": [],
  "meetings": [],
  "hiringRequests": [],
  "office": {}
}
```

---

# 19. WebSocket 事件协议

统一 Event Envelope：

```ts
interface DomainEvent<T = unknown> {
  eventId: string;
  companyId: string;
  type: string;
  timestamp: string;
  actorId?: string;
  correlationId?: string;
  payload: T;
}
```

事件名至少包含：

```text
message.sent
message.received
employee.state.changed
employee.destination.changed
employee.hired
employee.profile.updated
meeting.created
meeting.gathering
meeting.started
meeting.utterance
meeting.finished
hiring.request.created
hiring.candidates.ready
interview.scheduled
interview.started
interview.utterance
interview.scored
interview.finished
candidate.hired
candidate.rejected
task.created
task.assigned
task.state.changed
agent.error
```

前端 Zustand Store 处理领域状态；Pixi Office Engine 订阅 store 的必要变化，不要让 Pixi 直接承担整个业务状态管理。

---

# 20. SQLite 数据库

V1 建议表：

```text
companies
employees
employee_skills
employee_memories
projects
tasks
messages
meetings
meeting_participants
meeting_utterances
hiring_requests
candidates
interviews
interview_participants
interview_utterances
interview_scores
events
agent_runs
llm_calls
tool_calls
workspaces
```

重要字段可以先 JSON 存储，减少过早拆表，例如 personality / appearance / model_config。

SQLite：

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
```

迁移放 `apps/server/migrations/`。

---

# 21. Event Store 与回放

所有关键业务变化必须写入 `events`。

未来时间轴回放直接按事件恢复办公室行为：

```text
09:31 CEO 下达目标
09:32 Supervisor 阅读消息
09:33 Product 开始需求分析
09:36 创建需求会议
09:37 参与者前往会议室
09:42 会议结束
09:43 Backend 开始编码
09:45 Frontend 等待 UI
09:58 QA 收到测试任务
```

第一版 UI 右侧先提供事件时间线，不必完成完整可拖拽回放，但数据库和 API 必须为此保留事件。

---

# 22. 页面布局

桌面端优先，最小推荐宽度 1280。

```text
┌─────────────────────────────────────────────────────────────────────────┐
│ 顶栏：公司名 / 当前项目 / 在线人数 / Token / 设置                        │
├───────────────┬───────────────────────────────────┬─────────────────────┤
│               │                                   │                     │
│ CEO Console   │                                   │ Employee / Task     │
│               │         PIXI OFFICE               │ Detail Panel        │
│ 消息          │                                   │                     │
│ 项目          │   工位 / 会议室 / 面试室 / HR     │ 员工档案            │
│ 招聘          │   小人实时走动                    │ 当前任务            │
│ 会议          │                                   │ 活动记录            │
│               │                                   │                     │
├───────────────┴───────────────────────────────────┴─────────────────────┤
│ 底部可选：Event Timeline                                                │
└─────────────────────────────────────────────────────────────────────────┘
```

左侧导航至少：

```text
公司
消息
项目
招聘
会议
员工
```

点击员工时，右侧显示：

```text
头像/程序化人物预览
姓名 / 年龄 / 性别
职位 / 部门
性格
技能
Agent 模型
当前状态
当前任务
所在位置
最近消息
最近 Agent Run
Token / 费用（有真实模型时）
定位员工
```

---

# 23. 招聘页面

招聘页面至少包含：

1. 招聘需求列表。
2. 需求详情。
3. 候选人卡片。
4. 候选人技能雷达图不是第一版必要项，优先用可读数值条。
5. “安排面试”。
6. “进入面试现场/定位到面试室”。
7. 面试文字记录。
8. 面试官评分。
9. “录用 / 拒绝”。

用户也可以不进招聘页面，全程从办公室观看，但详细数据必须能查。

---

# 24. 第一版 Office Demo 剧情（必须实现）

提供一个“Demo”按钮，一键演示：

```text
1. CEO 发送：招聘一名 Rust/WebRTC 高级工程师
2. HR 手机亮起，进入 checking_phone
3. HR thinking
4. 招聘请求创建
5. 生成 4 个候选人
6. HR 选择其中 1 个进入面试
7. 候选人从 reception 出现
8. HR + Backend + Supervisor 走入 interview-room
9. 三轮面试对话
10. 展示评分
11. 自动选择“录用”或由用户点击“录用”
12. 候选人转为 Employee
13. 为新员工自动创建 desk
14. 新员工从前台走到工位
15. 电脑亮起
16. 员工列表出现新员工
```

这条 Demo 是第一版的主要验收链路。

---

# 25. 性能要求

目标不是 3A 游戏。

第一版性能目标：

- 20~50 个人物仍保持流畅。
- Pixi ticker 内避免 React setState。
- 人物 actor 对象常驻，不每帧创建对象。
- DOM 面板不要每帧更新。
- WebSocket 不发送逐帧坐标。
- ambient animation 客户端运行。
- Canvas 根据设备像素比设置合理上限，避免超高 DPR 过度消耗 GPU。
- 静止时尽量减少不必要的复杂绘制。

不要添加物理引擎，除非确有必要。

---

# 26. 推荐项目目录

```text
ai-company/
  AGENTS.md
  README.md
  .env.example
  docker-compose.yml
  docs/
    AI_COMPANY_PRODUCT_SPEC.md
    architecture.md
  apps/
    web/
      package.json
      src/
        app/
        api/
        store/
        features/
          company/
          employees/
          hiring/
          meetings/
          tasks/
        office/
          OfficeCanvas.tsx
          engine/
          actors/
          renderers/
          layout/
          navigation/
          effects/
        styles/
    server/
      Cargo.toml
      migrations/
      src/
  scripts/
  workspaces/
    .gitkeep
```

---

# 27. Docker

V1 `docker-compose.yml` 只需要：

```text
server
web（可选，开发模式也可直接 npm run dev）
```

SQLite 文件通过 volume 保存。

不要为了 7 个 Agent 加 Redis/Postgres。

生产构建：

- web => 静态资源。
- server => Rust release binary。
- 可由 Rust server 静态托管 web dist，最终实现单容器部署；开发阶段可分开。

---

# 28. 安全要求

第一阶段没有真实 Shell 工具时仍要预留 Tool Policy。

后续接入代码执行必须：

- 默认只写项目 workspace。
- 禁止读取宿主 SSH key、浏览器数据、用户主目录敏感文件。
- 禁止默认挂载 Docker socket。
- 危险命令必须通过策略层。
- 每次 tool call 写入 `tool_calls`。
- 支持人工审批状态。

---

# 29. Codex 实现顺序（必须按阶段，不要同时铺开）

## Phase 1：初始化骨架

完成：

```text
React/Vite/Pixi web
Rust/Axum server
SQLite migrations
/api/health
/api/bootstrap
/ws
```

要求能 `npm run dev` + `cargo run` 启动。

## Phase 2：办公室静态场景

完成：

- OfficeLayout 数据。
- rooms/furniture/desks。
- 7 个默认员工。
- 程序化人物。
- 每个人外观明显不同。
- 平移/缩放/员工选中。

不要接 LLM。

## Phase 3：状态机与事件

完成：

- WebSocket。
- employee state event。
- walking。
- phone / thinking / coding / meeting 动画。
- 事件时间线。

## Phase 4：招聘与面试（Mock）

完成本文第 24 章完整 Demo。

## Phase 5：真实 LLM

加入 OpenAI-compatible endpoint，保留 Mock 模式。

## Phase 6：真实 Coding Tool Runtime

再做 Git worktree、Docker sandbox、Shell、测试等。

**不要在 Phase 2 尚不可用时直接跳到 Phase 6。**

---

# 30. 第一版验收标准（Definition of Done）

以下全部满足才算第一版完成：

- [ ] 一个命令/清晰步骤可以启动前后端。
- [ ] 页面不是传统后台表格，而是以办公室 Canvas 为核心。
- [ ] 7 个默认员工都显示在办公室。
- [ ] 7 人姓名、年龄、性别、性格、职位均来自后端数据。
- [ ] 7 人外观明显不同，刷新后保持一致。
- [ ] 员工可以从工位走到会议室。
- [ ] 收到消息有看手机动作。
- [ ] coding 时会自动回工位、坐下、点亮电脑。
- [ ] HR 是默认角色。
- [ ] 用户可以输入一个招聘需求。
- [ ] HR 能生成至少 4 个虚构候选 Agent。
- [ ] 候选人拥有姓名、年龄、性别、性格、技能、履历摘要和独立外观。
- [ ] 候选人能从前台进入面试室。
- [ ] HR + 技术面试官可以进行 Mock 面试。
- [ ] 面试记录和评分可以在 UI 查看。
- [ ] 点击“录用”后候选人变成员工。
- [ ] 新员工获得稳定 employee ID 和 desk。
- [ ] 新员工出现在办公室并走向自己的工位。
- [ ] 刷新页面以后新员工仍存在。
- [ ] SQLite 使用 WAL。
- [ ] 有事件表并记录关键状态变化。
- [ ] 无 LLM Key 时 Demo 仍完整可运行。
- [ ] 前端 lint/typecheck/build 通过。
- [ ] Rust fmt/clippy/test 通过。

---

# 31. Coding Agent 工作规则

执行本文时遵守：

1. 不要只输出方案，要实际创建文件和代码。
2. 每完成一个 Phase，运行测试/构建并修复错误。
3. 不要为了“架构完整”提前添加 Redis、Kafka、Kubernetes、微服务。
4. 不要使用 `any` 大量逃避 TypeScript 类型。
5. 不要把所有逻辑塞进一个 React 文件或一个 Rust 文件。
6. Office Canvas 不允许每一帧触发 React render。
7. 不要伪造后端状态；所有重要状态来自 domain event。
8. 不要把 API Key 写进仓库。
9. Mock 和 Real LLM 必须共享相同的领域接口。
10. 重要流程必须可测试。
11. 遇到本文没有明确规定的小问题，选择最简单、可维护的实现并继续，不要停止等待确认。
12. 如果发生依赖版本冲突，选择当前稳定、兼容的版本，并在 README 记录。

---

# 32. 第一轮 Codex 任务（直接执行）

当你第一次读取本文时，不要试图一次做完全部系统。

**第一轮目标：完成 Phase 1 + Phase 2 + 最小 Phase 3。**

必须实际实现：

```text
1. 创建 monorepo 目录。
2. 创建 Rust Axum server。
3. 创建 SQLite migrations 和 seed。
4. 创建 React + TypeScript + Vite web。
5. 接入 PixiJS 8。
6. 实现 defaultOffice 数据。
7. 用 Graphics 直接画办公室，不依赖外部美术素材。
8. 创建 7 个默认员工数据。
9. 实现 AppearanceProfile + seeded appearance generator。
10. 画出 7 个不同的人。
11. 让员工各自在自己的工位。
12. 点击员工时显示右侧资料。
13. 实现一个 Mock Event 按钮：让 Backend 员工看手机 -> 走到会议室 -> 坐下 -> 回工位 -> coding。
14. 实现 /api/bootstrap，从 Rust 后端加载员工和办公室数据。
15. 实现 WebSocket 连接骨架。
16. 提供 README 启动方法。
17. 执行前端 build/typecheck 与 Rust fmt/clippy/test。
```

完成第一轮后，给出：

```text
- 已完成内容
- 目录结构
- 启动命令
- 测试结果
- 当前截图可观察到的行为（文字说明即可）
- 下一阶段建议：招聘/面试 Mock 流程
```

---

# 33. 第二轮 Codex 任务：招聘/面试

第一轮稳定后执行：

```text
1. 新增 hiring_requests / candidates / interviews 相关 migration。
2. 完成 Mock HR Candidate Generator。
3. CEO Console 能提交招聘需求。
4. HR 收消息 -> 看手机 -> thinking。
5. 生成 4 个候选人。
6. 招聘页显示候选资料。
7. 给每个候选人生成稳定 AppearanceProfile。
8. 安排面试。
9. 候选人作为 Visitor Actor 从 reception spawn。
10. HR/技术面试官/Supervisor 走到 interview-room。
11. 执行三轮 Mock 对话。
12. 生成评分。
13. 用户可以录用/拒绝。
14. 录用后把 Candidate 转成 Employee。
15. 自动生成 desk，并更新 OfficeLayout 的运行时实例。
16. 新员工走到工位。
17. 刷新页面确认持久化。
18. 为招聘状态机写后端测试。
```

---

# 34. 第三轮 Codex 任务：真实模型

仅在 Mock 招聘完整稳定以后：

```text
1. 实现 LlmClient trait。
2. 实现 OpenAICompatibleClient。
3. 所有模型配置通过 .env / DB 管理。
4. Candidate Generation 要求模型返回 JSON Schema 兼容结果。
5. Interview Agent 保持 Session Context。
6. 面试评分要求 evidence。
7. 增加 llm_calls 表记录 model/input tokens/output tokens/latency/cost（若供应商可得）。
8. LLM 失败自动回退可读错误，不得让整个公司状态卡死。
```

---

# 35. README 中必须提供的启动方式

优先提供 Windows + PowerShell 与通用方式。

示例目标：

```powershell
# terminal 1
cd apps/server
cargo run

# terminal 2
cd apps/web
npm install
npm run dev
```

以及 Docker：

```powershell
docker compose up --build
```

不要要求用户安装复杂的本地集群。

---

# 36. 对 Codex 的最终要求

你正在开发的是一个长期演进的产品，不是 Demo 页面。

第一版 UI 可以简洁，但领域模型、事件边界和状态机必须正确；画面可以先是程序化简洁风格，但必须已经有“公司在运转”的感觉。

判断实现是否正确的最简单标准：

> 用户不看日志，只看办公室，就能大致知道每个 AI 员工现在在做什么、谁在开会、谁收到消息、谁在开发，以及 HR 正在招聘还是面试。

如果这一点没有做到，则项目核心目标尚未完成。
