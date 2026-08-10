# Virtual AI Company

这是一个以办公室为核心的多 Agent 工作系统首轮骨架：Rust/Axum 负责持久化员工与领域事件，React 负责控制台和资料面板，PixiJS 负责办公室与人物的帧级动画。

## 当前已实现

- Rust + Axum + Tokio + SQLx + SQLite WAL 后端。
- `/api/health`、`/api/bootstrap`、`/api/events`、`/ws`。
- 首次启动自动创建公司、7 名默认员工和稳定的 `appearance.seed`。
- 数据驱动的 1600 × 900 办公室：开放办公区、会议室、面试室、HR、前台、茶水间、卫生间。
- PixiJS 8 程序化人物与家具绘制；人物拥有不同发型、服装、肤色、配饰和身高比例。
- 员工点击选中、右侧资料卡、技能/性格/当前状态、定位镜头、滚轮缩放和拖动画布。
- Rust Mock Event：Backend 收到消息并看手机 → 思考 → 走进会议室 → 会议结束 → 返回工位 → coding；每个关键变化写入 `events` 并通过 WebSocket 广播。
- Mock 模式不需要任何 LLM API Key。

## 启动

需要 Node.js 20+、npm、Rust stable 和 Cargo。

终端 1：

```powershell
cd apps/server
cargo run
```

终端 2：

```powershell
cd apps/web
npm install
npm run dev
```

打开 <http://localhost:5173>。如果从仓库根目录运行前端检查，也可以使用：

```powershell
npm install
npm run typecheck
npm run build
npm run test
```

服务器默认监听 `0.0.0.0:3001`，这样可以从 Windows 浏览器访问 WSL 内的服务；SQLite 文件默认在 `apps/server/data/ai_company.db`。可通过环境变量覆盖：

```text
HOST=0.0.0.0
PORT=3001
DATABASE_URL=sqlite://data/ai_company.db?mode=rwc
LLM_ENABLED=false
```

## 验证

```powershell
cd apps/web
npm run typecheck
npm run build
npm run test

cd ../server
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## 下一阶段

招聘/面试 Mock 流程还未接入。下一轮应先新增招聘状态机和 `hiring_requests`、`candidates`、`interviews` 表，再让 HR 从 CEO Console 的招聘意图生成 4 个虚构候选人，并复用办公室的前台/面试室/事件协议。面试评分必须只使用技能、经验、沟通、任务匹配和工作方式证据，不能把年龄、性别或外观放进评分公式；候选人录用后还要把既有 `appearance.seed` 转成持久化 employee ID。
