# CI Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the repo a real CI gate (fmt/clippy/test) that a PR cannot bypass, make the production Docker build/deploy depend on that gate, keep dependencies patched (cargo audit + Dependabot), and drop the abandoned pre-commit dependency.

**Architecture:** A new reusable `ci.yml` workflow runs three parallel jobs (`fmt`, `clippy`, `test`) on every PR and on push to `main`, using `Swatinem/rust-cache` to cache `~/.cargo` and `target/`. `ci.yml` declares `workflow_call` as an additional trigger so `build_docker_image.yml` can invoke it as a job and make the build/push/deploy job `needs:` that job — a red CI run stops the image from ever being built, so the deploy webhook is never called. `rust-clippy.yml` (SARIF-to-code-scanning) is left untouched: it's a reporting workflow, not a gate, and now duplicates `ci.yml`'s clippy job only in the sense that both run clippy — that's an acceptable, intentional overlap (one gates, one reports to the Security tab). A separate scheduled `cargo-audit.yml` workflow and a new Dependabot `cargo` ecosystem block cover dependency vulnerabilities and staleness. `.pre-commit-config.yaml` moves from the abandoned `doublify/pre-commit-rust` repo to `repo: local` hooks that just shell out to `cargo fmt`/`cargo clippy` — no external dependency to go stale again.

**Tech Stack:** GitHub Actions (`dtolnay/rust-toolchain`, `Swatinem/rust-cache@v2`, `rustsec/audit-check@v2`, `docker/build-push-action@v7` GHA cache), pre-commit (`repo: local`).

## Global Constraints

- `cargo fmt --check` and `cargo clippy --workspace --all-targets` already pass with zero findings on `main` (verified locally) — the gate goes in as-is, no code cleanup needed.
- Rust edition is 2021 across both workspace crates (`book_bot`, `book_bot_macros`); no `rust-toolchain.toml` is pinned, so `dtolnay/rust-toolchain@stable` tracks whatever `stable` is at CI runtime — do not introduce a pinned toolchain as part of this plan (out of scope).
- Keep `rust-clippy.yml` and its `continue-on-error: true` untouched — it's a SARIF/code-scanning report, not a merge gate; do not fold it into `ci.yml`.
- Default branch is `main`; there is no separate `develop`/`release` branch to account for.

---

## Post-merge manual step (cannot be scripted into this PR)

GitHub only lets you mark a status check "required" in branch protection **after** it has run at least once on the repo. That means: merge Task 1 first (or open the PR and let `ci.yml` run on it), then an admin runs:

```bash
gh api repos/Kurbezz/book_bot/branches/main/protection \
  --method PUT \
  -f required_status_checks[strict]=true \
  -f 'required_status_checks[checks][][context]=fmt' \
  -f 'required_status_checks[checks][][context]=clippy' \
  -f 'required_status_checks[checks][][context]=test' \
  -F enforce_admins=true \
  -F required_pull_request_reviews='null' \
  -F restrictions='null'
```

This is a repo-settings change (shared, moderately hard to reverse) — do not run it as part of automated task execution. Surface it to the human at the end of the plan instead.

---

## File Structure

- `.github/workflows/ci.yml` (new) — the fmt/clippy/test gate, callable both directly (PR/push) and via `workflow_call` (from `build_docker_image.yml`).
- `.github/workflows/build_docker_image.yml` (modified) — adds a `ci` job that calls `ci.yml`, makes `Build-Docker-Image` depend on it, adds GHA layer caching to `docker/build-push-action`.
- `.github/workflows/cargo-audit.yml` (new) — weekly `rustsec/audit-check`.
- `.github/dependabot.yml` (modified) — adds the `cargo` ecosystem block.
- `.pre-commit-config.yaml` (modified) — `repo: local` hooks instead of `doublify/pre-commit-rust`.

## Task 1: Add the `ci.yml` gate workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: a workflow with `on.workflow_call` so Task 2 can do `uses: ./.github/workflows/ci.yml`. Job names `fmt`, `clippy`, `test` — Task 2's `needs:` and the post-merge branch-protection step both reference these exact names.

- [ ] **Step 1: Create `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  pull_request:
    branches: ["main"]
  push:
    branches: ["main"]
  workflow_call:

jobs:
  fmt:
    name: fmt
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v7

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: cargo fmt --check
        run: cargo fmt --all --check

  clippy:
    name: clippy
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v7

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: cargo clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    name: test
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v7

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: cargo test
        run: cargo test --workspace
```

- [ ] **Step 2: Validate the YAML parses**

Run: `ruby -ryaml -e "YAML.load_file('.github/workflows/ci.yml'); puts 'ok'"`
Expected: `ok`

- [ ] **Step 3: Run the same checks locally to confirm the workflow's commands are correct**

Run:
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: all three exit 0 (fmt and clippy are already known-clean per Global Constraints; this step confirms `cargo test --workspace` also passes before wiring it into CI).

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add fmt/clippy/test gate workflow"
```

## Task 2: Gate the Docker build/deploy on CI, add GHA layer cache

**Files:**
- Modify: `.github/workflows/build_docker_image.yml`

**Interfaces:**
- Consumes: `.github/workflows/ci.yml`'s `workflow_call` trigger and its three job names (`fmt`, `clippy`, `test`) from Task 1 — this task references the workflow file, not the individual job names, since a reusable-workflow call collapses to one caller-side job (`ci`).

- [ ] **Step 1: Rewrite `.github/workflows/build_docker_image.yml`**

```yaml
name: Build docker image

on:
  push:
    branches:
      - "main"

jobs:
  ci:
    uses: ./.github/workflows/ci.yml

  Build-Docker-Image:
    needs: ci
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v7

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v4

      - id: repository_name
        uses: ASzc/change-string-case-action@v8
        with:
          string: ${{ github.repository }}

      - name: Login to ghcr.io
        uses: docker/login-action@v4
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Build and push
        id: docker_build
        uses: docker/build-push-action@v7
        env:
          IMAGE: ${{ steps.repository_name.outputs.lowercase }}
        with:
          push: true
          platforms: linux/amd64
          tags: ghcr.io/${{ env.IMAGE }}:latest,ghcr.io/${{ env.IMAGE }}:${{ github.sha }}
          context: .
          file: ./docker/build.dockerfile
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Invoke deployment hook
        uses: joelwmale/webhook-action@master
        with:
          url: ${{ secrets.WEBHOOK_URL }}?BOOK_BOTS_TAG=${{ github.sha }}
```

- [ ] **Step 2: Validate the YAML parses**

Run: `ruby -ryaml -e "YAML.load_file('.github/workflows/build_docker_image.yml'); puts 'ok'"`
Expected: `ok`

- [ ] **Step 3: Diff-check that only the intended lines changed**

Run: `git diff .github/workflows/build_docker_image.yml`
Expected: the diff adds the `ci:` job, `needs: ci` on `Build-Docker-Image`, and the two `cache-from`/`cache-to` lines on the `docker_build` step — nothing else moves.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/build_docker_image.yml
git commit -m "ci: gate docker build/deploy on CI, add GHA layer cache"
```

## Task 3: Weekly `cargo audit` workflow + Dependabot cargo updates

**Files:**
- Create: `.github/workflows/cargo-audit.yml`
- Modify: `.github/dependabot.yml`

**Interfaces:**
- None — this task is independent of Tasks 1–2.

- [ ] **Step 1: Create `.github/workflows/cargo-audit.yml`**

```yaml
name: Cargo audit

on:
  schedule:
    - cron: "0 6 * * 1"
  workflow_dispatch:

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v7

      - name: Audit dependencies
        uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
```

- [ ] **Step 2: Update `.github/dependabot.yml` to also watch cargo**

Full resulting file:

```yaml
version: 2
updates:
  # Maintain dependencies for GitHub Actions
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "daily"

  # Maintain cargo dependencies
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
```

- [ ] **Step 3: Validate both files parse**

Run:
```bash
ruby -ryaml -e "YAML.load_file('.github/workflows/cargo-audit.yml'); puts 'ok'"
ruby -ryaml -e "YAML.load_file('.github/dependabot.yml'); puts 'ok'"
```
Expected: `ok` printed twice.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/cargo-audit.yml .github/dependabot.yml
git commit -m "ci: add weekly cargo audit workflow and dependabot cargo updates"
```

## Task 4: Migrate pre-commit off the abandoned `doublify/pre-commit-rust` repo

**Files:**
- Modify: `.pre-commit-config.yaml`

**Interfaces:**
- None — independent of Tasks 1–3.

- [ ] **Step 1: Rewrite `.pre-commit-config.yaml`**

```yaml
repos:
  - repo: local
    hooks:
      - id: fmt
        name: cargo fmt
        entry: cargo fmt --all --check
        language: system
        types: [rust]
        pass_filenames: false
      - id: clippy
        name: cargo clippy
        entry: cargo clippy --workspace --all-targets -- -D warnings
        language: system
        types: [rust]
        pass_filenames: false
```

- [ ] **Step 2: Validate the YAML parses**

Run: `ruby -ryaml -e "YAML.load_file('.pre-commit-config.yaml'); puts 'ok'"`
Expected: `ok`

- [ ] **Step 3: Run the hooks locally with pre-commit (if installed) to confirm they execute and pass**

Run: `pre-commit run --all-files`
Expected: both `cargo fmt` and `cargo clippy` hooks report `Passed`. If `pre-commit` is not installed locally, instead run the two `entry:` commands directly and confirm both exit 0 (already confirmed in Task 1 Step 3):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add .pre-commit-config.yaml
git commit -m "chore: migrate pre-commit to local cargo hooks, drop abandoned doublify/pre-commit-rust"
```

---

## Self-Review Notes

- **Spec coverage:** Problem 1 (no gate) → Task 1. Problem 2 (unchecked deploy) → Task 2. Problem 3 (Dependabot cargo) → Task 3. Problem 4 (abandoned pre-commit repo) → Task 4. Problem 5 (no Docker layer cache) → Task 2 Step 1 (`cache-from`/`cache-to`). Acceptance criterion "PR with failing check can't merge" → covered by Task 1's jobs plus the documented post-merge branch-protection step (can't be done before the checks exist). Acceptance criterion "cargo audit runs" is covered by Task 3; the spec's `rustsec/audit-check` and `Swatinem/rust-cache` versions were confirmed against the actions' GitHub tags (`audit-check` latest `v2.0.0`, `rust-cache` latest `v2.9.1`) — pinning to the `v2`/`v1`-style major tags used elsewhere in this repo's existing workflows (e.g. `actions/checkout@v7`, `docker/login-action@v4`).
- **Placeholder scan:** none found — every step has literal file contents and literal shell commands.
- **Type/name consistency:** Task 2's `needs: ci` matches the job name `ci:` defined in the same file (Step 1), which in turn is the job that invokes `ci.yml` from Task 1 — no mismatch between `ci` (the calling job) and `fmt`/`clippy`/`test` (the reusable workflow's internal jobs, referenced only in the post-merge branch-protection command).
