# Docker Image Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `docker/build.dockerfile` produce a non-root, healthchecked image built from a minimal build context, with a dependency-cache layer that fails loudly (instead of silently via `|| true`) and covers `build.rs`/new binary targets automatically.

**Architecture:** Replace the dummy-`main.rs` dependency-caching trick with `cargo-chef`'s two-stage `planner`/`builder` split — `cargo chef prepare` fingerprints the full dependency graph (including `build.rs` and any binary target) into `recipe.json`, and `cargo chef cook` builds only those dependencies in a layer that's cached until `Cargo.toml`/`Cargo.lock` change, with dependency-build failures surfacing as normal `cargo` errors instead of being swallowed by `|| true`. The runtime stage adds a non-privileged system user (`useradd -r -s /usr/sbin/nologin app` + `USER app`), a `HEALTHCHECK` against the existing `/health` route (justifying the already-installed `curl`; `jq` is unused and dropped), and calls the binary directly via `CMD` instead of through the one-line `scripts/start.sh`, which is deleted. `.dockerignore` is extended so `.git`, `test_env/` (secrets), `graphify-out/`, `docs/`, `.github/`, `*.md`, `.DS_Store`, and (now-empty) `scripts/` never enter the build context.

**Tech Stack:** Docker multi-stage build, `cargo-chef` (installed via `cargo install --locked` on top of the existing `rust:bookworm` base — no new third-party base image), `debian:bookworm-slim` runtime.

## Global Constraints

- Runtime env vars (`WEBHOOK_PORT`, `TELEGRAM_BOT_API_ROOT`, etc.) are required and un-defaulted (`book_bot/src/config.rs:31-32` panics if absent) — the Dockerfile must not set defaults for them; they're supplied by whoever runs the container.
- The webhook server binds `0.0.0.0:$WEBHOOK_PORT` (`book_bot/src/bots_manager/axum_server.rs:39-41`) and exposes `GET /health` returning `200 OK` (`book_bot/src/bots_manager/axum_server.rs:154`) — `HEALTHCHECK` can reach it via `localhost` from inside the same container.
- The workspace has exactly two members: `book_bot` (the `book_bot` binary) and `book_bot_macros` (a `proc-macro` lib, path-dependency of `book_bot`) — no `build.rs` exists today, but `cargo-chef`'s recipe format covers one if added later, unlike the old dummy-`main.rs` pattern.
- `.github/workflows/build_docker_image.yml` references `file: ./docker/build.dockerfile` and `context: .` — neither path changes in this plan.
- **No Docker daemon is available in this execution environment** (`docker` is not on `PATH`). Steps that need `docker build`/`docker run`/`docker inspect` are written with exact commands but must be run by a human (or CI) with Docker installed — they cannot be checked off by an agent running in this sandbox. Everything that can be verified without Docker (cargo-chef's dependency-cook step, the dep-break-fails-clearly criterion) is verified directly with `cargo`.

---

## File Structure

- `.dockerignore` (modified) — add the missing exclusion patterns.
- `docker/build.dockerfile` (modified) — cargo-chef planner/builder stages, non-root runtime user, `HEALTHCHECK`, drop `jq`, direct `CMD`.
- `scripts/start.sh` (deleted) — replaced by `CMD ["/usr/local/bin/book_bot"]`.
- `scripts/` (deleted) — empty after the above.

## Task 1: Extend `.dockerignore`

**Files:**
- Modify: `.dockerignore`

**Interfaces:**
- None — independent of Tasks 2–3.

- [ ] **Step 1: Rewrite `.dockerignore`**

Full resulting file:

```
target
.git
test_env
graphify-out
docs
.github
.DS_Store
*.md
scripts
```

- [ ] **Step 2: Confirm every required pattern is present**

Run: `for p in target .git test_env graphify-out docs .github .DS_Store '*.md' scripts; do grep -qxF "$p" .dockerignore && echo "OK: $p" || echo "MISSING: $p"; done`
Expected: nine `OK:` lines, no `MISSING:` lines.

- [ ] **Step 3: Commit**

```bash
git add .dockerignore
git commit -m "chore(docker): exclude secrets and dev-only paths from build context"
```

## Task 2: Delete `scripts/start.sh` and the now-empty `scripts/` directory

**Files:**
- Delete: `scripts/start.sh`

**Interfaces:**
- Produces: no more `scripts/` directory for Task 3's Dockerfile to `COPY`. Task 3's `CMD` must be `["/usr/local/bin/book_bot"]`, not `["/start.sh"]`.

- [ ] **Step 1: Delete the file and directory**

Run:
```bash
git rm scripts/start.sh
rmdir scripts
```
Expected: `scripts/start.sh` removed from git; `rmdir` succeeds silently (directory was empty).

- [ ] **Step 2: Confirm removal**

Run: `test -d scripts && echo "STILL EXISTS" || echo "removed"`
Expected: `removed`

- [ ] **Step 3: Commit**

```bash
git commit -m "chore: remove trivial scripts/start.sh, call the binary directly from CMD"
```

## Task 3: Rewrite `docker/build.dockerfile` — cargo-chef, non-root user, healthcheck

**Files:**
- Modify: `docker/build.dockerfile`

**Interfaces:**
- Consumes: `scripts/` no longer exists (Task 2) — this Dockerfile must not `COPY ./scripts /` or reference `/start.sh`.
- Consumes: `.dockerignore` from Task 1 — `COPY . .` in both the `planner` and `builder` stages now excludes `.git`, `test_env`, etc.

- [ ] **Step 1: Rewrite `docker/build.dockerfile`**

Full resulting file:

```dockerfile
FROM rust:bookworm AS chef

RUN cargo install cargo-chef --locked

WORKDIR /app

FROM chef AS planner

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json

# Build only dependencies (this layer is cached unless Cargo.toml/Cargo.lock change).
# Unlike the old dummy-main.rs + `|| true` trick, a broken dependency fails here
# with a normal cargo error, and build.rs / new binary targets are covered
# automatically since the recipe captures the whole dependency graph.
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

RUN cargo build --release --bin book_bot

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y openssl ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /usr/sbin/nologin app

COPY --from=builder /app/target/release/book_bot /usr/local/bin/book_bot

USER app

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:${WEBHOOK_PORT}/health || exit 1

CMD ["/usr/local/bin/book_bot"]
```

- [ ] **Step 2: Sanity-check the cargo-chef flow locally, without Docker**

This validates the dependency-layer logic (recipe generation + cook) directly against the workspace, independent of whether Docker is available.

Run:
```bash
cargo install cargo-chef --locked
cargo chef prepare --recipe-path /tmp/book_bot_recipe.json
cargo chef cook --release --recipe-path /tmp/book_bot_recipe.json
cargo build --release --bin book_bot
```
Expected: all four commands exit 0; `cargo chef cook` prints normal dependency-compilation output (no `|| true`-style suppressed errors are possible here since the command isn't wrapped).

- [ ] **Step 3: Confirm a broken dependency fails at the cook step, not later**

Run:
```bash
cp Cargo.toml /tmp/Cargo.toml.bak
sed -i.bak 's/tokio = { version = "1.44.2"/tokio = { version = "999.0.0"/' book_bot/Cargo.toml
cargo chef prepare --recipe-path /tmp/book_bot_recipe_broken.json
cargo chef cook --release --recipe-path /tmp/book_bot_recipe_broken.json; echo "exit: $?"
```
Expected: `cargo chef cook` fails (non-zero exit) with a clear cargo resolver error naming `tokio` and the unsatisfiable version — this is the failure a broken `Cargo.toml` would hit inside the Docker `builder` stage's `cargo chef cook` layer, before `COPY . .` / the final `cargo build` ever runs.

Then revert:
```bash
mv book_bot/Cargo.toml.bak book_bot/Cargo.toml 2>/dev/null || cp /tmp/Cargo.toml.bak Cargo.toml
git diff --stat Cargo.toml book_bot/Cargo.toml
```
Expected: no diff (both files restored to their committed state).

- [ ] **Step 4: Commit**

```bash
git add docker/build.dockerfile
git commit -m "feat(docker): cargo-chef dep caching, non-root user, healthcheck; drop jq"
```

## Task 4: Docker-level verification (requires a local Docker daemon — run manually, not in this sandbox)

**Files:**
- None (verification only).

**Interfaces:**
- Consumes: the finished image from Tasks 1–3.

- [ ] **Step 1: Build the image and note context size**

Run: `docker build -t book_bot:hardening-test -f docker/build.dockerfile .`
Expected: the first output line (`Sending build context to Docker daemon  N MB` on classic builder, or the `[internal] load build context` transfer size on BuildKit) is small — no multi-megabyte `graphify-out/` or `.git/` history included. Compare against a build from `main` before this change if you want an exact before/after byte count.

- [ ] **Step 2: Confirm the process does not run as root**

Run:
```bash
docker inspect --format='{{.Config.User}}' book_bot:hardening-test
```
Expected: `app` (not empty, not `root`/`0`).

- [ ] **Step 3: Confirm `HEALTHCHECK` is present and turns `healthy`**

Run:
```bash
docker inspect --format='{{json .Config.Healthcheck}}' book_bot:hardening-test
docker run -d --name book_bot_hardening_test \
  -e TELEGRAM_BOT_API_ROOT=https://api.telegram.org \
  -e WEBHOOK_BASE_URL=http://localhost \
  -e WEBHOOK_PORT=8080 \
  -e WEBHOOK_SECRET_TOKEN=test \
  -e MANAGER_URL=http://localhost \
  -e MANAGER_API_KEY=test \
  -e USER_SETTINGS_URL=http://localhost \
  -e USER_SETTINGS_API_KEY=test \
  -p 8080:8080 \
  book_bot:hardening-test
sleep 10
docker inspect --format='{{.State.Health.Status}}' book_bot_hardening_test
docker logs book_bot_hardening_test
docker rm -f book_bot_hardening_test
```
Expected: the first `docker inspect` shows a `Test` array containing `curl -f http://localhost:8080/health`; the second shows `healthy` (add remaining required env vars from `book_bot/src/config.rs` if the container exits before the healthcheck's `--start-period` elapses — check `docker logs` for which `Cannot get the ... env variable` panic fired first).

- [ ] **Step 4: Confirm a broken `Cargo.toml` fails the Docker build at the dependency layer**

Run:
```bash
sed -i.bak 's/tokio = { version = "1.44.2"/tokio = { version = "999.0.0"/' book_bot/Cargo.toml
docker build -t book_bot:broken-dep-test -f docker/build.dockerfile . ; echo "exit: $?"
mv book_bot/Cargo.toml.bak book_bot/Cargo.toml
git diff --stat book_bot/Cargo.toml
```
Expected: the build fails during the `RUN cargo chef cook --release --recipe-path recipe.json` step (visible in the failing step's name in the build log) with a cargo resolver error, not during the final `RUN cargo build --release --bin book_bot` step; `git diff --stat` shows no diff after revert.

- [ ] **Step 5: Confirm the image still starts the bot normally**

Run: `docker run --rm book_bot:hardening-test /usr/local/bin/book_bot --help 2>&1 | head -5` (or run with the full env-var set from Step 3 and confirm it logs `Webhook server listening on port ...` per `book_bot/src/bots_manager/axum_server.rs:167`).
Expected: the binary executes (no `exec format error`, no permission-denied — confirms the non-root `app` user can execute the copied binary).

---

## Self-Review Notes

- **Spec coverage:** Problem 1 (root) → Task 3 Step 1 (`useradd -r -s /usr/sbin/nologin app` + `USER app`), verified in Task 4 Step 2. Problem 2 (`.dockerignore`) → Task 1, verified in Task 4 Step 1 (context size) and by the pattern-presence check in Task 1 Step 2. Problem 3 (no `HEALTHCHECK`, unused `jq`) → Task 3 Step 1, verified in Task 4 Step 3. Problem 4 (fragile `|| true` cache layer) → Task 3 Step 1 (cargo-chef), verified without Docker in Task 3 Steps 2–3 and with Docker in Task 4 Step 4. Problem 5 (`scripts/start.sh`) → Task 2, with the Dockerfile's `CMD` updated in Task 3 Step 1.
- **Placeholder scan:** none — every step has literal file contents or literal commands with stated expected output.
- **Type/name consistency:** the image tag `book_bot:hardening-test` in Task 4 Steps 1–3 matches `book_bot:broken-dep-test` only by deliberate difference (Step 4 uses a separate tag so a failed build doesn't clobber the working image); the container name `book_bot_hardening_test` in Step 3 is created and removed within the same step, not reused elsewhere. `useradd -r -s /usr/sbin/nologin app` (Task 3) and `USER app` / `docker inspect --format='{{.Config.User}}'` expecting `app` (Task 4 Step 2) all reference the same username.
- **Deviation from the spec's literal wording:** the spec says `useradd -r app`; this plan adds `-s /usr/sbin/nologin` (no login shell) as a small additional hardening consistent with the spec's own title ("hardening") — it doesn't change the acceptance criteria (still a non-root named user) and doesn't conflict with anything else in the spec.
