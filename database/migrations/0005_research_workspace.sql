-- AI research workspace: chat sessions, their runs, live progress events and structured results.
-- Plain SQL on purpose (no plpgsql needed). Sessions belong to one person; every table is also
-- fenced by company (tenant) with row level security like the rest.

CREATE TABLE research_sessions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    user_id uuid NOT NULL REFERENCES users(id),
    mode text NOT NULL CHECK (mode IN ('leads', 'competitors')),
    title text NOT NULL DEFAULT '' CHECK (length(title) <= 200),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX research_sessions_owner ON research_sessions (tenant_id, user_id, mode, updated_at DESC);

CREATE TABLE research_messages (
    id bigserial PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    session_id uuid NOT NULL REFERENCES research_sessions(id) ON DELETE CASCADE,
    role text NOT NULL CHECK (role IN ('user', 'assistant')),
    body text NOT NULL CHECK (length(body) <= 12000),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX research_messages_session ON research_messages (session_id, id);

CREATE TABLE research_session_runs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    session_id uuid NOT NULL REFERENCES research_sessions(id) ON DELETE CASCADE,
    status text NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'done', 'failed', 'cancelled')),
    request text NOT NULL DEFAULT '',
    step text NOT NULL DEFAULT '',
    error text NOT NULL DEFAULT '',
    found integer NOT NULL DEFAULT 0,
    duplicates integer NOT NULL DEFAULT 0,
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz
);
CREATE INDEX research_session_runs_session ON research_session_runs (session_id, started_at DESC);
-- Only one run at a time per session.
CREATE UNIQUE INDEX research_session_runs_one_active ON research_session_runs (session_id) WHERE status = 'running';

CREATE TABLE research_events (
    id bigserial PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    run_id uuid NOT NULL REFERENCES research_session_runs(id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('step', 'site', 'lead', 'note', 'error')),
    body text NOT NULL CHECK (length(body) <= 600),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX research_events_run ON research_events (run_id, id);

CREATE TABLE research_results (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    session_id uuid NOT NULL REFERENCES research_sessions(id) ON DELETE CASCADE,
    run_id uuid REFERENCES research_session_runs(id) ON DELETE SET NULL,
    name text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 200),
    category text NOT NULL DEFAULT '',
    website text NOT NULL DEFAULT '',
    address text NOT NULL DEFAULT '',
    location text NOT NULL DEFAULT '',
    phone text NOT NULL DEFAULT '',
    email text NOT NULL DEFAULT '',
    socials text NOT NULL DEFAULT '',
    source_url text NOT NULL DEFAULT '',
    verification text NOT NULL DEFAULT 'unverified' CHECK (verification IN ('verified', 'partial', 'unverified')),
    notes text NOT NULL DEFAULT '',
    services text NOT NULL DEFAULT '',
    pricing text NOT NULL DEFAULT '',
    reviews text NOT NULL DEFAULT '',
    presence text NOT NULL DEFAULT '',
    offerings text NOT NULL DEFAULT '',
    dedupe_key text NOT NULL,
    saved_as text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (session_id, dedupe_key)
);
CREATE INDEX research_results_session ON research_results (session_id, created_at);

ALTER TABLE research_sessions ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON research_sessions USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
ALTER TABLE research_messages ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON research_messages USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
ALTER TABLE research_session_runs ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON research_session_runs USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
ALTER TABLE research_events ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON research_events USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
ALTER TABLE research_results ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON research_results USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
