# Virtual AI Company

这是一个以办公室为可视化客户端的持久化多 Agent 工作系统。第二阶段开始后，React/Pixi 只负责读取 Snapshot 和播放后端事件；Rust Worker、SQLite 中的消息、Job、员工状态和事件不会因为浏览器关闭而停止。

## 当前阶段能力

- Rust + Axum + Tokio + SQLx + SQLite WAL，服务启动时独立启动 Agent Worker。
- `GET /api/company/snapshot` 一次恢复公司、员工、Goal、Task、Messages 未读数、Jobs、Provider 和办公室状态；页面随后才建立 WebSocket 增量事件流。
- Model Provider 管理：支持多个 Provider、`mock` 和 `openai_compatible` 协议、Base URL、模型、启用状态、连接测试。
- API Key 只在写入 Provider 时从浏览器发送到 Rust 后端，并以 AES-256-GCM 加密保存；列表、员工资料和 Snapshot 永远不返回完整 Key。
- Agent 通过 `model_profile_id` 绑定 Provider。默认员工绑定本地 Mock Provider，配置真实 Provider 后可在“大模型”页面逐个绑定。
- OpenAI Compatible 回复中的 `input/output/cache/total` Token usage 会按 LLM Call、员工和公司持久化统计；右侧员工列表实时显示个人用量，顶部显示公司累计用量。Mock Provider 不伪造 Token，统计为 0。
- `agent_jobs` 支持 `PENDING`、`RUNNING`、`WAITING`、`COMPLETED`、`FAILED`、`CANCELLED` 数据状态；Rust Worker 会把重启前的 `RUNNING` Job 恢复为 `PENDING`。
- 点击办公室人物打开持久化手机界面：消息、资料、工作、记忆四个页面。消息由 API 写入数据库并创建 Job，Worker 调用模型，回复写库并广播 WebSocket 事件。
- Agent 状态由后端事件驱动，并拆成独立维度：`activity`（IDLE/THINKING/READING/CODING/MEETING 等）、`movement`（STATIONARY/PREPARING/WALKING/ARRIVED）、`device`、`posture`、`presence` 和 `locationId`；旧 `currentState` 仅作为第一阶段客户端兼容字段。
- 办公室空间模拟：统一 `OfficeLayout.locations`、障碍物和网格配置；Rust 使用 A* 生成路线，`agent_movements` 持久化起点、终点、路径、服务器时间和到达状态，独立 Movement Worker 在没有浏览器时仍继续运行。
- 聊天中的“去洗手间 / 去卫生间 / 去厕所”会由确定性空间指令映射到 `toilet_entrance`，复用统一寻路和持久化 Movement，不调用 LLM 决定坐标。
- PixiJS 不再自行规划路线或报告到达。它只根据 Snapshot 的 `serverTime`、`activeMovements.path`、`startedAt` 和 `estimatedArrivalAt` 做插值，并由 `AnimationController` 播放 idle/walk 视觉动作。
- 会议流程使用同一套移动能力：创建会议后为每位参会者预留独立座位，所有人真实到达并坐下后才进入 `IN_PROGRESS`；结束会议后分别沿路线返回各自工位。
- 本阶段不包含招聘、面试和真实 Coding Tool，也不提供脱离后端状态的演示操作；Mock Provider 只作为无 Key 的本地运行兜底。

## 启动

需要 Node.js 20+、npm、Rust stable 和 Cargo。

在仓库根目录创建本地配置（`.env` 已被 Git 忽略，不要提交真实密钥）：

```bash
cp .env.example .env
# 编辑 .env，将 MODEL_PROFILE_ENCRYPTION_KEY 改成自己的随机长字符串
```

Rust 后端会自动读取根目录 `.env`；如果同名变量已经在 WSL Shell 中导出，则以 Shell 环境变量为准。然后启动后端：

```bash
cd apps/server
export HOST=0.0.0.0
export PORT=3001
cargo run
```

然后在 WSL 或 Windows 终端启动 Vite：

```bash
cd apps/web
npm install
npm run dev -- --host 0.0.0.0
```

Windows 浏览器访问 <http://localhost:5173>。Vite 已代理 `/api` 和 `/ws` 到 WSL 内的 `127.0.0.1:3001`；Rust 默认监听 `0.0.0.0:3001`，因此不需要让浏览器直接访问 Rust 端口。

SQLite 文件默认是 `apps/server/data/ai_company.db`。可覆盖：

```text
HOST=0.0.0.0
PORT=3001
DATABASE_URL=sqlite://data/ai_company.db?mode=rwc
MODEL_PROFILE_ENCRYPTION_KEY=your-local-encryption-secret
```

## 人工验证第二阶段 Demo

1. 打开“大模型”，确认默认 `Mock Office Provider` 存在；新增一个 `OpenAI Compatible` Provider，填写 Base URL、模型和 API Key，点击“测试连接”。
2. 在同页的“Agent 模型绑定”中把一个员工绑定到新 Provider。API Key 输入框不会回显已保存的值。
3. 点击办公室中的该员工，手机默认打开“消息”页，发送一条消息。观察事件和人物状态：`CHECK_PHONE → THINKING → IDLE`；回复来自 Worker 写入的数据库消息。
4. 刷新浏览器，消息、回复、未读数、Job 和当前员工状态从 `/api/company/snapshot` 恢复，而不是依靠 WebSocket 历史回放。
5. 发送另一条消息后立即关闭浏览器。Rust 进程继续消费 Job、调用模型并保存回复；重新打开页面即可看到完成的聊天和“工作”页记录。

如果暂时没有真实 API Key，可以保留 Mock Provider 验证完整的数据库、Worker、状态和刷新流程。

## 人工验证办公室空间移动 Demo

1. 在办公室工具栏点击“召开三人会议”。默认参会者是 Backend、Frontend、QA，分别从 `backend_desk_01`、`frontend_desk_01`、`qa_desk_01` 出发。
2. 观察三名员工的 `AGENT_MOVE_REQUESTED → AGENT_MOVE_STARTED` 事件、不同路径和“移动中”动画。路线会经过 `meeting_room_door`，不会穿过会议桌；此时 Snapshot 的 `activeMovements` 保存完整路径和预计到达时间。
3. 先到达的员工会显示 `WAITING`；最后一名员工到达后才会出现 `MEETING_STARTED`，会议状态才变为 `IN_PROGRESS`。
4. 点击“结束会议并回工位”。员工先起身，再沿各自路线回到原工位，最后收到 `AGENT_ARRIVED` / `AGENT_SAT_DOWN`，状态恢复为 `IDLE`。
5. 断线恢复：会议移动开始后关闭浏览器，等待一段时间再打开；页面先请求 Snapshot，再建立 WebSocket。若移动已完成，员工直接显示在会议座位；若仍在路上，Pixi 按 `serverTime` 从路径中间继续插值，不会从起点重走。

### 从手机聊天触发移动

手机消息会先由 Rust Worker 做确定性的中文空间指令识别，再调用统一的 `MoveAgentCommand`；这些指令不需要 LLM Key，也不会让浏览器直接改坐标：

- 对单个员工发送“去会议室”“去茶水间”“去入口”或“回工位”，会创建持久化移动并返回目标位置。
- 对超级管理员发送“让所有人去会议室”，会创建全员会议，为每个人分配不同座位；所有人真实到达后会议才进入 `IN_PROGRESS`。
- 对超级管理员发送“散会”“结束会议”或“散会，所有人回到工位”，会结束当前活动会议，并为每位参会者创建回自己工位的持久化移动。
- “让所有人去办公室”中的“办公室”指当前开放办公区。员工本来就在各自工位时不会被伪造移动；超级管理员会依次走到各工位当面通知。若要看到所有员工从不同位置走动，请使用“让所有人去会议室”。

也可以直接调用会议 API：

```bash
curl -X POST http://localhost:3001/api/companies/company-01/meetings \
  -H 'content-type: application/json' \
  -d '{"title":"空间移动验收","participantIds":["backend-01","frontend-01","qa-01"],"meetingType":"TEAM"}'
```

通用移动命令 API 是 `POST /api/companies/{companyId}/agents/{agentId}/move`，请求体为 `{"destinationLocationId":"coffee_point_01","reason":"API"}`；会议、Runtime 和未来招聘流程都应复用这条能力，不直接改员工坐标。

## 校验命令

```bash
cd apps/web
npm run typecheck
npm run build
npm run test

cd ../server
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
