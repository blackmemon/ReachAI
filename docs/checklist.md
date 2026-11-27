# Checklist before shipping

A box is ticked only with evidence. "Not run" means it is not done.

## Intake and decisions
- [ ] Database choice asked, answered and recorded. PostgreSQL is ASSUMED (ADR 0002). Not asked.
- [ ] Company country asked. Not asked; Pakistan assumed.
- [x] Design review lists laws, retention risk and the 24 hour backup gap (`docs/requirements.md`).
- [x] Review ended with the two lists in Roman Urdu (`docs/requirements.md`, Open items). They are unanswered.
- [x] Admin panel look recorded (ADR 0009, Look A).
- [x] PROPOSED items recorded: hyper (ADR 0003), Iced renderer (ADR 0004), libraries (ADR 0008).

## Technology rules
- [x] `scripts/check_deps.sh` passes (run 2026-11-21).
- [x] No hand-written HTML, CSS or JavaScript in the repository (same script). Only generated loader and generated glue.
- [ ] Every UI part lives once in `frontend/src/components` with all states and a keyboard path. Token tests pass (28 frontend tests). Keyboard path and all states were not audited.

## Security and data
- [x] Tenant isolation tests pass on the data paths covered (`tests/tests/isolation.rs`, `http_api.rs`). Not every path was audited.
- [x] Hidden buttons also refused by the server (`tests/tests/rights.rs`).
- [x] Session hard limit and idle timeout tested (`tests/tests/sessions.rs`).
- [x] Login CSRF and change CSRF tested (`tests/tests/http_api.rs`).
- [ ] A person's deletion or anonymise request. Not built.
- [ ] Support access limited to one case. Not built.
- [x] CSV import and export: template, all-or-nothing, duplicates, formula safe (backend tests and `outreach.rs`). Update by `row_version` tested. Import through the browser file chooser was not run in a browser.
- [x] Audit log and login history recorded (backend tests).

## Interface
- [x] Public pages (privacy, terms, cookies, sign-in, unsubscribe) built in Iced and open without sign-in (browser test).
- [ ] Admin screens checked at 320, 768, 1024, 1440 px and 200% and 400% zoom. Checked by screenshot at 320, 390, 768, 1024, 1280, 1440 and 1920 px (2026-11-24); zoom at 200% and 400% not checked.
- [ ] WCAG 2.2 AA checked in real browsers with a screen reader. Not run.

## Delivery and performance
- [ ] Release profile compared (z, s, 3). Only `z` was built.
- [ ] Heavy work in a Web Worker. Not built.
- [ ] Load test for 1 million concurrent users. Not run.

## Operations
- [ ] Backup restore drill. Not run.
- [ ] Rolling release and rollback drill. Not run.
