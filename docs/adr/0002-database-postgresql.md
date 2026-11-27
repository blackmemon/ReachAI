# 0002 PostgreSQL

Status: ASSUMED. The leads skill names PostgreSQL. The standard says the database choice is always asked.

PostgreSQL is used: row level security gives tenant isolation inside the database, and the job queue and the append-only log use it. If the owner wants SQLite, the isolation tests and the migrations must be redone. See the open items in `docs/requirements.md`.
