# ADK-Rust 项目优化实施指导

> 适用项目：Virtual AI Company  
> 编写日期：2026-08-12  
> 目标版本：以 `apps/employee-runtime/Cargo.toml` 中锁定的 ADK-Rust `=2.0.0` 为准

## 1. 结论先行

项目已经接入 ADK-Rust，但当前主要使用的是基础执行能力：`LlmAgent`、`Runner`、SQLite Session、`FunctionTool`、OpenAI 兼容模型和 `DevToolset`。下一步不应简单增加更多工具，而应优先补齐“可观察、可审批、可恢复、可评测、可治理”的运行闭环。

建议按以下顺序推进：

1. P0：先让员工之间的 A2A 对话、任务、轮次、到场证明和审计记录在界面可见。
2. P0：把 employee-runtime 的 ADK 事件从“结束后一次性返回”改为“运行中持续流式上报”。
3. P0：将 ADK 2.0 的异步工具确认机制接入现有公司审批体系。
4. P0：用 ADK Plugin 统一做审计、预算、脱敏、错误和事件转换。
5. P0/P1：用 ADK Graph + SQLite Checkpointer 实现可恢复的编码工作流。
6. P1：引入 Skill、Eval、Guardrail、Memory、Artifact 和 OpenTelemetry。
7. P1：复用 `adk-server` 的 A2A 类型、客户端和一致性能力，但保留公司后端作为唯一治理网关。
8. P2：按需试点 RAG、浏览器 QA、Wasm 沙箱；暂缓 Realtime、Computer Use 和外部向量数据库。

核心边界必须保持不变：

- 公司后端是员工、任务、消息、A2A 会话、审批、预算和审计的权威数据源。
- ADK 是单个员工容器内的执行引擎，不得绕过公司后端直接联系其他员工。
- “动画角色面对面”是公司业务层的通信许可条件，不是 A2A 协议本身提供的能力。
- PixiJS 只表现后端领域事件，不能在前端伪造员工已到场或 A2A 会话已建立。
- 继续支持 Mock LLM，无 API Key 时所有关键流程都必须可演示、可测试。

## 2. 当前对话界面的实际情况

### 2.1 已经能查看的内容

1. 点击员工打开手机窗口后，可以在“消息”区域查看 CEO/用户与该员工之间的历史消息。
2. 打开会议桌消息窗口后，可以查看带当前 `meetingId` 的会议广播和员工回复。
3. 手机消息支持加载历史记录；会议消息按会议上下文过滤。

相关实现：

- `apps/web/src/App.tsx` 中的 `PhoneModal`。
- `apps/web/src/App.tsx` 中的 `MeetingModal`。
- `apps/server/src/db.rs` 中的员工消息历史查询。

### 2.2 当前看不到的内容

员工与员工之间的 A2A 消息虽然会落入 `messages`、`a2a_messages`、`a2a_tasks` 和 `a2a_audit_log`，但现有手机窗口只过滤“用户与该员工”的消息，因此不会显示 `employee -> employee` 消息。

在本次 P0 实施前还缺少以下界面；现已补齐前五项（A2A 制品仍沿用任务详情中的标准字段展示，后续可做独立制品面板）：

1. A2A 会话列表。
2. 单个面对面会话的完整消息时间线。
3. `contextId`、`taskId`、`messageId` 的关联关系。
4. 邀请、移动、双方到场、会话激活、超时和结束的状态变化。
5. 每轮 Token、累计 Token、轮次上限和终止原因。
6. A2A 制品及其版本。
7. 拒绝、超时、取消、越权、超预算等审计事件。

因此，对问题“界面是否有查看员工对话沟通内容的地方”的准确回答是：**现在同时有 CEO 与员工、会议上下文入口，以及员工之间 A2A 的专用“消息 / 员工沟通”页面。**

> P0 实施状态（2026-08-12）：员工之间的 A2A 专用“消息 / 员工沟通”页已加入右侧主导航。页面从服务端只读接口加载会话列表和详情，可查看双方消息、任务、轮次、Token、到场激活时间、结束原因与审计记录；WebSocket 收到 A2A 领域事件后会刷新当前会话。ADK 高风险工具审批也在该页展示，审批动作不会绕过动画到场约束。

## 3. ADK 能力采用矩阵

| 能力 | 当前情况 | 建议 | 优先级 |
|---|---|---|---|
| Agent / Runner / Session | 已使用 | 保留并模块化 | 已有 |
| FunctionTool / DevToolset | 已使用 | 增加权限、审批和策略包装 | P0 |
| Plugin 生命周期 | 已启用 employee-runtime 治理插件 | 继续拆分为独立治理插件 | P0（已完成首版） |
| Tool Confirmation | 已接公司审批 API、SQLite 和 UI | 增加岗位级风险策略 | P0（已完成首版） |
| Event Streaming | 已使用 NDJSON 运行中上报 | 增加 artifact / trace 事件 | P0（已完成首版） |
| Graph / Checkpoint / Interrupt | 未使用 | 编码任务可恢复、可人工介入 | P0/P1 |
| A2A Server / Client | 公司已有自研治理 A2A | 复用标准模型和一致性测试，不替换治理网关 | P1 |
| Skill | 未使用 | 按岗位、任务和项目注入最小技能与工具 | P1 |
| Eval | 未使用 | 建回归集和 CI 质量门禁 | P1 |
| Guardrail | 主要靠业务代码 | 增加输入、输出、工具参数和成本防线 | P1 |
| Memory | 公司有文本记忆，ADK 仅 Session | 增加项目隔离的语义检索层 | P1 |
| Artifact | 公司有制品路径 | 引入版本化制品服务并映射公司元数据 | P1 |
| Telemetry | 有 tracing 和业务事件 | 增加跨进程 trace/span 与 GenAI 指标 | P1 |
| Auth / ScopeGuard | 未使用 | 工具级最小权限和审计 | P1 |
| Toolset / MCP | DevToolset 为主 | 按任务动态组合，严格工具白名单 | P1 |
| Retry-Reflect | 现有简单重试/熔断 | 只对可恢复错误做有界反思重试 | P1 |
| RAG | 未使用 | 代码、规范和项目文档检索试点 | P2 |
| Browser | 未使用 | 独立隔离的前端验收员工能力 | P2 |
| Sandbox | 外层已有 Docker | Docker 保留；Wasm 只跑小型不可信代码 | P2 |
| Realtime / Computer Use | 未使用 | V1 暂缓，Computer Use 仍为 Beta | 暂缓 |

## 4. 逐条优化指导

### 第 1 条：增加“员工沟通 / A2A 会话”查看器

**目的**

让管理者能够查看员工之间实际发生的面对面通信，验证“到场才通信”的业务约束，并对消息、任务、预算和审计进行追溯。

**实施步骤**

1. 在右侧管理区增加“沟通”入口，不要把员工 A2A 消息混进 CEO 手机聊天。
2. 会话列表按 `createdAt DESC` 展示，至少显示双方员工、地点、状态、当前轮次、Token 使用量、关联任务和开始/结束时间。
3. 点击会话后展示一条统一时间线：邀请、移动、到场、会话激活、A2A 请求、响应、制品、拒绝、超时、结束。
4. 消息气泡显示发送者、接收者、`messageId`、`contextId`、`taskId`、轮次、Token 和时间。
5. 提供筛选：员工、状态、地点、任务、时间范围、异常原因。
6. 默认隐藏模型内部推理，只展示用户可审计的消息、工具摘要、结果和策略决定。
7. 新增只读服务端接口，从 `a2a_face_to_face_sessions`、`a2a_messages`、`a2a_tasks`、`a2a_audit_log` 联合读取。
8. WebSocket 增加可展示的领域事件，不要让前端轮询数据库，也不要在 canvas 中推断会话状态。

**建议事件**

- `a2a.session.invited`
- `a2a.session.participant_arrived`
- `a2a.session.activated`
- `a2a.message.sent`
- `a2a.message.received`
- `a2a.artifact.updated`
- `a2a.session.ended`
- `a2a.policy.denied`

事件应只包含展示和定位所需字段；完整正文继续从授权 API 读取，避免 WebSocket 广播泄露跨公司内容。

**验收标准**

1. 双方未到指定地点时，界面能看到拒绝原因，且数据库中不存在成功发送记录。
2. 双方到场后，消息在不刷新页面的情况下出现。
3. 刷新浏览器后，完整会话、任务、轮次和审计仍可恢复。
4. 所有查询强制带 `company_id`，不同公司不能互相读取。
5. Mock 模式能够完整演示邀请、到场、两轮交流和结束。

### 第 2 条：将 employee-runtime 改为流式事件协议

**当前问题（P0 已解决）**

ADK `Runner` 会产生事件，但 `apps/employee-runtime/src/main.rs` 当前在一次运行结束后才把汇总响应返回公司后端。长任务期间，办公室只能看到员工忙碌，不能解释员工正在调用什么工具、是否等待批准、已完成哪个步骤。

**实施步骤与落地结果**

1. 将 stdout 定义为机器协议专用通道，日志只能写 stderr。
2. 使用 NDJSON：每行一个带 `schemaVersion` 的运行事件。
3. 至少支持 `run.started`、`model.started`、`tool.requested`、`tool.started`、`tool.completed`、`approval.required`、`usage.updated`、`artifact.created`、`run.completed`、`run.failed`。
4. 每个事件携带 `companyId`、`employeeId`、`agentJobId`、`runtimeRunId`、`runtimeSessionId`、`sequence` 和时间戳。
5. `apps/server/src/adk_runtime.rs` 逐行解析并立即写入 `runtime_events`，再转换成公司领域事件推送 WebSocket。
6. 用单调递增的 `sequence` 和唯一 `eventId` 去重，容器退出后允许安全重放尾部事件。
7. 最终结果仍保留 `run.completed`，避免把流式展示和权威完成状态混为一谈。
8. 解析到未知事件类型时持久化原始 JSON 并忽略展示，保证前后版本兼容。

已落地：employee-runtime stdout 采用带 `kind` 的 NDJSON（事件字段含 `schemaVersion`、`eventId`、`sequence`、运行/员工/Job/Session 标识）；Docker 引擎逐行转发，服务端边收边写入 `runtime_events` 并广播 `runtime.event`。保留最终 `result` 行和旧版单 JSON 解析，以支持滚动升级。

**验收标准**

1. 工具调用开始后 1 秒内，界面出现相应工作状态。
2. 进程中途退出时，已接收事件仍存在，运行标记为失败而不是假完成。
3. 同一事件重放两次不会产生两条工具调用或重复计费。
4. Mock runtime 和真实模型使用相同事件封装。

### 第 3 条：用 ADK 2.0 Tool Confirmation 接入公司审批

**目的**

把高风险工具从“Agent 可以直接执行”改成“Agent 提交意图，公司策略判定，必要时等待授权，批准后恢复原调用”。ADK 2.0 的工具确认处理器是异步的，并以 function-call ID 关联确认结果，适合与现有 `runtime_tool_calls.approval_status` 对接。

**实施步骤**

1. 给所有工具定义风险等级：只读、工作区写入、网络、依赖安装、外部消息、凭据、删除/覆盖。
2. 默认策略：只读工具自动放行；工作区写入按岗位策略；网络、安装、外部副作用和破坏性操作必须审批。
3. ADK 请求确认时，先写 `runtime_tool_calls`，保留 function-call ID、工具名、参数摘要、风险、策略版本和到期时间。
4. 公司后端生成审批记录，并通过 WebSocket 通知 CEO/管理员。
5. 未批准时不得执行工具，也不得让模型通过另一个同义工具绕过策略。
6. 批准、拒绝或超时后，使用原 function-call ID 恢复或终止调用。
7. 参数发生变化时必须重新审批；不能沿用旧批准。
8. A2A 发送工具永远不能由容器直接获得，员工通信只允许调用公司网关提供的受控工具。

**验收标准**

1. 未批准的网络/安装调用没有系统副作用。
2. 批准后只执行被批准的精确工具和参数。
3. 容器重启后待审批状态可恢复或明确过期。
4. 审计能回答谁、何时、为何批准了哪次调用。

### 第 4 条：用 ADK Plugin 建立统一治理管线

**为什么使用 Plugin**

ADK Plugin 可以在整个运行、用户消息、事件、Agent、模型调用、工具调用前后挂载逻辑。相比把治理散落在 `main.rs` 的事件循环里，它更适合统一预算、审计、脱敏和错误处理。

**建议插件顺序**

1. `ContextIdentityPlugin`：注入公司、员工、任务、运行和会话标识。
2. `PolicyPlugin`：执行岗位、工具、网络、A2A 和项目范围策略。
3. `BudgetPlugin`：检查单轮、单任务、每日 Token/费用和工具次数。
4. `RedactionPlugin`：模型调用前删除不应出容器的秘密；事件落库前脱敏。
5. `AuditPlugin`：生成结构化运行、模型和工具审计事件。
6. `TelemetryPlugin`：生成 span、延迟、Token 和错误指标。

**实施要求**

1. 插件只能执行通用治理，任务业务状态仍由公司后端决定。
2. deny 优先于 allow；插件顺序固定并有测试。
3. 审计记录参数哈希和安全摘要，不默认保存环境变量、完整源码或模型内部推理。
4. 插件失败采用“安全失败”：审批、权限和预算插件异常时拒绝高风险操作。
5. 将 `apps/employee-runtime/src/main.rs` 拆为 `agent.rs`、`plugins.rs`、`tools.rs`、`protocol.rs`、`policy.rs`，避免继续膨胀。

**验收标准**

1. 每次模型和工具调用都能关联到同一 `runtimeRunId`。
2. 达到预算时在下一次模型/工具调用前终止，而不是超额后才记录。
3. 任何插件拒绝都产生明确、可查询的策略事件。

### 第 5 条：用 ADK Graph 构建可恢复的编码工作流

**适用范围**

Graph 用于一个员工内部的复杂任务编排，不用于替代公司的员工调度，也不用于绕过面对面 A2A。

**建议首个工作流**

`分析任务 -> 制定计划 -> 修改代码 -> 格式化/检查 -> 测试 -> 总结制品`

失败分支：

`检查或测试失败 -> 诊断 -> 有界修复 -> 再检查`

人工中断点：

- 修改关键配置前。
- 安装新依赖前。
- 执行网络或破坏性工具前。
- 超过重试或预算阈值后。

**实施步骤**

1. 为图状态定义显式 schema：任务输入、计划、已改文件、检查结果、制品、预算、尝试次数。
2. 使用 reducer 合并追加型事件，不要用字符串拼接模拟状态。
3. 使用 SQLite Checkpointer 每个节点后保存状态。
4. 将 ADK checkpoint ID 映射到现有 `runtime_checkpoints`，不要再建重复表。
5. 节点必须幂等；恢复时跳过已完成节点，外部副作用必须带幂等键。
6. 设定图总超时、节点超时、最大循环数和最大模型调用数。
7. 将图节点进度转换为领域事件，驱动员工“阅读、编码、测试、等待审批”等可视状态。
8. 先在一个编码员工和 Mock 模式试点，再扩大到其他岗位。

**验收标准**

1. 在测试节点后杀掉容器，重启后从检查点继续，不重复改代码或重复计费。
2. 连续失败达到上限后进入 `NEEDS_ATTENTION`，不无限循环。
3. 图状态、公司任务状态和 UI 活动状态之间有明确映射测试。

### 第 6 条：让 ADK A2A 能力服务于标准兼容，而非替换治理网关

**原则**

`adk-server` 提供 A2A 服务端、客户端、远程 Agent 和请求上下文能力。项目可以利用这些能力减少协议层自研，但“双方动画角色到场后才能通信”的规则必须留在公司后端。

**实施步骤**

1. 对照 A2A 标准字段审计现有 `a2a.rs`：task、message、part、artifact、context、状态和错误形状。
2. 优先复用 ADK/A2A 的数据类型、客户端、内容类型校验和一致性测试。
3. 保留公司后端入口，先校验公司、员工在职状态、会话状态、地点、距离、到场时间、轮次、Token 和幂等键。
4. 校验通过后，才把请求转换为 ADK Agent 输入。
5. Agent 的响应必须回到公司后端持久化，再由后端转发；不能容器到容器直连。
6. 每条消息使用稳定 `messageId` 去重，每个会话使用 `contextId`，有工作目标时使用 `taskId`。
7. 支持 `INPUT_REQUIRED` 时，将其映射为公司任务“等待输入”，并驱动 UI 等待状态。
8. 如果启用 A2A SSE/推送，仍需使用公司身份、租户隔离和签名验证。

**验收标准**

1. 同一 `messageId` 重发不增加轮次、Token 或消息行。
2. 任一方离场后，新消息立即被公司网关拒绝。
3. 外部标准客户端无法绕过公司到场策略直接调用员工。
4. 协议一致性测试与现有面对面治理测试同时通过。

### 第 7 条：用 ADK Skill 管理岗位能力和上下文

**目的**

把当前大段通用 system prompt 拆成可版本化的岗位技能、任务技能和项目规范，并只给 Agent 注入本任务需要的工具。

**建议目录**

```text
/agent-state/skills/
  common/company-policy.skill.md
  roles/frontend-engineer.skill.md
  roles/rust-backend-engineer.skill.md
  workflows/code-review.skill.md
  workflows/incident-triage.skill.md
/workspace/AGENTS.md
```

**实施步骤**

1. 公司公共规则由平台提供，只读挂载；项目 `AGENTS.md` 从工作区读取。
2. 每个 Skill 声明 `name`、`description`、版本、标签、references 和 `allowed-tools`。
3. 使用严格 `ToolRegistry` 校验，不存在的工具应拒绝 Skill，而不是让模型看到“幽灵工具”。
4. 按岗位 + 明确任务类型选择技能；高风险任务禁止仅靠模糊语义自动选择。
5. 限制注入 Token，只注入最佳匹配技能正文，其他技能只暴露摘要。
6. 持久化 Skill 内容哈希、版本和选择原因，确保结果可复现。
7. Skill 的 `allowed-tools` 只是候选白名单，仍需叠加公司 RBAC、审批和预算策略。

**验收标准**

1. 前端员工不能获得生产数据库写工具。
2. 同一任务、同一 Skill 版本和 Mock 输入得到确定的技能选择结果。
3. 修改 Skill 后新运行使用新哈希，旧运行审计仍能定位旧版本。

### 第 8 条：建立分层记忆，而不是把全部聊天塞回上下文

**建议记忆分层**

1. Session：本次对话短期上下文，由 ADK SQLite Session 维护。
2. Employee memory：员工偏好、长期经验和已确认事实。
3. Project memory：项目约束、架构决定、常见故障和重要路径。
4. Company knowledge：组织制度、岗位规范和公共流程。

**实施步骤**

1. 保留公司 `agent_memory` 为权威记录；ADK Memory 作为可检索索引或执行侧适配器。
2. 使用 `company_id + project_id + employee_id` 做强隔离，禁止跨租户检索。
3. 只把显式确认、任务总结或高价值事实写入长期记忆，普通寒暄不写。
4. 每条记忆保存来源消息/任务、创建者、置信度、有效期和敏感等级。
5. 检索时限制 top-k 和总 Token，结果带来源，不允许模型把推断当成已确认事实。
6. 支持员工离职、项目删除和用户请求删除时同步删除索引。
7. V1 先使用 SQLite/轻量实现；没有容量证据时不要引入外部向量数据库。

**验收标准**

1. 公司 A 的内容无法在公司 B 的查询中命中。
2. 删除权威记忆后，检索索引不再返回该内容。
3. Mock embedder 可在无 API Key 的 CI 中稳定测试。

### 第 9 条：用 ArtifactService 管理版本化交付物

**目的**

将代码补丁、报告、截图、测试日志等大内容从消息正文分离，并保留版本历史。

**实施步骤**

1. 公司数据库继续保存制品元数据和权限；ADK ArtifactService 负责员工执行侧的文件版本。
2. 制品键必须包含公司、项目、任务、员工和运行范围，不能只用文件名。
3. 保存 SHA-256、MIME、大小、版本、创建运行、来源工具和安全扫描状态。
4. 消息和 A2A 只传 artifact reference，不直接复制大文件或二进制。
5. 制品更新产生 `artifact.created` / `artifact.updated` 领域事件。
6. 只有显式允许的任务参与者可以读取 A2A 制品。
7. 清理策略只删除已过期且无任务/审计引用的版本。

**验收标准**

1. 同名制品更新后可读取旧版本。
2. 消息删除不会级联误删仍被任务引用的制品。
3. 越权员工无法通过猜路径读取制品。

### 第 10 条：把 ADK Eval 接入回归测试和发布门禁

**建议评测集**

1. 工具轨迹：编码任务是否按“检查 -> 修改 -> 测试”调用工具。
2. 响应质量：任务总结是否包含改动、验证和风险。
3. 安全：是否泄露秘密、越权路径或跨公司信息。
4. 幻觉：未运行测试时不能声称测试已通过。
5. A2A 治理：未到场不能发送，到场后可发送，离场后停止。
6. 招聘公平：评分不能使用年龄、性别或外貌。
7. 预算：达到轮次或 Token 上限必须终止。
8. Mock 兼容：核心用例不依赖在线模型。

**实施步骤**

1. 创建版本控制的 `.test.json` 评测集和确定性 Mock 工具。
2. 对关键流程使用精确工具轨迹；对自然语言输出使用 rubric/语义评分。
3. 将非确定的 LLM judge 与确定性协议/权限测试分开，避免 CI 偶发失败。
4. 记录模型、prompt、Skill、工具和评测集版本。
5. PR 先跑小型确定性集；夜间或发布前再跑在线模型质量集。
6. 设定基线和允许退化幅度，不只看一次总分。

**验收标准**

1. 面对面策略、租户隔离、预算和秘密泄露测试为硬门禁。
2. 模型升级前后能生成可比较报告。
3. Mock 模式评测在无网络、无密钥环境中通过。

### 第 11 条：增加 Guardrail 的四层防线

**四层防线**

1. 输入：提示注入、敏感信息、超长内容和不允许的任务类型。
2. 模型输出：结构 schema、PII/秘密、危险建议和虚假完成声明。
3. 工具参数：路径、命令、域名、参数规模和公司范围。
4. 成本：单轮、单任务、单员工每日 Token/费用和工具次数。

**实施步骤**

1. 结构化输出必须先做 Serde schema 校验，失败后只允许一次定向修复。
2. 路径在宿主映射前 canonicalize，并验证位于员工工作区/制品目录内。
3. 网络采用域名 allowlist；重定向后的最终域名也要重新校验。
4. 检测到秘密时在发给模型、写日志和 A2A 转发三个位置分别拦截。
5. Guardrail 拒绝必须记录规则 ID、策略版本和安全摘要。
6. 不让 Guardrail 自己决定公司任务成功；它只允许、修改或拒绝当前步骤。

**验收标准**

1. `../`、符号链接和编码变体不能逃离工作区。
2. Token 达到硬上限时不会再发起下一次模型调用。
3. 日志、WebSocket 和数据库中都不出现测试秘密明文。

### 第 12 条：用 OpenTelemetry 串联公司、容器、模型和工具

**标识映射**

- Trace：一个 `agent_job` 或 A2A task。
- Span：运行、模型调用、工具调用、图节点、审批等待、A2A 转发。
- Attributes：`company.id`、`employee.id`、`task.id`、`runtime.run_id`、`a2a.context_id`、模型、工具、Token、状态。

**实施步骤**

1. 公司后端创建 trace context，通过 employee-runtime 请求传入容器。
2. ADK Telemetry/Plugin 创建子 span，并把上下文带回流式事件。
3. 敏感 prompt、响应和工具参数默认不作为 span 属性。
4. 本地开发继续使用结构化日志；生产可选 OTLP 导出，未配置时不能影响运行。
5. 建立指标：成功率、P50/P95 延迟、Token/任务、工具失败率、审批等待、图恢复次数、A2A 拒绝原因。
6. 给事件和日志保存 trace ID，界面审计页可跳转到同一次运行。

**验收标准**

1. 从一个 UI 任务可追踪到容器运行、模型、工具和 A2A 消息。
2. Telemetry 后端不可用时任务继续执行，且有本地错误指标。
3. 跨公司查询仍受公司后端权限控制。

### 第 13 条：用 Auth、ScopeGuard 和 Toolset 实施最小权限

**实施步骤**

1. 将岗位、公司、项目、任务和运行身份映射为工具 scope。
2. 用 deny-first 规则包装工具；工具内部仍要重复校验关键边界。
3. 根据任务动态组合 Basic/Filtered/Merged/Prefixed Toolset，避免每个员工看到全部工具。
4. MCP 服务由公司批准和配置，不允许 Agent 任意连接用户提供的 MCP 地址。
5. 外部凭据只由受控 credential provider 注入，不能进入 prompt、Skill、Session 或制品。
6. 每次权限检查记录主体、scope、工具、结果和策略版本。
7. `DevToolset` 的 shell、文件和网络能力分别授权，不用一个“开发工具”权限覆盖全部风险。

**验收标准**

1. 工具未出现在当前 Toolset 时，模型无法通过名称猜测调用。
2. 换岗位或换项目后，旧 scope 立即失效。
3. 凭据不会出现在 ADK Session 序列化数据中。

### 第 14 条：使用有界 Retry-Reflect，而不是无限自我修复

**适用错误**

- 模型临时格式错误。
- 编译/测试失败且有明确诊断。
- 可重试的限流、超时或短暂网络错误。

**不应自动重试**

- 权限拒绝、审批拒绝、预算耗尽。
- 相同参数导致的确定性失败。
- 面对面到场约束不满足。
- 删除、发布、支付等外部副作用不确定是否已发生。

**实施步骤**

1. 统一错误分类：transient、validation、policy、budget、side-effect-unknown、fatal。
2. 只对 transient/validation 启用反思，并给模型最小失败摘要。
3. 最大尝试次数同时受 ADK、Graph 和公司 `agent_jobs.attempts` 约束，选最小值。
4. 每次重试增加退避并记录原因；相同工具、相同参数、相同错误连续出现时熔断。
5. 外部副作用必须带幂等键，无法确认状态时转人工处理。

**验收标准**

1. 策略拒绝不会被反思改写成另一种绕过方式。
2. 相同确定性错误不会消耗完全部 Token 才停止。
3. 审计能区分一次运行内反思和公司队列级重试。

### 第 15 条：使用 RAG 提供项目知识检索，但从轻量试点开始

**适合索引的内容**

- `docs/` 产品规范、架构决定和操作手册。
- 项目 README、API schema、数据库迁移说明。
- 经筛选的源码符号、模块摘要和测试说明。

**实施步骤**

1. 先用 Markdown/recursive chunker 和确定性 Mock embedding 验证流程。
2. collection 必须按公司和项目隔离，metadata 保存来源路径、提交哈希和权限标签。
3. 只索引允许员工读取的文件；检索结果再次做权限过滤。
4. 文件变化时增量重建对应文档，不全量重复嵌入。
5. 每次答案保留来源 URI/路径和提交版本。
6. V1 优先嵌入式或内存实现；没有规模证据时不引入 Qdrant、Postgres 或其他服务。
7. RAG 是参考上下文，不能替代实际读文件、运行测试或公司权威状态查询。

**验收标准**

1. 检索结果只来自当前公司和项目的授权文件。
2. 删除或改权限后旧向量不可再命中。
3. 回答能追溯到具体来源和版本。

### 第 16 条：将浏览器能力限定为隔离的 QA 工具

**适用场景**

前端员工完成页面后，QA 员工在隔离浏览器中打开本地预览、点击、输入、提取文本和截图，生成验收制品。

**实施步骤**

1. 浏览器运行在独立容器/服务，不与员工主工作区共享浏览器用户目录。
2. 默认只允许本地预览地址和测试环境 allowlist。
3. 每个公司/运行独立 browser context，完成后销毁。
4. 下载、上传、剪贴板、凭据输入和生产环境操作默认禁用并要求审批。
5. 截图作为版本化制品保存，DOM/控制台错误作为结构化运行事件。
6. Mock 模式提供确定性的虚拟浏览器工具结果。

**验收标准**

1. QA 员工不能访问其他公司的 cookie、下载或页面。
2. 未在 allowlist 的域名无法打开，即使通过重定向也不行。
3. 浏览器失败不会被误报为功能验收通过。

### 第 17 条：保留 Docker 外层隔离，Wasm 仅用于小型代码节点

ADK Sandbox 的进程后端主要提供超时和环境隔离，不能代替容器的文件系统、网络和资源边界。项目已有每员工容器，因此：

1. 继续使用 Docker/容器作为员工 runtime 的主要安全边界。
2. 禁止用普通 ProcessBackend 取代容器隔离。
3. 对表达式、数据转换或用户生成的小型纯计算代码，可试点 WasmBackend。
4. Wasm 默认无文件、网络和宿主环境访问，只暴露必要输入输出。
5. 资源限制由公司后端配置，执行结果仍写 runtime event 和 tool call 审计。

### 第 18 条：暂缓 Realtime 和 Computer Use

1. Realtime 语音可在未来用于“会议室语音交流”，但当前文字 A2A、治理和审计更优先。
2. Computer Use 在 ADK-Rust 2.0 稳定性说明中仍为 Beta，且桌面控制攻击面明显大于浏览器 QA。
3. V1 不把 Computer Use 放进默认员工镜像，也不允许访问宿主桌面。
4. 若未来试点，必须在一次性桌面容器中、无真实凭据、全程录屏/审计、域名与应用 allowlist、关键动作审批。

## 5. 建议的数据与接口演进

现有迁移已经包含 `runtime_runs`、`runtime_tool_calls`、`runtime_checkpoints`、`a2a_face_to_face_sessions`、`a2a_tasks`、`a2a_messages` 和 `a2a_audit_log`。应优先扩展而不是创建语义重复的表。

### 5.1 建议扩展字段

1. `runtime_runs`：`trace_id`、`skill_hash`、`policy_version`、`graph_name`、`graph_version`、`last_event_sequence`。
2. `runtime_tool_calls`：`function_call_id`、`risk_level`、`arguments_hash`、`approval_id`、`policy_rule_id`、`idempotency_key`。
3. `runtime_checkpoints`：明确区分容器文件快照与 ADK Graph 状态检查点；保存 graph/thread/checkpoint 标识和 schema 版本。
4. `a2a_messages`：若尚无对应字段，增加轮次、输入/输出 Token、终止原因和 artifact 引用关系。
5. `a2a_audit_log`：增加 actor、policy version、rule ID、trace ID 和安全摘要。

字段增加前先检查现有 schema，能从 JSON payload 稳定读取且无需索引的字段可以暂不拆列。

### 5.2 建议只读 API

```text
GET /api/companies/:companyId/communications
GET /api/companies/:companyId/a2a/sessions/:sessionId
GET /api/companies/:companyId/a2a/sessions/:sessionId/messages
GET /api/companies/:companyId/a2a/sessions/:sessionId/audit
GET /api/companies/:companyId/runtime/runs/:runId/events
GET /api/companies/:companyId/runtime/runs/:runId/tool-calls
```

所有列表使用游标分页；正文、审计和制品分别鉴权；不得接受客户端传入的 employee 身份作为可信主体。

## 6. 推荐代码改造位置

1. `apps/employee-runtime/src/main.rs`：拆分构建 Agent、协议、插件、工具和策略。
2. `apps/employee-runtime/src/protocol.rs`：新增 NDJSON 事件 envelope 和版本兼容解析测试。
3. `apps/employee-runtime/src/plugins.rs`：治理、预算、审计、脱敏和 telemetry 插件。
4. `apps/employee-runtime/src/workflows/`：Graph 工作流和 checkpoint 适配器。
5. `apps/server/src/adk_runtime.rs`：流式读取、幂等事件写入、审批恢复和 trace 传播。
6. `apps/server/src/runtime_store.rs`：运行、工具调用、检查点和事件的原子持久化。
7. `apps/server/src/a2a.rs`：A2A 标准模型适配、到场策略和幂等检查。
8. `apps/server/src/routes/communications.rs`：A2A 会话、消息、任务和审计只读 API。
9. `apps/web/src/features/communications/`：会话列表、时间线、筛选、详情和制品查看器。
10. `apps/web/src/store/company-store.ts`：消费新的领域事件，只更新事件级状态，不参与逐帧动画。

## 7. 分阶段实施计划

### 阶段 A：可见与可审计

1. A2A 沟通查看器。
2. employee-runtime NDJSON 流式事件。
3. runtime/A2A 事件幂等与分页 API。
4. Mock 模式端到端演示。

完成标志：用户能在界面上看到一次真实的“邀请 -> 行走 -> 双方到场 -> 两轮 A2A -> 返回工位”全过程及消息正文、任务关联和终止原因。

### 阶段 B：可控制与可恢复

1. Plugin 治理管线。
2. Tool Confirmation 审批。
3. Graph 编码工作流与 SQLite checkpoint。
4. 预算硬限制、取消、重试和熔断统一。

完成标志：容器崩溃后可恢复；高风险工具未审批不会执行；达到限制后不会继续消耗 Token。

### 阶段 C：可复用与可评测

1. Skill 与严格 ToolRegistry。
2. Eval 回归集和 CI 门禁。
3. Memory、Artifact 和 Telemetry。
4. A2A 标准一致性测试。

完成标志：岗位能力可版本化，模型/Prompt/Skill 升级有量化回归报告，运行可跨进程追踪。

### 阶段 D：按证据扩展

1. RAG 项目知识检索。
2. 隔离浏览器 QA。
3. Wasm 小型计算节点。
4. 根据实际规模再评估外部向量库、实时语音或 Computer Use。

## 8. 每个优化项的统一完成定义

每一项进入“完成”前必须满足：

1. 公司后端仍为权威数据源。
2. 有 `company_id` 隔离和越权测试。
3. 有 Mock 模式测试，不依赖 API Key。
4. 有失败、超时、取消、重放和重启场景测试。
5. 有 Token/工具次数/时间上限，不存在无限循环。
6. 有可查询审计，但不记录秘密和模型内部推理。
7. 重要 UI 状态由领域事件驱动。
8. 前端没有逐帧 React state 更新，角色动画继续由 Pixi 管理。
9. 通过 Web 的 typecheck/build/test 和 Server 的 fmt/clippy/test。
10. ADK 版本升级时先跑 eval 与协议兼容测试，再更新锁文件。

## 9. 版本与资料使用注意事项

项目锁定 ADK-Rust `2.0.0`，而 docs.rs 的 `latest` 页面在不同 crate 上可能仍展示 `1.0.x` API。实施时应遵循：

1. 先看 `Cargo.lock` 实际解析版本。
2. 以 ADK-Rust 仓库对应的 `2.0.0` tag/source 和迁移说明为准。
3. docs.rs 用于理解能力和类型，不直接复制未核对版本的代码片段。
4. 每引入一个新 crate，只开启需要的 feature，避免使用 `full` 把浏览器、RAG、Realtime 等全部带入员工镜像。
5. DevTools 和 Computer Use 在 2.0 稳定性说明中为 Beta；现有 DevToolset 应包在公司策略和容器边界内。

## 10. 官方资料索引

- [ADK-Rust 官方仓库](https://github.com/zavora-ai/adk-rust)
- [ADK-Rust 稳定性政策与 2.0 状态](https://github.com/zavora-ai/adk-rust/blob/main/STABILITY.md)
- [ADK-Rust 功能与 feature 列表](https://docs.rs/crate/adk-rust/latest/source/Cargo.toml.orig)
- [ADK Server 与 A2A](https://docs.rs/adk-rust/latest/adk_rust/server/index.html)
- [ADK Graph](https://docs.rs/adk-graph/latest/adk_graph/)
- [ADK Plugin](https://docs.rs/adk-plugin/latest/adk_plugin/)
- [ADK Skill](https://docs.rs/adk-skill/latest/adk_skill/)
- [ADK Eval](https://docs.rs/adk-eval/latest/adk_eval/)
- [ADK Memory](https://docs.rs/adk-memory/latest/adk_memory/)
- [ADK Artifact](https://docs.rs/adk-rust/latest/adk_rust/artifact/index.html)
- [ADK Telemetry](https://docs.rs/adk-telemetry/latest/adk_telemetry/)
- [ADK Auth](https://docs.rs/adk-auth/latest/adk_auth/)
- [ADK Tool 与 Toolset](https://docs.rs/adk-tool/latest/adk_tool/)
- [ADK RAG](https://docs.rs/adk-rag/latest/adk_rag/)
- [ADK Sandbox](https://docs.rs/adk-sandbox/latest/adk_sandbox/)
- [ADK Browser](https://docs.rs/adk-browser/latest/adk_browser/)
