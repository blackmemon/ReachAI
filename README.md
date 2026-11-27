# blackmemon ReachAI

AI sales outreach and lead generation web app for Quickanalytics. Rust on the server (Tokio, hyper, PostgreSQL) and Rust with Iced compiled to WebAssembly in the browser. Read `docs/honest-status.md` first: it says what was run and what was not.

## What is inside
| Folder | What |
|---|---|
| `shared/` | Types used by the browser and the server (no logic) |
| `configs/`, `auth/`, `database/`, `query_engine/`, `backend/` | Settings, passwords and sessions, migrations and roles, the one list/search/CSV engine, business rules |
| `gateway/` | The only HTTP entry point (`blackmemon-leads` binary) |
| `worker/` | Background jobs, timers and sending (`blackmemon-worker` binary) |
| `frontend/` | The browser app (Iced, WASM). Two bundles: `public` and `app` |
| `vendor/iced_tiny_skia/` | Patched copy of the renderer (ADR 0004) |
| `tests/` | Integration tests (real PostgreSQL) and the browser test `tests/e2e/browser_flow.py` |
| `docs/` | Requirements, ADRs, honest status, checklist |
| `deploy/` | systemd units and an env example (no Docker) |

## Build tools
Rust 1.99.1 with the `wasm32-unknown-unknown` target, `wasm-bindgen` CLI 0.2.130, `wasm-opt`, `brotli`, PostgreSQL 16.

## Run it on one machine
1. Create the database and the two runtime roles. As a PostgreSQL superuser: create a database and an owner role, then run `database/roles.sql` once for the cluster. Change the role passwords.
2. Set the owner URL for the commands below: `export LEADS_OWNER_URL=postgres://OWNER:PASS@127.0.0.1:5432/leads`
3. Build the browser bundles: `scripts/build_wasm.sh` (writes `dist/`).
4. Build the server: `cargo build --release -p gateway -p worker`
5. Apply the migrations to the same database used by the app: `target/release/blackmemon-leads migrate`. Rebuild the gateway before running this after a code update. Migration `0003_crm_fields.sql` adds the CRM columns used by People and Companies; `0004_email_chat_research.sql` adds mail/chat state, labels, groups and competitor marking; `0005_research_workspace.sql` adds AI research conversations; `0006_list_indexes.sql` adds the list ordering indexes. If the server reports missing `display_name` or `emails` columns, this command has not been run against that database.
6. Create a company and its first admin: `target/release/blackmemon-leads create-tenant "Company" admin@company.com "Admin Name" "a-long-password"`. For demo data instead: `target/release/blackmemon-leads seed` (prints a random admin password once).
7. Start the gateway with the settings in `deploy/env.example` (for a local run over plain http set `LEADS_COOKIE_SECURE=false`): `target/release/blackmemon-leads serve`
8. Start the worker: `target/release/blackmemon-worker`
9. Install and sign in to Claude Code as the same operating-system user that runs the gateway (`claude`), then open `http://127.0.0.1:8000`. Set `CLAUDE_CLI` in the gateway environment if the executable is not on its `PATH`. AI Leads and Competitors use only Claude CLI's public web search/fetch tools; lead results wait for approval, while competitor results are marked in Companies.

Do not run the demo seed in production.

To try lists, search and paging with a larger dataset, create a separate company and load 160 made-up companies and 320 people into it only: `target/release/blackmemon-leads create-tenant "ReachAI Sample Data" sample@sample.example "Sample Admin" "<password>"`, then `psql "$LEADS_OWNER_URL" -v ON_ERROR_STOP=1 -f scripts/sample_data.sql`. `scripts/sample_data_remove.sql` removes it again.

## Tests
- `CARGO_TARGET_DIR=$PWD/target cargo test --workspace` (needs the PostgreSQL test database `leads_test` described in `tests/tests/common/mod.rs`, and `cargo build -p gateway` first)
- `scripts/check_deps.sh` (technology rules)
- `tests/e2e/browser_flow.py` (see the header of the file for the variables it needs; needs Chrome and Playwright for Python)

## Not done
Live Gmail, live OpenStreetMap/Overture collection, the Claude Desktop scheduled task, backups and load tests. See `docs/honest-status.md`.
