# Honest status

Date of this check: 2026-11-22. Only what was run is marked as checked.

## Run and passed
- `cargo test --workspace`: 153 tests passed, 0 failed (auth 7, backend 56, configs 1, frontend 28, gateway 6, query_engine 17, shared 12, and 26 integration tests against a real PostgreSQL: HTTP API 6, tenant isolation 3, outreach 6, rights 6, sessions 4, worker flow 1).
- `tests/e2e/browser_flow.py` in headless Chrome against the running gateway and the built WASM bundles: 11 of 11 steps passed (public page draws, a real unsubscribe link changes the database, wrong password refused, sign-in, session cookie, 26 pages open with no script error, empty form saves nothing, a new person reaches the database, sign-out ends the session, phone width signs in). Run output: `docs/e2e-run.txt`.
- `scripts/check_deps.sh`: no forbidden crate, no hand-written HTML, CSS or JavaScript, no Docker file.
- Bugs that the browser test found and that are fixed and covered by a test: the Today page returned an error (a wrong column name in a swallowed query); a form with blank boxes wrote NULL into columns that cannot hold it; unsubscribe links lost their dots in the address.

## Built but not proven
- 1 million concurrent users: this is a target. No load test was run. Nothing was measured beyond a single local server.
- Cold load size: each bundle is about 4.2 MB raw and 1.24 MB with Brotli. Start time on a real phone or slow network was not measured. The public bundle is not smaller than the app bundle (ADR 0005).
- WCAG 2.2 AA: colours were tested for contrast and control sizes for 40 px. No screen reader test was done. A canvas screen has no real text for a screen reader or a search engine. This is a known limit of the Iced canvas approach.
- Responsive: checked at 1280 and 390 px wide in headless Chrome. Not checked at 320, 768, 1024, 1440 px, or at 200% and 400% zoom.
- Only Chrome was used. Firefox and Safari were not tried.

## Not built
- Live Gmail SMTP and IMAP (mail is a recording test double), live OpenStreetMap and Overture collection (separate from Claude CLI web research), live DNS, SPF, DKIM and DMARC checks.
- The Claude Desktop scheduled task. The MCP server and its six tools exist and are tested over HTTP; Claude Desktop was not connected.
- Web Worker for heavy work (nothing here is heavy), CDN, backups and restore drill, rolling release, systemd unit was written but not run (`deploy/`).
- Person-level deletion or anonymise (the owner rule is that nothing is deleted; the do not contact list is used instead).
- Firefox and Safari tests, the screen reader test.
- The legal pages are plain drafts. A lawyer must check them.

## Needs your decision
See "Open items" in `docs/requirements.md`.

## Update: ReachAI rename, new colour, logo, filters (earlier check)
- Product name is now blackmemon ReachAI in every screen, page title, legal page and document. The program files keep their old names (`blackmemon-leads`, `blackmemon-worker`) so the service files and scripts still work. The first database migration comment still says the old name, because changing an applied migration would break its checksum.
- Primary colour is #36454F. Soft tints, borders and the page background were re-derived. The WCAG text contrast test still passes.
- The logo is `frontend/assets/logo.svg`, drawn with the Iced svg widget in the top bar and the public frame. The signed-in bar shows the logo plus "ReachAI".
- Every list page now has filters for each listed column: pick lists for choices, Any/Yes/No for yes-no columns, and text filters, plus Clear filters. Number and date columns can be filtered by the server but have no box in the screen yet.
- Tables fill the card on wide screens and scroll sideways only when the window is too narrow.
- Checked: cargo test --workspace passes, browser flow 11 of 11 in Chrome at 1280 and 390 wide, 26 pages screenshotted at 1440 wide. Not checked: other browsers, screen readers, a load test.
- The local test database had to be rebuilt in this session; no data from the earlier version was kept.

## Update: CRM fields, keyboard handling, and idle work
- The People and Companies screens show only their requested CRM fields. Workflow and operational fields remain available internally and are hidden from these screens.
- Email addresses, phone numbers, links, industries, and keywords accept one value per line. The first person email remains the outreach address; the first company link and industry continue to populate existing operational fields.
- Filters are generated for every listed, filterable field. Text/list fields use contains; employee count uses numeric comparisons.
- The app no longer polls every 300 ms for browser history changes; it listens for browser `popstate`. Tab and Shift+Tab move focus through controls, and relevant forms support Enter submission.
- The input widgets already handle standard copy/paste/select-all shortcuts while focused. Browser-level verification of keyboard behavior on the updated signed-in app has not yet been run.
- `cargo test -p query_engine -p frontend -p backend --lib`: 105 tests passed. App and public WASM `cargo check` passed. The full workspace formatting check still reports pre-existing formatting differences.
- Production bundles built successfully with `wasm-opt -Oz` (Binaryen 132): public WASM 4,238,135 bytes / 1,255,050 Brotli; app WASM 4,252,031 bytes / 1,258,141 Brotli. Browser startup time was not measured.
- The WASM build script's missing-optimizer check runs before it clears the output directory.
- The updated signed-in UI has not been profiled in an authenticated browser, so smoothness and comparative rendering performance remain unverified. A prior public-page baseline is not a measurement of this update.
- The integration test requiring PostgreSQL could not be run because the local `leads_test` database is unavailable.

## Update: Email, Chats, Quick, and Research
- The missing `display_name` and `emails` SQL columns are introduced by migration `0003_crm_fields.sql`; the registry expressions correctly target those CRM columns. Rebuild the gateway and run its `migrate` command against the app database before serving the updated bundles. Migration `0004_email_chat_research.sql` adds per-user read/archive state, shared labels, group participants, grouped outgoing messages, and a competitor marker.
- Email now has All/Read/Unread filters, compose-to-existing-contact that queues after send checks, and tenant-shared labels. Chats have All/Group/Archive/Unread filters, group conversations, per-user read/archive state, and labels.
- Quick has AI Chat, AI Usage, and History tabs. Its assistant remains rules-based and defers larger requests to the Claude Desktop queue; the usage view combines saved Quick questions, task counts, and logged tool calls.
- Research is labeled AI Leads and has AI Leads, Competitors, and Data Collections tabs. Its chats invoke the locally installed Claude CLI with public WebSearch/WebFetch tools only; new lead candidates wait for review, competitor findings are marked in Companies, and Data Collections switches between the existing sources and collector runs. Claude Code must be installed and signed in for the gateway's operating-system user. Live end-to-end search was not run during this code change.
- `cargo test -p backend -p gateway -p query_engine` passed (89 tests); `cargo test -p frontend` passed (34 tests). The signed-in and public frontends both pass WASM-target checks.
- `scripts/build_wasm.sh` built both optimized bundles with Binaryen 126: public WASM 4,278,220 bytes / 1,263,334 Brotli; app WASM 4,288,117 bytes / 1,265,063 Brotli. Output was temporary and has been removed; the checked-in/deployed `dist/` was not replaced.
- PostgreSQL-backed migration/API tests and browser visual checks have not yet been run, and migration `0004` has not been applied to the app database. Compose queues mail only; delivery still requires a live mail transport, which is not implemented.

## Update: CRM filter selection and communication layouts
- People and Companies no longer show Email, Phone, Links, or employee-count filters. Job Title, Company, Location, and Keywords accept multiple entered values; a record can match any selected value for one field, while different fields still combine.
- Email and Conversations now use split-pane inbox/chat views; Conversation threads load messages with tenant and employee-owner scoping. Quick AI uses the centered welcome/prompt and bottom composer style from the references.
- Checked: focused tests for query_engine, frontend, and backend passed; both frontend WASM feature checks, `cargo check -p gateway`, and the production gateway/worker plus optimized WASM builds passed. Browser visual verification and database-backed thread testing remain pending.

## Update: performance, Quick freeze, responsive layout, research workspace (2026-11-24)

### What was wrong and what changed
- **The whole app felt laggy: every hover repainted the whole window.** A renderer patch redrew the full 1280x800 canvas on every frame, and hover causes a frame. Measured in headless Chrome: 230 to 400 ms per mouse-move frame, even on the empty Help page. Now partial redraw is back with the real cause of its old artifact fixed (shadows drawn without a clip). Mouse-move frames: median 17 ms. Scrolling a table: 2 to 3 s frames before, median 17 ms after. Details in `docs/adr/0004-...`.
- **The bundle was built for size.** The drawing crates are now built with `opt-level = 3` (Brotli bundle 1.27 MB to 1.40 MB).
- **`/quick` froze the app.** A `Fill`-height box inside a scrollable became infinite, the renderer panicked on the path and the WASM app was dead. Fixed in the Quick screen and made safe in the renderer.
- **Database server.** The PostgreSQL that was running was 16.13 while the installed packages are 18.3 (the system was updated while it ran): `pg_trgm` and `plpgsql` could not load, so inserts into `contacts` failed. The data was dumped (`~/pgdump16-all-20261124.sql`), the old directory kept as `~/pgdata16-backup`, and a new 18.3 cluster was created at `~/pgdata` on port 5433 and restored. If something looks wrong, the old directory is untouched.
- **Migrations 0003 and 0004 had not been applied** to `leads_dev` (the "column m.emails does not exist" errors). `database/build.rs` now rebuilds when a migration file is added; before, a new file was silently left out of the binary.
- **List indexes** (migration 0006): the default list order had no index, so every page sorted the whole company. At 30,000 people: page 1 from 61 ms to 20 ms, last page from 36 to 8 ms.
- Enrichment returned HTTP 400 (its preset filter used a field that was no longer filterable). Fixed.
- The multi-value filter's "Add" button overlapped its box. Fixed.

### Responsive
- A root scale follows the browser width, like a root font size: 1 on laptops, down to 0.82 on very narrow phones, up to 1.3 on large monitors. Everything (text, spacing, controls) scales with it.
- Fixed pixel heights and widths of panes were replaced by shares of the window (Quick, Email, Conversations, the research workspace, the drawer, report and usage columns). Side-by-side panes stack when the page itself is narrower than 760 px, not only on phones. The top bar drops its search box below 980 px; page titles stack above their buttons below 1100 px.
- SVG icons were misplaced at any scale other than 1; fixed in the renderer.
- Checked by screenshot at 320, 390, 768, 1024, 1280, 1440 and 1920 px wide: no sideways page overflow; wide tables scroll inside their card. Not checked: 200% and 400% browser zoom, other browsers.

### Sample data
`scripts/sample_data.sql` adds 160 companies, 320 people, 80 deals, 106 tasks and 32 conversations, all made up, only to a separate company, "ReachAI Sample Data" (sign in with the account created for it). It cannot touch the real company. `scripts/sample_data_remove.sql` takes it out. Search, sort, filters, paging, the edit drawer and "New contact" were driven in the browser on this data and checked in the database. At 30,000 people (temporary, since removed) search takes about 120 ms.

### Research workspace (AI Leads and Competitors)
Described in `docs/adr/0011-research-conversations.md`. Tested with a stand-in CLI (4 new integration tests against PostgreSQL) and with the **real** Claude CLI: leads in Brooklyn (4 results in 94 s, then a follow-up that skipped them and added 2 more), competitors of a Greenpoint pizzeria (3 results with pricing and review details). The real runs reported what they could not find (no public emails, one site returned HTTP 403) and ignored an instruction hidden in a search result.

### Checked
- `cargo test --workspace` (needs `CARGO_TARGET_DIR` set to the absolute `target` path): all pass, including the PostgreSQL tests that could not run before.
- All 26 pages opened one after another inside the app at 1280 and 390 px: no HTTP error, no console error, no ERROR in the gateway log.
- Browser tests used headless Chromium driven by Playwright, not the browser inside VS Code (no tool for it was available).

### Known problems found and not fixed
- **Keyboard shortcuts in text boxes do not work in the browser.** Ctrl+A types an "a"; Ctrl+Backspace deletes one letter; Shift+arrows do not select. The browser sends the right key events, so the loss is between winit (web) and the Iced text box, which never sees the held modifier keys. Not caused by this session's changes. A one-line change inside `iced_widget` did not help, so it was reverted; the cause needs a debug build with logging.
- Search over 8 columns uses `ILIKE`; at 30,000 people it is about 120 ms, and it will grow with the table. A combined trigram index would fix it if needed.
- The `tests/e2e/browser_flow.py` script clicks screen positions of the old layout and needs updating before it is run again.
- Live Gmail, live map collection and the Claude Desktop task are still not built.
