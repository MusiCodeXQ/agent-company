# AI Company × OpenClaw 集成实施规格（给开发 AI）

> 目标：在当前 AI Company（Rust Backend + React/PixiJS + SQLite + WebSocket + Floor/Employee/HR/Goal/Task/Meeting/Movement）架构上，把“每个员工”真正绑定为一个长期运行的 OpenClaw Agent。员工可以使用 OpenClaw 的工具、Skills、MCP、浏览器、文件、Shell，以及按岗位选择 Codex 等 Harness；员工之间可以主动沟通、开会、交接任务，并且所有公司内部沟通必须在 AI Company 中可见、可检索、可审计。

## 0. 先明确最终产品模型

不要把 OpenClaw 当成新的业务后端，也不要让 OpenClaw 接管公司组织逻辑。

最终职责边界必须固定为：

```text
AI Company Backend（Rust）
= 公司世界 / 业务真相 / 管理层

负责：
Company / Floor / Room / Workstation
Employee / Candidate / HR / Role
Goal / Project / Task / Workflow
Meeting / Recruitment / Onboarding
Company Message / Conversation
Movement / Location / Presence
Permission / Audit / Cost / Artifact
前端 Snapshot / WebSocket Event

                │
                │ OpenClaw Gateway Adapter
                ▼

OpenClaw
= 员工的大脑 + 工具运行环境

负责：
LLM Agent Loop
Session / Context / Memory
Skills / MCP / Tools
File / Shell / Browser
Codex Harness / ACP Harness
Sandbox Tool Execution
Agent Runtime Transcript
```

**AI Company 数据库是公司业务的唯一真相源（source of truth）。**

OpenClaw 的 session/transcript 是 Runtime 内部状态和上下文镜像，不能代替 AI Company 的 Employee、Task、Message、Meeting、Artifact 数据库。

---

# 1. 一个 Employee = 一个独立 OpenClaw Agent

当前每一个正式员工 Employee，在完成入职后都可以绑定一个 OpenClaw Agent。

例如：

```text
Employee
王浩
Backend Engineer
employee_id = emp_1008

        ↓ 1:1 runtime binding

OpenClaw Agent
agentId = company_emp_1008
workspace = /data/openclaw/workspaces/company_emp_1008
agentDir   = OpenClaw 自己独立的 agent state
```

不要用“一个 OpenClaw Agent 模拟所有员工”。

每个人必须拥有自己独立的：

- OpenClaw agentId
- workspace
- agentDir
- session history
- persona / identity
- memory
- model/runtime profile
- skills/tools policy
- sandbox

新增数据模型：

```text
employee_runtime_binding

id
employee_id
runtime_type            OPENCLAW
runtime_agent_id        company_emp_xxx
runtime_profile_id
workspace_path
provision_status        PENDING / PROVISIONING / READY / ERROR / DISABLED
last_health_at
last_error
created_at
updated_at
```

Employee 本身不要存 OpenClaw 的大量配置，只保存 runtime binding。

---

# 2. 一个公司优先使用一个 OpenClaw Gateway

当前产品是“一个老板管理自己的一家公司”的信任模型，所以第一版建议：

```text
AI Company Backend
        │
        ▼
1 × OpenClaw Gateway
        │
        ├── HR 苏晴
        ├── Product
        ├── Backend 王浩
        ├── Frontend
        ├── QA
        ├── Writer A
        ├── Writer B
        └── Reviewer
```

不要每个员工都启动一个 OpenClaw Gateway 进程。

OpenClaw 本身支持一个 Gateway 中运行多个隔离 Agent，每个 Agent 有自己的 workspace、agentDir 和 SQLite session history。

但是未来如果 AI Company 做 SaaS、多租户，不同客户公司属于不同信任边界，则应该：

```text
Company A → Gateway A
Company B → Gateway B
```

不要把互不信任的客户共用一个 Gateway。

---

# 3. 新员工入职时自动 Provision OpenClaw Agent

把“录用候选人”扩展成真正的 Runtime Provisioning Workflow。

流程：

```text
Candidate APPROVED
↓
Create Employee
↓
Create Workstation
↓
Create EmployeeRuntimeBinding(PENDING)
↓
生成 OpenClaw workspace
↓
生成 Agent 身份文件
↓
创建 OpenClaw Agent
↓
配置 Model / Tools / Skills / Sandbox
↓
Runtime Health Check
↓
Binding READY
↓
开始 Onboarding Movement
↓
新人从公司入口走到自己的楼层和工位
↓
正式 AVAILABLE
```

建议给每个员工的 workspace 自动生成：

```text
AGENTS.md
SOUL.md
IDENTITY.md
USER.md（只放公司老板和工作相关的稳定信息，不要塞无关隐私）
MEMORY.md
memory/
artifacts/
notes/
```

IDENTITY.md 来源于 Employee：

```text
姓名：王浩
职位：Backend Engineer
所属楼层：F1 研发中心
所属团队：Backend
```

SOUL.md 来源于：

```text
Employee.personality
沟通风格
性格
价值倾向
工作习惯
边界
```

AGENTS.md 来源于：

```text
RoleDefinition
Floor Mission
Company Rules
工具使用原则
权限原则
公司沟通原则
```

不要把 Employee 的全部数据库内容拼成超长 prompt。

---

# 4. Runtime Profile：不同岗位拥有不同能力

新增 `runtime_profile`，不要让所有员工都拿到相同工具权限。

例如：

```text
HR_PROFILE
runtime = openclaw
skills = recruitment, web_search, docs
shell = false
filesystem = limited
browser = true
company_tools = true

WRITER_PROFILE
runtime = openclaw
skills = web_search, research, writing, docs
browser = true
shell = false
company_tools = true

BACKEND_PROFILE
runtime = codex-through-openclaw
skills = coding, git, docs
shell = true
filesystem = rw project worktree
browser = optional
company_tools = true

QA_PROFILE
runtime = codex-through-openclaw
shell = true
filesystem = project/test scope
browser = playwright
company_tools = true
```

建议数据：

```text
runtime_profile

id
name
runtime_owner        OPENCLAW / CODEX / ACP
model_profile_id
sandbox_profile_id
tool_policy_json
skills_json
mcp_policy_json
config_json
enabled
```

具体岗位只引用 Runtime Profile。

未来新建“视频剪辑”“销售”“数据分析”等岗位时可以新增配置，而不是修改 Rust enum。

---

# 5. 编程员工不要绕过 OpenClaw：使用 OpenClaw 的 Codex Harness

Backend / Frontend / QA 等 coding employee 仍然是一个 OpenClaw Employee Agent。

不要把“王浩”和“Codex”做成两个员工。

正确理解：

```text
王浩（Employee）
↓
OpenClaw Agent（身份、记忆、公司沟通、工具入口）
↓
Codex Harness（具体 coding runtime）
↓
Sandbox / Git Worktree
```

OpenClaw 当前已有官方 Codex Harness，可通过 Codex app-server 执行 agent turn。

因此：

- 员工身份属于 OpenClaw Agent。
- 公司消息属于 AI Company。
- coding session 可以由 Codex Harness 执行。
- Codex 产生的运行事件再映射成办公室里的 CODING / TESTING / READING 等状态。

Claude Code、Gemini CLI、OpenCode 等其他外部 Harness 后续通过 ACP 接入，不要第一版同时实现全部 Harness。

第一阶段优先：

```text
OpenClaw Embedded Agent
+
OpenClaw Codex Harness
```

---

# 6. 每个员工必须有真正的 Sandbox，不只是文件夹

OpenClaw workspace 只是默认工作目录，不是真正安全边界。

每个员工默认启用 OpenClaw sandbox：

```text
mode = all
backend = docker
scope = agent
workspaceAccess = rw
```

核心原则：

```text
1 Employee
=
1 OpenClaw Agent
=
1 独立 Agent Workspace
=
1 独立 Sandbox（scope=agent）
```

严禁普通员工获得：

- `/var/run/docker.sock`
- 宿主机 SSH Key
- 宿主机 ~/.config
- 其他员工 workspace
- 整个宿主机 home
- AI Company 数据库文件直接访问权限

普通员工只能通过受控 Company Tool/API 操作公司业务。

---

# 7. Coding 员工使用独立 Git Worktree

对于同一个软件项目：

```text
project-main

worktrees/
  emp_backend_01/
  emp_frontend_01/
  emp_qa_01/
```

Company Backend 创建和管理 worktree。

OpenClaw Sandbox 给对应员工挂载：

```text
/project → 该员工自己的 worktree（rw）
```

需要共享参考资料时：

```text
/reference → ro
```

QA 是否有源码写权限由 Runtime Profile 决定。

Supervisor 才拥有 merge/approve 权限。

不要让普通 Agent 直接操作 Docker socket 来创建自己的容器。

---

# 8. Rust Backend 与 OpenClaw 的连接方式

不要在每次员工工作时通过 `spawn("openclaw agent ...")` 新建 CLI 子进程作为主要 Runtime 通道。

正式 Runtime 通道使用 OpenClaw Gateway WebSocket Protocol。

新增：

```text
OpenClawGatewayClient
```

建议 Rust：

```text
tokio-tungstenite
serde / serde_json
```

负责：

```text
connect / authenticate
request correlation
start agent run
stream events
receive completion
cancel run
session lookup
health check
reconnect
protocol version check
```

OpenClaw Gateway 使用 WebSocket + JSON request/response/event frame。

请把所有 OpenClaw 协议细节封装在：

```text
infrastructure/openclaw/
```

业务层禁止直接拼 Gateway JSON。

建议接口：

```rust
trait AgentRuntimeAdapter {
    async fn run_turn(...);
    async fn send_message(...);
    async fn cancel_run(...);
    async fn get_run_status(...);
    async fn health(...);
}
```

OpenClaw 实现：

```text
OpenClawRuntimeAdapter
```

以后换 Runtime 不影响 Company Domain。

### Agent 创建/删除

Provisioning 第一版可以由受控后台调用官方：

```text
openclaw agents add ... --non-interactive --json
openclaw agents set-identity ... --json
openclaw agents delete ... --force --json
```

但这些 CLI 只能由 AI Company 的 Runtime Provisioner 调用，不允许员工 Agent 自己调用。

消息/工作 Run 仍使用 Gateway WS，不要每次启动 CLI。

同时固定/pin OpenClaw 版本，不要生产环境自动使用 latest。

---

# 9. 最关键：员工之间不要直接“偷偷 sessions_send”

我要在 AI Company 中看到所有员工的沟通记录。

因此公司内部通信必须遵循：

```text
Employee A
↓
Company Messaging Tool
↓
AI Company Backend
↓
先持久化 Message
↓
Message Delivery Job
↓
OpenClaw Gateway
↓
Employee B 的 Agent Session
↓
Employee B 回复
↓
AI Company Backend 持久化
↓
再投递给 A / 更新 UI
```

**Company Message Bus 是唯一正式的员工通信总线。**

不要让 Employee A 直接调用 OpenClaw `sessions_send` 给 B 后完全绕开 AI Company 数据库。

否则：

- UI 看不到记录
- Meeting/Task 无法关联
- 无法审计
- 无法统计
- 无法做未读
- 无法回放
- 无法阻止无限互聊

如果 OpenClaw 自带跨 session 消息能力，需要在员工 Tool Policy 中禁止或隐藏直接 A2A 发送能力，正式公司通信走我们的 Company Tool。

---

# 10. 实现一个 `ai-company-bridge` OpenClaw Tool Plugin

这是本次集成非常重要的一层。

建议编写一个很薄的 OpenClaw 插件：

```text
ai-company-bridge
```

它不保存公司数据，只负责把 Agent Tool Call 转发给 Rust Backend。

插件至少提供：

```text
company_get_context
company_directory_search
company_send_message
company_reply_message
company_request_help
company_request_meeting
company_get_task
company_update_task
company_publish_artifact
company_request_handoff
```

例如 Backend Agent 想找 Frontend：

```text
company_directory_search("frontend")
↓
找到 employee_id
↓
company_send_message(
  to_employee_id,
  text,
  goal_id,
  task_id,
  requires_reply=true
)
```

OpenClaw Plugin 将：

```text
OpenClaw agentId
+
tool arguments
+
service authentication
```

发送给 Rust internal API。

Rust Backend 根据 runtime_agent_id 反查 employee_id，**不要信任模型自己提交的 from_employee_id**。

这样可以防止一个 Agent 假装成另一个员工。

### 为什么建议 Plugin，而不是直接让 Agent 调 Rust REST

因为 Plugin 可以统一：

- 身份识别
- Tool schema
- 权限
- 审计
- Company API token
- 错误处理

并且 OpenClaw 的动态工具可以继续提供给 Codex Harness。

以后如果要给 ACP Harness 使用同一套 Company Tools，再增加 MCP Bridge，不要第一步就把所有路径做复杂。

---

# 11. Company Conversation / Message 数据模型

新增正式消息系统：

```text
company_conversation

id
company_id
type          DIRECT / GROUP / MEETING / TASK / SYSTEM
floor_id      nullable
goal_id       nullable
task_id       nullable
meeting_id    nullable
title
status
created_at
updated_at
```

```text
conversation_member

conversation_id
employee_id
member_role
joined_at
left_at
last_read_message_id
```

```text
company_message

id
conversation_id
sender_type       USER / EMPLOYEE / SYSTEM
sender_employee_id nullable
recipient_employee_id nullable
message_type      TEXT / ARTIFACT / TASK / SYSTEM / ACTION
content
correlation_id
reply_to_message_id
requires_reply
created_at
```

```text
message_delivery

id
message_id
recipient_employee_id
status        PENDING / DELIVERING / DELIVERED / READ / FAILED
runtime_run_id
attempt_count
last_error
created_at
delivered_at
read_at
```

### 核心原则

**Message 必须先入 AI Company DB，再投递 OpenClaw。**

绝对不要：

```text
先 OpenClaw 聊完
最后再“尽量同步”数据库
```

因为中间崩溃就会丢公司记录。

---

# 12. Direct Message：两个员工怎么聊天

例如 Product 对 Backend 说：

> 登录接口字段已经确定，你按最新 PRD 开发。

真实流程：

```text
Product Agent
↓ company_send_message
Rust Backend
↓
conversation = Product ↔ Backend
↓
保存 Message #1001
↓
创建 MessageDelivery
↓
WebSocket UI：MESSAGE_CREATED
↓
Backend 人物 device = PHONE
↓
OpenClawGatewayClient.run_turn(Backend)
↓
Backend 读取消息并思考
↓
Backend 回复
↓
保存 Message #1002
↓
Product 收到新消息
```

办公室视觉状态：

```text
收到消息：CHECK_PHONE
模型思考：THINKING
需要写代码：先走回 Workstation → 坐下 → COMPUTER → CODING
```

所以“Agent Communication”和“Office Movement”必须联动，但不能耦合在 OpenClaw 内部。

---

# 13. Session 设计：不要把所有聊天塞进一个 OpenClaw Session

同一个员工需要多个上下文：

```text
与老板的手机私聊
与 Product 的工作对话
与 QA 的 Bug 对话
某一次 Meeting
某一个 Task
```

不要全部进入一个无限增长 main session。

AI Company 维护 `conversation_id`，OpenClaw Adapter 为不同业务上下文维护稳定 Session Mapping：

```text
runtime_session_binding

id
employee_id
context_type       USER_CHAT / DIRECT / MEETING / TASK
context_id         company conversation / meeting / task id
runtime_session_key
last_used_at
```

原则：

```text
一个 Company Conversation
在每个参与 Employee 的 OpenClaw 侧
拥有自己的稳定 Runtime Session Context。
```

例如会议 M100：

```text
Product → session for meeting M100
Backend → session for meeting M100
QA      → session for meeting M100
```

Meeting 结束后 session 可以归档，但 Company Transcript 永久在 AI Company DB。

实现时请以当前 OpenClaw Gateway 支持的 session key/session label/session selector 为准，不要自己假定不存在的 RPC 字段。

---

# 14. 防止 Agent 无限互相聊天

必须从第一版加入通信限流，不然两个 Agent 很容易：

```text
A：你怎么看？
B：你怎么看？
A：我同意，你再说说？
...
```

新增规则：

```text
max_reply_rounds_per_request = 4
max_agent_hops = 8
max_messages_per_conversation_per_minute
max_tokens_per_conversation
conversation timeout
```

Message 带：

```text
correlation_id
root_message_id
hop_count
```

同一个 correlation 超过限制：

```text
自动停止
↓
上报 Supervisor
```

默认 Agent-to-Agent 消息使用异步模式。

`company_send_message` 应先返回：

```text
message_id
accepted
```

不要让发送 Agent 永久同步等待目标 Agent。

如果确实需要答案，使用：

```text
requires_reply = true
```

由当前 Agent Job 进入 WAITING_FOR_MESSAGE，收到回复后恢复。

---

# 15. 会议必须通过 Company Meeting Orchestrator，而不是让 Agents 自由群聊

现有 Meeting + Movement 继续保留。

真实流程：

```text
Create Meeting
↓
邀请 Employee
↓
每个人 WALKING 去会议室
↓
到达自己的 seat
↓
WAITING_MEETING
↓
全员到齐
↓
Meeting IN_PROGRESS
↓
创建 company_conversation(type=MEETING)
↓
Meeting Orchestrator 控制发言
```

第一版发言策略不要让 LLM 自己无限抢话。

建议：

```text
Chair/Supervisor 开场
↓
Round Robin 每人一次
↓
Chair 根据结果决定是否第二轮
↓
形成 Decision / Action Items
↓
Meeting FINISHED
```

每次 Agent 发言：

```text
Meeting Orchestrator
↓
调用该 Employee 对应 OpenClaw Meeting Session
↓
返回发言
↓
保存 company_message
↓
前端实时显示字幕/气泡
```

所以 Meeting Transcript 必须完全可见。

前端会议详情显示：

```text
14:20 周淼 Product：...
14:21 王浩 Backend：...
14:22 李欣 QA：...
```

用户可以实时旁观，也可以在会议中插话。

用户插话也是一条 `company_message(sender_type=USER)`。

---

# 16. 自媒体选题会也复用 Meeting

不要给 F2 自媒体另写一套 Agent Chat。

例如：

```text
Topic Planner
Writer A
Writer B
Reviewer
Media Supervisor
```

参加选题会议：

```text
Movement → Meeting Room
↓
Meeting Conversation
↓
每个人用自己的 OpenClaw Agent 思考/发言
↓
所有发言落 company_message
↓
形成 Topic Brief Artifact
↓
Goal/Workflow 继续
```

这样研发评审会、面试会、选题会、销售复盘会都用同一个基础能力。

---

# 17. 用户必须能看到员工之间所有正式沟通记录

前端增加“公司消息 / 内部沟通”能力。

至少提供三种入口：

### A. 点击员工 → 手机 → 消息

显示：

```text
与老板
同事消息
群组/会议
```

例如点击 Backend 王浩：

```text
周淼 Product        2 条未读
李欣 QA             1 条未读
研发会议 M100
```

点击进入完整历史。

### B. 公司左侧导航增加“内部消息”

可以按：

```text
全部
Direct
Meeting
Task
Floor
Employee
```

筛选和搜索。

### C. Meeting 页面

实时显示 Meeting Transcript。

办公室里如果 A 给 B 发消息，可以短暂显示：

```text
A 📱 → B 📱
```

但不要把全部文本永久漂在办公室里。

---

# 18. “私有思考”和“公司沟通”必须分开

不要把 OpenClaw/Codex 的隐藏推理过程、内部 chain-of-thought 当成公司聊天记录。

公司可见的是：

- 员工明确发送的 message
- 正式会议发言
- Task 更新
- Artifact
- Tool Activity 的摘要状态

而不是：

- 私有内部推理文本
- 隐藏 chain-of-thought
- 大量原始 tool payload

例如 UI 可以显示：

```text
王浩：正在分析需求
王浩：正在读取 user.rs
王浩：正在运行 cargo test
```

不要要求展示模型私有推理全文。

---

# 19. OpenClaw Runtime Event → Office State Adapter

建立：

```text
RuntimeEventNormalizer
```

把 OpenClaw/Codex Runtime Event 转成 AI Company Domain Event。

例如：

```text
runtime run started
→ AgentActivity.THINKING

read file
→ READING

write/edit/apply_patch
→ CODING

shell + test command
→ TESTING

browser tool
→ BROWSING

company_send_message
→ Device.PHONE / COMMUNICATING

run complete
→ 根据 Task 决定 IDLE / WAITING / 下一 Activity

runtime error
→ ERROR
```

前端仍然只消费 AI Company Domain Event，不要直接理解 OpenClaw 原始事件。

这样以后换 Harness，PixiJS 不用改。

---

# 20. Company MCP/Plugin 工具要和权限绑定

不同角色能调用的 Company Tool 不同。

例如：

```text
普通员工：
company_directory_search
company_send_message
company_get_task
company_update_task
company_publish_artifact
company_request_help

Manager：
以上 +
company_create_task
company_request_meeting
company_assign_task

HR：
以上 +
company_create_hiring_request
company_update_candidate
company_request_interview

Supervisor：
更高级组织操作，但仍不能绕过 Validator。
```

即使 LLM 调用了工具，Rust Backend 仍然必须再次权限校验。

OpenClaw Tool Policy 是第一层。
AI Company Authorization 是第二层。

不能只相信 Prompt。

---

# 21. Artifact：员工写文档、代码、报告以后如何交付

不要把长文档全部塞到 Message content。

新增：

```text
artifact

id
company_id
project_id
goal_id
task_id
created_by_employee_id
type       DOCUMENT / CODE / IMAGE / REPORT / DATA / OTHER
title
storage_uri
mime_type
version
metadata_json
created_at
```

Agent 使用：

```text
company_publish_artifact
```

发布文件。

聊天消息中只引用：

```text
artifact_id
```

例如 Writer：

```text
“初稿完成，请审核。”
附件：article-v1.md
```

Reviewer 点开即可查看。

Coding 员工交付优先使用：

```text
Git branch / commit / diff
```

Artifact 保存 commit/ref 信息。

---

# 22. 员工主动工作与后台持续运行

前端关闭绝不能影响 Agent。

真实链路：

```text
Persistent Agent Job
↓
AI Company Worker
↓
OpenClaw Gateway Run
↓
Runtime event streaming
↓
DB
↓
Company Event
```

浏览器只是：

```text
Snapshot + WebSocket Observer
```

如果 AI Company Backend 重启：

- RUNNING job 要进入 RECOVERING。
- 重新查询/对账 OpenClaw session/run 状态。
- 可以安全恢复则恢复。
- 无法确认则幂等重试或人工处理。

不要因为 WebSocket 前端断开取消 Agent Run。

---

# 23. OpenClaw 不负责 Company Scheduler

不要把整个公司的 Goal / Task / Workflow 都改成 OpenClaw 自己的 session/subagent。

Company Scheduler 仍由 Rust Backend 控制。

例如：

```text
Goal：完成登录功能

Task 1 Product：定义接口
Task 2 Backend：实现接口
Task 3 Frontend：对接
Task 4 QA：测试
```

Rust 判断依赖关系：

```text
Task1 完成
↓
发送正式 Company Message 给 Backend
↓
创建 Backend Agent Job
↓
OpenClaw 执行
```

OpenClaw 是 Employee Runtime，不是整个公司的 Workflow Database。

---

# 24. Agent 可以请求帮助，但不能私自创建“影子员工”

允许 Agent：

```text
company_request_help
```

例如 Backend：

> 这个任务需要数据库专家协助。

Rust Backend：

```text
记录 Help Request
↓
Supervisor 决定转给现有员工 / HR 招聘 / 创建临时 subtask
```

OpenClaw 自己的 sub-agent 可以用于 Employee 内部临时工作，但：

**OpenClaw sub-agent 不等于公司正式 Employee。**

它不会自动：

- 获得姓名
- 获得工位
- 出现在办公室
- 进入 HR 花名册

只有 HR Hiring + Employee Creation 才能创建正式员工。

---

# 25. 对 OpenClaw 原生 sessions_send 的策略

OpenClaw 自身有跨 session/agent 协作工具，这是很有用的 Runtime 能力。

但是在 AI Company 产品中需要分两类：

### 正式公司通信

必须：

```text
company_send_message
→ AI Company DB
→ OpenClaw Target Session
```

### Employee 内部 Runtime 子任务

可以使用 OpenClaw 自己的 sub-agent/session orchestration，但属于员工内部执行细节。

不要把它显示成正式“同事对话”。

如果一个正式 Employee A 要联系正式 Employee B，必须经过 Company Message Bus。

第一版建议对正式 Employee Agent 直接 deny `sessions_send` A2A，避免绕开审计。

---

# 26. 安全底线

必须遵守：

1. 每个 Employee 独立 agentId、workspace、agentDir。
2. Sandbox 明确开启；不要依赖 workspace 当安全边界。
3. Sandbox scope 优先 `agent`。
4. 默认不给 host elevated exec。
5. 不挂载 docker.sock。
6. 不把 OpenClaw Gateway admin token 暴露给员工 sandbox。
7. Company Bridge 使用独立 service credential。
8. Rust 根据 runtime agent identity 验证 Employee 身份。
9. Project mount 最小化，能 ro 就 ro。
10. HR/Writer 等非 coding 角色默认不给 shell。
11. Network 默认按角色最小开放。
12. 所有 Company mutation 经过 Validator/Authorization。
13. Agent 不能直接写 AI Company SQLite。

---

# 27. 建议增加的数据表

本阶段建议最少增加：

```text
employee_runtime_binding
runtime_profile
runtime_session_binding
agent_run
runtime_event

company_conversation
conversation_member
company_message
message_delivery

artifact
```

如果已有相同概念，扩展已有表，不重复建模。

---

# 28. 建议增加的 Rust 模块

```text
src/
  domain/
    messaging/
    runtime/
    artifact/

  application/
    runtime/
      run_employee.rs
      provision_employee.rs
    messaging/
      send_message.rs
      deliver_message.rs
      receive_agent_reply.rs

  infrastructure/
    openclaw/
      gateway_client.rs
      protocol.rs
      runtime_adapter.rs
      provisioner.rs
      event_normalizer.rs

    persistence/
      conversation_repository.rs
      runtime_repository.rs

  api/
    internal/
      company_bridge.rs
```

名称按现有工程风格调整，不要强行重构整个项目。

---

# 29. Company Bridge Internal API 建议

供 `ai-company-bridge` OpenClaw plugin 调用，不直接暴露给公网：

```text
POST /internal/agent/context
POST /internal/agent/messages
POST /internal/agent/messages/{id}/reply
GET  /internal/agent/directory/search
GET  /internal/agent/tasks/{id}
POST /internal/agent/tasks/{id}/status
POST /internal/agent/meetings/request
POST /internal/agent/artifacts
POST /internal/agent/help-requests
```

每次调用包含由 Bridge 注入的 runtime identity。

Backend 反查：

```text
runtime_agent_id → employee_id
```

模型自己不能指定 sender identity。

---

# 30. 一个真实的端到端研发协作 Demo

完成后必须人工验证下面场景。

## 场景

F1 有：

```text
周淼 Product
王浩 Backend
陈曦 Frontend
李欣 QA
```

四个人都是真实 OpenClaw Agent。

王浩使用 Codex Harness。
陈曦使用 Codex Harness。
李欣使用 Codex Harness 或 OpenClaw coding profile。

### Step 1

用户给 Product：

> 给系统增加用户备注字段。

### Step 2

Product Agent 分析后，通过：

```text
company_send_message
```

给 Backend：

> 接口和数据库增加 remark 字段，需求见 Task T100。

AI Company：

- 保存这条消息。
- UI 内部消息可看到。
- Backend 手机出现未读。

### Step 3

Backend 收到消息。

如果王浩不在工位：

```text
CHECK_PHONE
↓
WALKING 回 Backend workstation
↓
SIT
↓
COMPUTER ON
↓
OpenClaw/Codex Run
```

Codex 在王浩自己的 Sandbox + Git Worktree 修改后端。

### Step 4

Backend 完成后：

```text
company_update_task
company_publish_artifact(commit/diff)
company_send_message(to=Frontend)
```

告诉 Frontend：

> API 已完成，可以开始联调。

这个对话必须出现在公司消息里。

### Step 5

Frontend 完成后通知 QA。

QA 运行测试。

发现 Bug：

```text
QA → company_send_message → Backend
```

消息：

> remark 超过 255 字符时接口 500，请处理。

Backend 收到并修复。

### Step 6

QA 通过。

Goal 完成。

### 验收

用户必须能够：

- 点击每个人手机看到相关消息。
- 在“内部消息”看到 Backend ↔ Frontend ↔ QA 的全部正式沟通。
- 点击 Task 看到关联 conversation/message/artifact。
- 点击 Artifact 查看 commit/diff/doc。
- 在办公室看到员工真实移动、看手机、回工位、Coding、Testing。
- 浏览器关闭后全部工作继续。

---

# 31. 一个真实的自媒体协作 Demo

F2 动态招聘：

```text
Topic Planner
Writer
Reviewer
Media Supervisor
```

Goal：

> 写一篇今天的 AI 科技新闻稿。

流程：

```text
Topic Planner research
↓
request meeting
↓
所有人真实走到 F2 会议室
↓
Meeting Orchestrator
↓
每个人自己的 OpenClaw Agent 发言
↓
所有会议记录保存到 company_message
↓
形成 Topic Brief Artifact
↓
Writer 回工位写稿
↓
publish Artifact(article-v1.md)
↓
company_send_message → Reviewer
↓
Reviewer 审核
↓
通过 / 退回修改
↓
最终文章 Artifact
```

用户必须能完整查看：

- 选题会记录
- Writer/Reviewer 私聊
- 每个稿件版本
- 当前 Goal/Task

---

# 32. 本阶段实施顺序

不要一次把所有 OpenClaw 功能都接完。

按下面顺序：

```text
P1
OpenClaw Gateway 连接与 Health Check

P2
一个已有员工 ↔ 一个 OpenClaw Agent
能从手机发消息并得到真实 OpenClaw 回复

P3
Employee Runtime Provisioning
录用员工自动创建 OpenClaw Agent + workspace

P4
Agent Sandbox
每员工独立 Docker sandbox

P5
ai-company-bridge Plugin
Agent 能调用 company_send_message / task / artifact

P6
Employee-to-Employee Message Bus
消息持久化 + UI 可见

P7
Meeting Transcript
多个真实 Agent 开会并保存记录

P8
Codex Harness
Backend 员工真正修改自己的 worktree

P9
Artifacts + Git handoff + QA

P10
再考虑 ACP / Claude Code / OpenCode / 更多 MCP 能力
```

---

# 33. 第一阶段最小验收（先把这一条跑通）

请先只绑定两个已有员工：

```text
Product
Backend 王浩
```

两个都是独立 OpenClaw Agent。

测试：

1. 用户点击 Product 手机。
2. 用户说：“让王浩在自己的 workspace 创建 hello-company.md，并写一句项目说明。”
3. Product OpenClaw Agent 理解需求。
4. Product 调用 `company_send_message` 给王浩。
5. AI Company DB 先保存 Product → 王浩消息。
6. 王浩手机出现未读。
7. 王浩人物 CHECK_PHONE。
8. 王浩如果不在工位，真实走回工位。
9. 王浩坐下、电脑亮起。
10. 王浩 OpenClaw/Codex Runtime 在自己的 sandbox/workspace 创建文件。
11. 王浩调用 `company_publish_artifact`。
12. 王浩调用 `company_send_message` 回复 Product：“已完成。”
13. Product 收到回复。
14. 用户可以在“内部消息”看到完整 Product ↔ Backend 对话。
15. 用户点击王浩手机也可以看到同一线程。
16. 文件真实存在于王浩允许的 workspace/project 区域。
17. 王浩不能访问 Product 的 workspace。
18. 关闭浏览器重测，后端仍然继续执行。

只有这个闭环稳定以后，再做多人会议和更多 Harness。

---

# 34. 不要做的事情

本阶段禁止：

- 不要让 OpenClaw 取代 Company DB。
- 不要让每个 Employee 启一个 Gateway。
- 不要把正式员工沟通只存在 OpenClaw transcript。
- 不要让员工直接修改 AI Company SQLite。
- 不要让 Agents 直接拥有 Gateway admin credential。
- 不要给普通员工 docker.sock。
- 不要让所有角色默认 shell/full network/full filesystem。
- 不要把 Codex 做成独立“员工”。
- 不要一次实现 Codex + Claude Code + Gemini + OpenCode 全部 Harness。
- 不要把私有推理过程当聊天记录展示。
- 不要为了接 OpenClaw 推翻现有 Floor/Movement/Meeting/Goal/Task 架构。

---

# 35. 最终产品原则

整个系统最终应该可以这样理解：

```text
Employee
= 公司里的“人”

OpenClaw Agent
= 这个人的长期大脑、记忆和工具入口

Codex / Claude Code / OpenCode
= 这个人的专业工作 Harness

Docker Sandbox
= 这个人的工作电脑

Workspace / Git Worktree
= 这个人的文件和项目工作区

Company Message Bus
= 公司微信/内部通讯

Meeting
= 多个员工正式同步协作

Artifact
= 员工交付的文档、代码、报告

AI Company Backend
= 公司制度、空间、组织和业务事实

PixiJS Office
= 这一切真实状态的可视化世界
```

开发时请先检查现有项目已有的 AgentRuntime、Message、Meeting、Employee、Task、Artifact、WebSocket Event 等代码，做最小增量改造；不要重复创建已有概念。

另外，OpenClaw 仍在快速迭代。请锁定项目使用的 OpenClaw 版本，并以该版本官方 Gateway Protocol / Agent / Sandbox / Codex Harness 文档为准确认 RPC 名称、配置字段和事件名称。业务层只能依赖我们自己的 `OpenClawRuntimeAdapter`，禁止让 OpenClaw 协议字段散落到整个项目。
