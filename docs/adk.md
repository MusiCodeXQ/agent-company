# AI Company × ADK-Rust 集成架构与开发路线

> 文档状态：设计稿，供后续实现使用
> 调研基线：2026-08-11，`zavora-ai/adk-rust` v2.0.0
> 适用项目：当前 AI Company（Rust + Axum + Tokio + SQLx + SQLite WAL，React + PixiJS）

## 1. 结论

`zavora-ai/adk-rust` 与本项目的融合度较高，但只适合接管“员工的大脑和工作工具”，不应接管“公司世界”。

建议采用以下边界：

- AI Company Rust Backend 是唯一公司状态与业务规则来源。
- ADK-Rust 运行员工的 LLM、思考循环、工具选择、工作流、Session 和任务内 checkpoint。
- 每位员工拥有一个宿主机持久目录；每次 Run 使用独立的一次性 Docker 容器执行 `employee-runtime`。
- Docker 负责文件、命令和进程隔离；AI Company Backend 负责容器编排、权限、审计和恢复。
- React + PixiJS 仍然只观察 Snapshot 和 WebSocket Domain Event。
- 员工位置、移动、工位、面对面会话、消息、会议、楼层、招聘和持久化不能迁入 ADK-Rust。

综合判断：

| 能力 | 融合度 | 结论 |
| --- | ---: | --- |
| LLM 与工具调用 | 高 | 用 ADK-Rust 替换或扩展当前 Runtime Adapter 很合适 |
| 编码、文件和 Shell | 高 | ADK-Rust 已有受工作区约束的开发工具，但必须再由 Docker 和 Company Policy 限权 |
| Session、Memory、Checkpoint | 高 | 可使用，但公司记录仍必须同步到 AI Company 数据库 |
| 多 Agent 工作流 | 中高 | 可用于任务内部协作；正式员工沟通必须经过 Company Message Bus |
| 会议讨论 | 中 | ADK 负责每位员工生成发言，Rust Meeting Orchestrator 控制到场、轮次和停止条件 |
| 卡死后的反思和重规划 | 中高 | ADK 图工作流和重试插件可提供能力，是否卡死及强制终止必须由 Rust Watchdog 判断 |
| 办公室空间模拟 | 不融合 | 继续完全由当前 Rust Spatial Simulation 管理 |
| 员工、楼层、招聘、解雇 | 不融合 | 继续由 AI Company Domain 管理 |
| Docker 生命周期和状态保存 | 不直接融合 | 需要项目自己的 `ContainerRuntime`；ADK 不是容器管理平台 |

这不是把 AI Company 改造成 ADK 的 UI，而是把 ADK-Rust 作为一种新的员工 Runtime 插入现有架构。

## 2. 我们要实现的产品模型

本项目的目标不是“多个聊天机器人加办公室皮肤”，而是持续运行的虚拟公司：

- 员工有稳定身份、部门、职位、技能、模型、工位、楼层和外观。
- 员工即使在浏览器关闭后仍继续工作、移动、开会和处理消息。
- 人物不能瞬移；空间变化必须经过 Rust 后端的寻路和持久化 Movement。
- 手机消息可以远程异步发送，不要求双方见面。
- 面对面谈话只有在双方真实到达同一交流点后才能开始。
- 会议只有在必要参会者进入会议室并坐到分配座位后才能开始。
- 管理员可以看到员工工作、工具、Token、进度、异常和交付物。
- 员工陷入重复尝试或无进展循环时，系统能暂停、诊断、重规划或转交，而不是无限消耗 Token。
- 编程或其他数字化岗位可以在受控工作区读写文件、运行命令和产出 Artifact。

因此总体流程仍然是：

```text
公司业务意图
  → AI Company Scheduler / Domain State Machine
  → 位置要求与权限检查
  → 必要时 MoveAgentCommand
  → 到达并满足业务前置条件
  → 创建 Agent Job
  → 调用该员工的 ADK Runtime
  → ADK 思考并调用受控工具
  → Runtime Event 归一化并持久化
  → Company Domain Event
  → Snapshot / WebSocket
  → PixiJS 表现
```

## 3. ADK-Rust 能提供什么

根据 ADK-Rust 官方仓库当前 v2.0.0 文档，可利用的能力包括：

- `adk-agent`：`LlmAgent`、顺序、并行、循环等 Agent。
- `adk-runner`：执行循环、Session 生命周期、流式 Event 和 Callback。
- `adk-model`：Gemini、OpenAI、Anthropic、DeepSeek、Ollama 等模型与 OpenAI-compatible preset。
- `adk-tool`：类型化 Rust Tool、Toolset 和 MCP。
- `adk-session`：内存或 SQLite Session 持久化。
- `adk-graph`：可 checkpoint、暂停、恢复和 human-in-the-loop 的图工作流。
- `adk-devtools`：限制在 Workspace 内的文件读取、写入、编辑、glob、grep 和 bash。
- `adk-sandbox` / `adk-code`：进程、WASM、Docker 等代码执行基础能力。
- `adk-plugin` / Callback：拦截模型和工具调用，适合采集运行事件与审计。
- `adk-retry-reflect`：工具失败后的反思、退避和 circuit breaker。
- `adk-telemetry`：结构化日志和 OpenTelemetry。

但要注意：

- ADK-Rust 是独立社区项目，不是 Google 官方 Rust SDK。
- v2.0.0 是有破坏性变化的大版本，依赖必须固定精确版本并经升级测试。
- `adk-devtools` 当前稳定性标为 Beta；首期只开放一小组经过测试的工具。
- ADK 官方也明确区分了 durable graph checkpoint 与自动进程恢复。项目仍需自己的 Worker、Job lease、heartbeat 和容器恢复机制。
- `ProcessBackend` 只有超时与环境隔离，并不天然提供内存和网络隔离；不能把它等同于 Docker 安全边界。

官方参考：

- [ADK-Rust 仓库与功能矩阵](https://github.com/zavora-ai/adk-rust)
- [Coding Agent 与 DevToolset](https://github.com/zavora-ai/adk-rust/blob/main/docs/official_docs/coding-agent/index.md)
- [Graph checkpoint 与 durable resume](https://github.com/zavora-ai/adk-rust/blob/main/docs/official_docs/agents/graph-agents.md)
- [各 crate 稳定性等级](https://github.com/zavora-ai/adk-rust/blob/main/STABILITY.md)
- [adk-sandbox API](https://docs.rs/adk-sandbox/latest/adk_sandbox/)

## 4. 必须保持 Rust Domain 独立的内容

下列数据和规则继续由当前 AI Company Backend 管理，ADK 只能通过 Company Tool 查询或提出请求，不能直接修改数据库。

### 4.1 公司与组织

- Company、Floor、Room、Office Layout、Navigation Grid。
- Employee 身份、职位、部门、性格、技能、外观 seed、在职状态。
- Workstation 分配、招聘、入职、调岗、解雇和工作交接。
- Model Profile、Runtime Profile、Employee Runtime Binding。

员工容器不是员工身份本身。删除或重建容器不能丢失员工身份；员工被解雇也不能只靠停止容器表示。

### 4.2 空间世界

- 当前 `location_id`、`room_id`、稳定坐标与 active movement。
- A* 寻路、门、障碍物、移动速度、预计到达时间。
- Location Reservation、会议座位、面试座位和交流点。
- Movement Worker、到达事件、坐下、起身和工作地点约束。

ADK 只能表达“我想去找 B”或“我想去会议室”这类高层意图，不能输出像素坐标后直接改位置，更不能用 LLM 决定逐步走路。

### 4.3 公司通信

- 手机 Direct Message 的保存、未读、投递 Job 和审计。
- 面对面会话是否成立以及发言记录。
- Meeting、Participant、Transcript、Decision、Action Item。
- 对话轮数、hop、Token、超时和频率限制。

禁止让 ADK 的 A2A、子 Agent 或 MCP 绕过 Company Message Bus 进行正式员工通信，否则管理员看不到，也无法关联任务和阻止无限互聊。

项目支持由 Company A2A Gateway 托管的标准 A2A HTTP+JSON 接口，但这不属于绕行路径。面对面 A2A 必须先创建持久化会话，由 Spatial Simulation 为双方预约同一房间内的不同座位并驱动移动；只有后端确认双方均已到场后，短期 capability token 才能用于 `message:send`。每次发送都会重新校验位置、发言顺序、轮数、Token 预算和过期时间，并把 A2A Message、Task、公司 Message、Agent Job 和审计记录关联写入 SQLite。任一员工离开会面座位时，会话立即关闭。

### 4.4 工作管理

- Goal → Task → Agent Job 的关系与状态。
- 调度、优先级、前置条件、预算、取消、失败和重试。
- 管理员介入、审批、帮助请求和任务转交。
- Token/费用汇总、Artifact 元数据、Tool Call 审计。

ADK Graph 可以保存一个任务内部的执行节点，但不能取代公司的 `agent_jobs`。公司 Job 是外部真实工作单元，ADK checkpoint 是该 Job 内部的执行状态。

## 5. 推荐部署架构：一个员工一个目录，每次 Run 一个临时容器

### 5.1 组件关系

```text
React + PixiJS
      │ Snapshot + WebSocket
      ▼
AI Company Server（Control Plane，宿主或管理容器）
  ├─ Company Domain / Scheduler / Spatial / Meeting
  ├─ SQLite WAL（公司唯一业务数据库）
  ├─ Agent Job Worker / Runtime Router
  ├─ Company Tool Gateway / Authorization / Audit
  ├─ Watchdog / Recovery Coordinator
  └─ ContainerRuntime（Docker Engine 适配器）
          │ 内部 HTTP/SSE 或 WebSocket，不引入消息中间件
          ├─ docker run --rm: backend-01 当前 Run
          ├─ docker run --rm: frontend-01 当前 Run
          └─ ...Run 结束即删除容器，不保存 container ID
```

每个临时容器中只运行一个 `employee-runtime` Rust 二进制：

- 加载自己的 employee/runtime profile。
- 建立一个 ADK Runner。
- 从 Backend 领取或接收该员工的 Job。
- 通过短期服务令牌调用 Company Tools。
- 把 model/tool/session/usage/heartbeat Event 上报 Backend。
- 只能访问自己的 Workspace 和 State Volume。

容器不直接连接公司 SQLite，不拥有其他员工 API Key，也不能访问 Docker socket。员工身份绑定的是宿主目录，不是容器 ID。

### 5.2 为什么不把 ADK 直接嵌进主 Server

嵌入同一 Rust 进程适合第一阶段 Spike，但不符合 Shell 隔离目标。正式方案在每次 Run 启动独立临时容器，容器内只执行 `employee-runtime`，原因是：

- 一个员工的 Shell、崩溃或内存泄漏不会拖垮公司 Server。
- 每个岗位可以拥有不同镜像、工具链、资源和网络权限。
- 可以单独暂停、重启、升级或解雇某个员工。
- Workspace 和环境状态边界清晰。

仍建议先做嵌入式 Adapter 验证 ADK API，再迁入容器，避免同时调试框架、协议和 Docker。

## 6. Runtime Adapter 设计

当前项目已经有 `runtime_profiles`、`employee_runtime_bindings`、`runtime_session_bindings`、`runtime_events` 和 OpenClaw 适配基础。不要另建一套平行概念，应扩展现有模型：

```rust
#[async_trait]
pub trait AgentRuntimeAdapter: Send + Sync {
    async fn health(&self, binding: &RuntimeBinding) -> Result<RuntimeHealth>;
    async fn start_run(&self, request: RuntimeRunRequest) -> Result<RuntimeRunHandle>;
    async fn cancel_run(&self, run_id: &str, reason: &str) -> Result<()>;
    async fn resume_run(&self, run_id: &str) -> Result<RuntimeRunHandle>;
    async fn shutdown_employee(&self, employee_id: &str) -> Result<()>;
}
```

实现：

- `MockRuntimeAdapter`：保留无 Key 可运行测试。
- `NativeLlmRuntimeAdapter`：兼容当前 OpenAI Compatible 路径。
- `OpenClawRuntimeAdapter`：保留现有功能。
- `AdkRustRuntimeAdapter`：新增，连接员工容器。

`runtime_type` 增加 `ADK_RUST`。业务代码只依赖 trait，不直接引用 ADK 类型。

不要一开始使用 ADK 自带 Server 取代 Axum。员工容器只需要窄协议：

```text
GET  /health
POST /v1/runs
POST /v1/runs/{id}/cancel
POST /v1/runs/{id}/resume
GET  /v1/runs/{id}/events       # SSE 或 WebSocket
```

或采用反向领取模式：

```text
POST /internal/runtime/jobs/lease
POST /internal/runtime/jobs/{id}/heartbeat
POST /internal/runtime/jobs/{id}/events
POST /internal/runtime/jobs/{id}/complete
```

V1 推荐反向领取模式。容器只需主动访问 Backend，不必为每个容器开放宿主端口，也更容易鉴权和重连。

当前首版已采用更窄的无端口协议：Backend 通过 `docker run --rm -i` 创建一次性员工容器，将 Run JSON 写入容器内的 `employee-runtime`，并读取结果 JSON。这保留了“密钥只按次下发”和“容器不开放端口”的边界；当后续需要超长 Run/中断/心跳时，再将同一 JSON 协议迁移到反向领取。

### 已落地的目录持久化规则

- `employee_runtime_workspaces` 保存员工的宿主根目录，不保存或复用 container ID。
- 员工构建阶段和执行阶段统一以 `debian:stable-slim` 为基础，不维护其他发行版变体。
- 每次 Agent Run 都使用 `docker run --rm` 启动临时容器；Run 结束后容器删除。
- 员工根目录下的 `workspace/state/artifacts/environment` 挂载到容器，备份和迁移只需处理该员工目录；其中 `environment` 在容器内作为员工的持久化 `$HOME=/home/employee`。
- 基础镜像不预装语言或构建工具链。Python、Node.js、Rust、JDK、Gradle、Maven 等由员工安装到个人 `$HOME`，所以升级和缓存都不依赖容器 writable layer。
- 新建员工目录时生成 `$HOME/.ai-company/RUNTIME_POLICY.md` 和 `toolchains.json`；每次 Run 还会重复注入关键规则，并要求通过 `company_record_personal_toolchain` 登记版本和路径。
- 安装命令同步写入 `runtime_environment_suggestions`，作为后续统一构建新基础镜像的建议清单。不使用 `docker commit`。

## 7. 员工如何控制文件和系统命令

答案是：可以，但不是“安装 ADK 后自动拥有整台电脑”。

ADK-Rust 的 Coding Agent / DevToolset 能提供受 Workspace 限制的：

- 读取、搜索、写入和编辑文件。
- glob、grep。
- bash 命令和测试命令。
- 计划、执行、验证的编码循环。

最终能操作哪些文件、命令和网络，取决于我们给容器挂载什么目录、开放什么 Tool、配置什么权限。

推荐目录：

```text
/workspace              # 宿主员工目录/workspace
/agent-state            # 宿主员工目录/state
/artifacts              # 宿主员工目录/artifacts
/home/employee          # 宿主员工目录/environment，员工持久化 HOME 与个人工具链
/tmp                    # tmpfs，容器销毁后丢弃
/runtime                # runtime 二进制和配置，只读
```

默认禁止：

- 挂载整个 Windows 盘、WSL Home、宿主 `/` 或用户主目录。
- 读取 `~/.ssh`、浏览器数据、Docker 配置和其他员工 Workspace。
- 在员工容器中挂载 `/var/run/docker.sock`。
- 容器使用 privileged、host PID、host network。
- 未审批安装系统包、修改防火墙或启动后台常驻服务。

如果需要操作用户本地项目，只挂载明确目录，例如：

```yaml
volumes:
  - /home/user/projects/project-a:/workspace
```

不要默认挂载 `/mnt/c`。Windows 文件夹可通过 WSL 路径定点挂载，但性能和文件权限需要单独测试。

## 8. 员工状态如何保存

容器不保存。应持久化的是员工目录和公司数据：

| 状态 | 保存位置 | 恢复方式 |
| --- | --- | --- |
| 员工、楼层、消息、任务、会议、位置 | AI Company SQLite | Server 启动恢复 |
| Agent Job 与 Runtime Event | AI Company SQLite | Worker 重新 lease 未完成 Job |
| ADK Session / Graph checkpoint | 员工目录 `state` | 新临时容器挂回同一目录后 resume |
| 项目文件、Git worktree、构建输出 | 员工目录 `workspace` | 下一个 Run 直接挂载 |
| Artifact | 员工目录 `artifacts` + 公司 DB 元数据 | 通过 artifact_id 找回 |
| Python/Node/Rust/JDK/Gradle/Maven 等个人工具链 | 员工目录 `environment`（容器内 `$HOME`） | 下一个 Run 从 `$HOME/.local/bin` 等路径复用 |
| 临时进程、内存、`/tmp` | 不保存 | Job 从 checkpoint 恢复 |
| 必需的系统库与基础命令 | 版本化基础镜像 + Environment Suggestion | 用同一 image digest 重建 |

建议原则：

1. 容器本身可丢弃，重要状态必须外置。
2. Workspace、Agent State、Artifact 和依赖环境使用每员工独立的宿主目录。
3. 基础镜像使用 digest 固定，不使用生产 `latest`。
4. 员工不直接执行需要 root 的 `apt install`；缺少的系统库或基础命令写入镜像建议清单，语言与构建工具链继续由员工在个人目录维护。
5. 经审批的环境变更生成 Dockerfile patch，由管理员统一构建版本化基础镜像。
6. 不使用 `docker commit`；临时容器的 writable layer 永不是公司持久化状态。

可增加表：

```text
employee_runtime_workspaces
  id, company_id, employee_id, host_root_path, workspace_path,
  state_path, artifacts_path, environment_path, status, created_at, updated_at

runtime_runs
  id, company_id, employee_id, agent_job_id, runtime_session_id,
  status, started_at, heartbeat_at, completed_at, stop_reason,
  input_tokens, output_tokens, cache_tokens, total_tokens

runtime_tool_calls
  id, run_id, employee_id, tool_name, arguments_redacted,
  status, approval_status, started_at, completed_at, result_summary

runtime_environment_suggestions
  id, company_id, employee_id, runtime_run_id, ecosystem,
  package_name, requested_spec, source_command, status, created_at, updated_at
```

`employee_runtime_workspaces` 才是员工 Runtime 的长期资源绑定。Docker 容器只是一次 Run 的可替换计算载体。

## 9. Company Tool Gateway

ADK 员工不能直接执行公司 SQL。为 ADK 编写一组类型化 Rust Tool；Tool 内部调用 Company Backend：

```text
company_get_self
company_get_task
company_report_progress
company_directory_search
company_locate_employee
company_send_message
company_request_face_to_face
company_request_meeting
company_reply_meeting
company_request_help
company_publish_artifact
company_request_activity
```

每个请求由 Backend 根据容器服务令牌反查 `runtime_agent_id → employee_id`。不要信任模型传入的 `from_employee_id`。

普通员工、Manager、HR、Supervisor 使用不同 Tool Policy；即使 ADK 做过 Tool 限制，Backend 仍要二次授权和审计。

## 10. 面对面交流：知道位置，但不追逐像素

### 10.1 两种通信必须区分

- `PHONE_MESSAGE`：异步消息，不需要移动，持久化后创建接收方 Job。
- `FACE_TO_FACE`：空间会话，必须满足到场条件后才能生成双方正式发言。

“只有面对面才能开始交流”应解释为：只有面对面通道需要共处；手机仍可发消息，但手机消息不能伪装成办公室当面谈话。

### 10.2 查询移动中员工的当前位置

当前 Movement 已有 `path`、`started_at`、`estimated_arrival_at`。Backend 应新增纯函数：

```rust
fn position_at(
    path: &[MovementPoint],
    started_at: DateTime<Utc>,
    estimated_arrival_at: DateTime<Utc>,
    server_time: DateTime<Utc>,
) -> MovementPoint;
```

该函数按路径累计长度插值，和前端使用同一算法。无需每帧写 SQLite，也无需 60 FPS 广播坐标。

`company_locate_employee(B)` 返回：

```json
{
  "employeeId": "backend-01",
  "floorId": "floor-01",
  "roomId": "open-office",
  "locationId": "backend_desk_01",
  "currentPosition": { "x": 420, "y": 318 },
  "movement": "WALKING",
  "destinationLocationId": "coffee_point_01",
  "workstationLocationId": "backend_desk_01",
  "availability": "MOVING",
  "canRequestFaceToFace": true
}
```

这里的 `currentPosition` 是 Backend 根据服务器时间推导的真实理论位置，不是前端回传的位置。

### 10.3 A 去找 B 的正确流程

不要让 A 每秒查询 B 的坐标并追着跑，这会造成路径抖动、追逐不收敛和大量事件。使用“稳定会合点”：

1. A 调用 `company_request_face_to_face(B, topic)`。
2. Backend 检查双方在职、Presence、Floor、Meeting、Interview、Job 优先级。
3. 如果 B 在开会、面试、离线或不可中断工作，返回 `DEFERRED` 或建议手机消息；A 不得去会议室门口骚扰。
4. 如果 B 在自己的工位且静止，预定 B 工位附近的 `social_point`，让 A 前往。
5. 如果 B 正在移动，Backend 用 `position_at` 知道其当前位置，但优先选择 B 的已知目的地作为会合点；也可以先询问 B 是否停下或约在其工位。
6. 如果 B 没有稳定目的地，创建邀请，待 B 接受后为双方预定统一 rendezvous location。
7. 所有移动仍走 `MoveAgentCommand → A* → Persistent Movement`。
8. 双方满足“同楼层、同房间、距离阈值内、均已停止、Reservation 有效”后，Backend 创建 `face_to_face_session`。
9. 只有 Session 进入 `IN_PROGRESS` 后，才给双方 ADK Runtime 投递面对面发言 Job。
10. 任一方离开、进入会议或超时，Backend 暂停或结束会话。

建议表：

```text
face_to_face_sessions
  id, company_id, initiator_id, target_id, topic,
  rendezvous_location_id, status,
  created_at, accepted_at, started_at, ended_at, end_reason

face_to_face_participants
  session_id, employee_id, status, arrived_at, last_turn_at
```

状态：

```text
REQUESTED → ACCEPTED → MOVING → WAITING → IN_PROGRESS → FINISHED
          ↘ DECLINED / DEFERRED / CANCELLED / EXPIRED
```

如果业务需要“去 B 工位找人”，应在每个工位旁定义可通行的 `workstation_social_point`。A 不应走到 B 的座椅像素上，也不能占用 B 的 workstation location。

## 11. 多 AI 会议讨论

会议继续由现有 Rust Meeting + Movement 系统主导：

```text
Meeting CREATED
  → 分配并预定座位
  → 每人 MoveAgentCommand
  → 先到者 WAITING
  → 最后一位必要参会者到达
  → Meeting IN_PROGRESS
  → Meeting Orchestrator 投递发言 Job
  → Transcript / Decision / Action Item 持久化
  → FINISHED
  → 每人按 Scheduler 决定下一目的地
```

ADK-Rust 的职责只是：给定员工身份、议题、可见历史和当前问题，生成该员工这一轮发言或 `NO_REPLY`。

Meeting Orchestrator 必须限制：

- 主持人和发言顺序。
- 每轮最多发言人数和最大轮数。
- 单人及整场 Token 预算。
- 重复观点检测和无新信息停止。
- 工具调用权限；会议发言默认不应随意执行 Shell。
- 用户插话后的重新提问范围。
- 超时、缺席和 Agent Runtime 故障处理。

不要让所有容器通过 ADK A2A 自由互聊。否则无法保证“到场后才讨论”，也无法可靠停止会议。

可以为每位员工建立独立 Session Context：

```text
DIRECT_MESSAGE:{conversation_id}
FACE_TO_FACE:{session_id}
MEETING:{meeting_id}
TASK:{task_id}
```

继续复用当前 `runtime_session_bindings`，避免把所有上下文塞入一个超长 Session。

## 12. 管理员查看所有人工作情况

ADK Callback、Tool Event 和 Runner Event 统一经过 `RuntimeEventNormalizer`，转成公司事件：

```text
ADK run started              → activity=THINKING
read/search file             → activity=READING, device=COMPUTER
write/edit file              → activity=CODING, device=COMPUTER
bash test/lint/build          → activity=TESTING, device=COMPUTER
company_send_message         → device=PHONE
waiting for approval/message → activity=WAITING
run completed                → 下一业务状态或 IDLE
runtime/tool error           → activity=ERROR
```

管理员页面应读取公司数据库中的归一化信息，而不是直连员工容器：

- 当前 Goal、Task、Job、步骤和最后进展时间。
- 当前活动、地点、移动目的地和 Runtime 健康。
- 最近模型调用、工具调用、命令摘要和错误。
- Workspace Git status/diff 摘要及 Artifact。
- input/output/cache/total Token 和费用。
- 已运行时长、无进展时长、重试次数和卡死风险。
- 暂停、取消、请求说明、转交、重启 Runtime 等管理动作。

敏感参数、环境变量、API Key 和完整命令输出要脱敏。管理员可审计不等于任何 UI 都返回 Secret。

## 13. 防止员工陷入死循环

不能只靠员工自己察觉“我卡住了”。新增独立的 `RuntimeWatchdog`，使用确定性指标判定：

- Job 超过最大 wall-clock 时间。
- 长时间没有 progress heartbeat、Artifact、文件 diff 或状态变化。
- 相同 Tool + 相同参数连续调用多次。
- 相同错误签名重复出现。
- 计划停留在同一步超过阈值。
- 测试结果不变却重复执行。
- 单 Job Token、Tool Call、重试次数或费用超预算。
- A→B→A 消息 hop 或会议发言超过限制。

建议状态机：

```text
RUNNING
  → SUSPECTED_STALL
  → PAUSING
  → DIAGNOSING
  → REPLANNING
  → RUNNING
      ↘ WAITING_FOR_HUMAN / ESCALATED / FAILED / CANCELLED
```

处理顺序：

1. 停止继续领取新 Tool Call。
2. 向 ADK Runner 发 cooperative cancel。
3. 保存 Session checkpoint、工作区 diff、命令摘要、最后错误和计划。
4. 由 Supervisor Agent 阅读精简证据，输出“原因、已尝试方案、禁止重复项、新计划、成功条件”。
5. Backend 验证新计划和预算后恢复原员工，或把任务转交其他员工。
6. cooperative cancel 超时才对容器发送 SIGTERM，再超时才 SIGKILL。
7. 下一次运行创建新的临时容器，挂回原员工目录，并从安全 checkpoint 继续。

ADK 的 LoopAgent、Graph checkpoint、Retry/Reflect 和 circuit breaker 可以参与第 3～5 步，但阈值、终止权和预算必须由 Company Backend 掌握。

## 14. Docker 安全与资源策略

每次员工 Run 的临时容器至少配置：

- 非 root UID/GID。
- `read_only: true` 根文件系统，Workspace/State/Artifact 单独可写。
- `no-new-privileges`，删除不需要的 Linux capabilities。
- CPU、内存、PIDs、磁盘配额和命令超时。
- `/tmp` 使用有大小限制的 tmpfs。
- 默认拒绝出站网络，按 Runtime Profile 开放模型 API、包仓库或业务站点。
- Secret 通过短期注入或代理，不写入镜像、员工目录、Prompt、日志。
- Tool allowlist + 参数校验 + 危险操作审批。
- Docker Engine 仅由 Control Plane 的受限适配器访问。

需要执行代码时使用一次性员工 Run 容器作为隔离边界。不要在容器里再启动 Docker；确需 ADK Docker backend 时，应由专门执行服务代理，而不是把 Docker socket 交给员工。

## 15. 与当前数据模型的最小改动

优先扩展已有表和模块：

- `employee_runtime_bindings.runtime_type` 增加 `ADK_RUST`。
- `runtime_profiles` 保存 ADK feature/tool/memory/checkpoint policy。
- `runtime_session_bindings` 保存 ADK session/thread key。
- `runtime_events` 保存原始事件摘要；大体积 stdout/stderr 放 Artifact 存储。
- `llm_calls` 继续作为 Token 统计来源，ADK usage 归一化写入该表。
- `agent_jobs` 增加 runtime run id、lease owner、heartbeat、attempt、checkpoint key 等字段。
- 新增 `employee_runtime_workspaces`、`runtime_runs`、`runtime_tool_calls`、`runtime_environment_suggestions`；旧版 `runtime_containers` 和容器 ID 字段只作为迁移兼容数据保留，不再写入或暴露。
- 新增 `face_to_face_sessions` 和 participant 表。
- Movement 与 Meeting 表继续使用现有实现，不重复创建。

SQLite 继续启用 WAL。表使用普通主键、外键和 JSON Text，避免依赖 SQLite 专属业务逻辑，为未来 PostgreSQL 留出迁移空间。

## 16. 推荐代码结构

```text
apps/server/src/
  runtime/
    mod.rs
    adapter.rs
    router.rs
    mock.rs
    native_llm.rs
    openclaw.rs
    adk_rust.rs
    event_normalizer.rs
  containers/
    mod.rs
    docker.rs
    reconciler.rs
    policy.rs
  watchdog/
    mod.rs
    detector.rs
    recovery.rs
  company_tools/
    auth.rs
    directory.rs
    messaging.rs
    spatial.rs
    meeting.rs
    task.rs
    artifact.rs
  face_to_face.rs

apps/employee-runtime/
  Cargo.toml
  src/
    main.rs
    config.rs
    agent.rs
    runner.rs
    company_toolset.rs
    event_reporter.rs
    checkpoint.rs
```

不要在 `apps/server` 中散落 ADK 具体类型；只有 `adk_rust.rs` 和独立 `employee-runtime` 应直接依赖相关 crate。

## 17. 分阶段实施路线

### Phase 0：兼容性 Spike

- 固定 ADK-Rust 精确版本，不使用通配版本。
- 用当前 Model Profile 创建一个 ADK `LlmAgent`。
- 实现 Mock Provider 和一个 OpenAI Compatible Provider 验证。
- 验证 usage、tool event、cancel、SQLite session/checkpoint。
- 记录编译体积、冷启动、常驻内存和当前 Rust toolchain 兼容性。

完成标准：不改现有业务流程，通过独立测试证明一次消息 Job 可由 ADK 返回结果。

### Phase 1：统一 Runtime Adapter

- 抽取 `AgentRuntimeAdapter`。
- 当前 Mock、Native LLM、OpenClaw 都迁到 Adapter 后面。
- 增加 `ADK_RUST`，先同进程运行。
- 实现 `RuntimeEventNormalizer` 和 Token 写入。

完成标准：同一员工可切换 Runtime，前端和 Company Domain 无需修改。

### Phase 2：每员工一个宿主目录 + 临时 Docker Run

- 新建 `apps/employee-runtime`。
- 实现无 container ID 持久化的 `docker run --rm` Runtime。
- 每员工分配 Workspace/State/Artifact/Environment 宿主目录、服务身份和资源限制。
- 实现 Job lease、heartbeat、cancel、complete；容器失败时使用同一员工目录重试。

完成标准：每次 Run 都可以使用全新容器，员工身份、任务、Session、Workspace 和员工安装的依赖不丢失。

### Phase 3：Company Tool Gateway

- 实现目录、消息、任务、进度、Artifact、求助 Tool。
- Backend 做身份反查、RBAC、审计和参数验证。
- 禁用正式员工通信的直接 A2A 绕行路径。

完成标准：A 可以给 B 发持久化手机消息；关闭网页不影响投递和回复。

### Phase 4：面对面会话

- 实现 Rust `position_at` 并在 Snapshot/查询 API 复用。
- 为工位、公共区域增加 social point。
- 实现 reservation、邀请和 face-to-face 状态机。
- 把“去找 B”改为稳定会合点流程。

完成标准：B 静止、B 移动、B 在会议中三个场景都有确定行为；只有双方到场才开始对话。

### Phase 5：多 AI 会议

- 复用现有 Meeting Movement。
- 实现轮次、主持人、`NO_REPLY`、预算和停止条件。
- 每位参会者使用独立 Meeting Session。
- 生成 Transcript、Decision 和 Action Item。

完成标准：未到场不能发言；最后一人到达后开始；用户插话可触发有疑问者回复；会议可可靠结束。

### Phase 6：文件、Shell 与环境持久化

- 接入 `adk-devtools` 的最小 Toolset。
- 建立 Workspace、Git worktree、Artifact 和审批流程。
- 固定基础镜像，加入 Environment Manifest。
- 验证 WSL/Windows 定点目录挂载、权限和性能。

完成标准：员工能在自己目录修改代码、运行测试；重建容器后文件和批准的环境仍可恢复；不能读取其他员工目录。

### Phase 7：Watchdog 和管理员救援

- 收集 progress/tool/error/token 指标。
- 实现 stall detector、cooperative cancel、checkpoint 和 Supervisor replan。
- 管理员增加暂停、恢复、澄清、转交和重启操作。

完成标准：构造重复失败任务后，系统在预算内停止循环，保存现场，并能按新计划恢复或升级给管理员。

### Phase 8：灰度替换

- 先给一名测试员工启用 ADK-Rust。
- 再启用 Backend、Frontend、QA 三个不同工具策略。
- 保留 Mock 和当前 Runtime 回退开关。
- 稳定后再接 HR、内容、运营等非开发岗位。

## 18. 最终验收场景

### 场景 A：移动中的员工会合

1. B 从工位走向茶水间。
2. A 请求当面找 B。
3. Backend 能按服务器时间算出 B 当前理论坐标，也知道 B 的目的地和工位。
4. 系统选择稳定会合点，不让 A 持续追逐 B 的每帧坐标。
5. 双方到达前没有面对面发言 Job。
6. 双方到达后开始谈话，记录对管理员可见。

### 场景 B：会议中的员工不可打扰

1. B 正在会议室开会。
2. A 请求当面找 B。
3. Backend 返回 `DEFERRED_MEETING`，不移动 A，不把 B 拉出会议。
4. A 可选择发送手机消息或预约会后会合。

### 场景 C：多 AI 会议

1. Backend、Frontend、QA 从不同工位走向独立座位。
2. 最后一人到达前会议不开始。
3. Meeting Orchestrator 逐人调用各自 ADK Runtime。
4. 发言、疑问、用户插话和结论全部持久化。
5. 达到停止条件后散会，员工按真实路径离开。

### 场景 D：文件和容器恢复

1. Backend 员工在 `/workspace` 修改文件并运行测试。
2. 强制停止当前临时 Run 容器。
3. 下一次 Run 用固定镜像创建全新容器，并挂回原员工目录。
4. Git diff、Artifact、ADK Session、Job checkpoint 和 Token 统计仍存在。
5. 员工从安全 checkpoint 继续，不重新做已完成步骤。

### 场景 E：死循环救援

1. 员工连续运行相同失败命令或重复相同 Tool Call。
2. Watchdog 标记 `SUSPECTED_STALL` 并暂停。
3. 系统保存现场，Supervisor 给出不同的新计划和成功条件。
4. 管理员可以查看原因并允许恢复、转交或终止。
5. 整个过程受时间、Token、Tool Call 和重试预算限制。

## 19. 明确不做的事情

- 不让 ADK 接管 Employee/Floor/Meeting/Movement 数据库。
- 不让 LLM 决定路径、坐标、走路、坐下或每帧动画。
- 不用 ADK A2A 代替 Company Message Bus。
- 不让员工容器直接读写公司 SQLite。
- 不把 Docker socket 或整个宿主文件系统交给员工。
- 不依赖 `docker commit` 保存日常状态。
- 不在本阶段引入 Redis、Kafka、RabbitMQ 或 Kubernetes。
- 不一次性把所有员工迁移到新 Runtime。

## 20. 最终设计原则

```text
AI Company Backend = 公司世界、规则、权力、审计和真实状态
ADK-Rust            = 员工大脑、任务内工作流、工具选择和 Session
Docker              = 每位员工的隔离工作环境和可恢复执行边界
PixiJS              = 对真实公司状态的视觉观察
```

只要坚持这个边界，ADK-Rust 可以与当前系统深度融合，又不会破坏已经建立的楼层、员工、消息、持久化、移动和会议逻辑。未来即使把某位员工换成 OpenClaw、Codex、Claude Code、ACP Agent 或其他 Runtime，公司世界也不需要重写。
