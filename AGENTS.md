# AGENTS.md

## Project mission

Build the Virtual AI Company described in `docs/AI_COMPANY_PRODUCT_SPEC.md`.
This is a real multi-agent work system with an office visualization, not a conventional admin dashboard and not a decorative game skin.

## Required stack for V1

- Web: React + TypeScript + Vite + PixiJS 8 + Zustand + native WebSocket.
- Server: Rust + Axum + Tokio + Serde + SQLx + SQLite WAL.
- Keep V1 lightweight. Do not add Redis, Kafka, Kubernetes, a physics engine, or microservices without a demonstrated need.

## Working rules

- Read `docs/AI_COMPANY_PRODUCT_SPEC.md` before implementation work.
- Implement in phases. Do not jump to real coding-agent shell execution before the office, events, hiring, and interview mock flow works.
- Prefer actual code changes over planning-only responses.
- After meaningful changes, run the relevant formatter, typecheck, build, lint/clippy, and tests; fix failures before stopping.
- Do not store secrets in the repo.
- Keep Mock LLM mode fully runnable without API keys.
- Important UI behavior must be driven by domain events; do not fake backend state in the canvas.
- Never update React state every animation frame. Pixi owns frame-level actor animation.
- Keep agent movement/ambient behavior deterministic or rule-based; do not spend LLM tokens on walking, coffee, restroom, sitting, or idle animation.
- Persist employee identity and `appearance.seed`; people must look the same after reload.
- Hiring score must not use age, gender, or appearance.
- Use simple maintainable solutions when a small detail is unspecified; continue without blocking for confirmation.

## V1 verification commands

Web (adapt to package scripts that exist):

```bash
npm run typecheck
npm run build
npm run test
```

Server:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## First implementation task

Start with Phase 1 + Phase 2 + the minimum Phase 3 described in the product spec. The first visible milestone must show a procedurally rendered office with seven persistent default employees, distinct appearances, selectable profiles, desks/meeting/interview/HR/reception/pantry/restroom areas, plus a mock event that makes one employee check their phone, walk to a meeting room, and return to their desk to code.
