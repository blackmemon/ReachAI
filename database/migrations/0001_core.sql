-- blackmemon Leads: core schema (PostgreSQL). Every tenant-owned row has tenant_id.
-- Times are UTC. Nothing is deleted: status fields are used instead.

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE tenants (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL,
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','suspended')),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    email text NOT NULL,
    name text NOT NULL,
    role text NOT NULL CHECK (role IN ('admin','manager','employee','viewer')),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','suspended')),
    last_sign_in timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, email)
);
CREATE UNIQUE INDEX users_email_unique ON users (lower(email));

-- Credentials live in their own table (separate from profile data).
CREATE TABLE user_credentials (
    user_id uuid PRIMARY KEY REFERENCES users(id),
    password_hash text NOT NULL,
    changed_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id_hash text PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    csrf text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen timestamptz NOT NULL DEFAULT now(),
    hard_expires_at timestamptz NOT NULL,
    revoked_at timestamptz
);
CREATE INDEX sessions_user ON sessions (user_id);

CREATE TABLE login_attempts (
    id bigserial PRIMARY KEY,
    key text NOT NULL,
    ok boolean NOT NULL,
    at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX login_attempts_key ON login_attempts (key, at);

CREATE TABLE settings (
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    key text NOT NULL,
    value text NOT NULL,
    updated_by uuid,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, key)
);

-- Append-only log with a hash chain per tenant.
CREATE TABLE activity_logs (
    id bigserial PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    at timestamptz NOT NULL DEFAULT now(),
    actor text NOT NULL,
    kind text NOT NULL,
    subject_type text NOT NULL DEFAULT '',
    subject_id text NOT NULL DEFAULT '',
    summary text NOT NULL,
    details jsonb NOT NULL DEFAULT '{}'::jsonb,
    prev_hash text NOT NULL DEFAULT '',
    hash text NOT NULL DEFAULT ''
);
CREATE INDEX activity_logs_tenant_at ON activity_logs (tenant_id, at DESC);

CREATE FUNCTION activity_logs_block_change() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'activity_logs is append-only';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER activity_logs_no_update BEFORE UPDATE OR DELETE ON activity_logs
    FOR EACH ROW EXECUTE FUNCTION activity_logs_block_change();

CREATE TABLE companies (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    name text NOT NULL,
    website text NOT NULL DEFAULT '',
    domain text,
    industry text NOT NULL DEFAULT '',
    city text NOT NULL DEFAULT '',
    size text NOT NULL DEFAULT '',
    phone text NOT NULL DEFAULT '',
    source text NOT NULL DEFAULT 'manual',
    status text NOT NULL DEFAULT 'active',
    notes text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE UNIQUE INDEX companies_domain_unique ON companies (tenant_id, lower(domain)) WHERE domain IS NOT NULL AND domain <> '';
CREATE INDEX companies_tenant ON companies (tenant_id);
CREATE INDEX companies_name_trgm ON companies USING gin (name gin_trgm_ops);

CREATE TABLE contacts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    company_id uuid REFERENCES companies(id),
    first_name text NOT NULL DEFAULT '',
    last_name text NOT NULL DEFAULT '',
    full_name text GENERATED ALWAYS AS (btrim(first_name || ' ' || last_name)) STORED,
    email text,
    email_status text NOT NULL DEFAULT 'unknown' CHECK (email_status IN ('unknown','verified','invalid','risky')),
    job_title text NOT NULL DEFAULT '',
    phone text NOT NULL DEFAULT '',
    city text NOT NULL DEFAULT '',
    country text NOT NULL DEFAULT 'Pakistan',
    stage text NOT NULL DEFAULT 'new' CHECK (stage IN ('new','to_write','contacted','replied','interested','ready_to_meet','won','closed_out')),
    score integer NOT NULL DEFAULT 0,
    score_parts jsonb NOT NULL DEFAULT '{}'::jsonb,
    source text NOT NULL DEFAULT 'manual',
    source_url text NOT NULL DEFAULT '',
    research_note text NOT NULL DEFAULT '',
    owner_id uuid REFERENCES users(id),
    do_not_contact boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE UNIQUE INDEX contacts_email_unique ON contacts (tenant_id, lower(email)) WHERE email IS NOT NULL AND email <> '';
CREATE INDEX contacts_tenant_stage ON contacts (tenant_id, stage);
CREATE INDEX contacts_name_trgm ON contacts USING gin (full_name gin_trgm_ops);

CREATE TABLE lists (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    name text NOT NULL,
    description text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'active',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE list_members (
    list_id uuid NOT NULL REFERENCES lists(id),
    contact_id uuid NOT NULL REFERENCES contacts(id),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    added_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (list_id, contact_id)
);

CREATE TABLE templates (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    name text NOT NULL,
    kind text NOT NULL DEFAULT 'email1' CHECK (kind IN ('email1','email2','email3','reply')),
    subject text NOT NULL DEFAULT '',
    body text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'active',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);

CREATE TABLE mailboxes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    email text NOT NULL,
    display_name text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','paused','error')),
    daily_limit integer NOT NULL DEFAULT 10,
    sent_today integer NOT NULL DEFAULT 0,
    sent_day date,
    last_checked timestamptz,
    -- App Password is encrypted with pgcrypto; the key is an environment secret.
    secret_enc bytea,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1,
    UNIQUE (tenant_id, email)
);

CREATE TABLE sequences (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    name text NOT NULL,
    description text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'active',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE sequence_steps (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    sequence_id uuid NOT NULL REFERENCES sequences(id),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    step_no integer NOT NULL,
    day_offset integer NOT NULL,
    template_id uuid REFERENCES templates(id),
    UNIQUE (sequence_id, step_no)
);

CREATE TABLE audiences (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    name text NOT NULL,
    min_score integer NOT NULL DEFAULT 60,
    industry text NOT NULL DEFAULT '',
    city text NOT NULL DEFAULT '',
    stage text NOT NULL DEFAULT 'new',
    status text NOT NULL DEFAULT 'active',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);

CREATE TABLE campaigns (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    name text NOT NULL,
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','running','paused','done')),
    sequence_id uuid REFERENCES sequences(id),
    audience_id uuid REFERENCES audiences(id),
    mailbox_id uuid REFERENCES mailboxes(id),
    subject_variants text NOT NULL DEFAULT '',
    started_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE campaign_contacts (
    campaign_id uuid NOT NULL REFERENCES campaigns(id),
    contact_id uuid NOT NULL REFERENCES contacts(id),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','stopped','done')),
    variant integer NOT NULL DEFAULT 0,
    next_step integer NOT NULL DEFAULT 1,
    next_send_at timestamptz,
    stop_reason text NOT NULL DEFAULT '',
    PRIMARY KEY (campaign_id, contact_id)
);
CREATE INDEX campaign_contacts_due ON campaign_contacts (next_send_at) WHERE status = 'active';

CREATE TABLE conversations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    contact_id uuid NOT NULL REFERENCES contacts(id),
    subject text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'open' CHECK (status IN ('open','handover','employee','ai','closed')),
    label text NOT NULL DEFAULT 'other',
    urgent boolean NOT NULL DEFAULT false,
    assigned_to uuid REFERENCES users(id),
    timer_due_at timestamptz,
    last_message_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);

CREATE TABLE emails (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    contact_id uuid REFERENCES contacts(id),
    campaign_id uuid REFERENCES campaigns(id),
    mailbox_id uuid REFERENCES mailboxes(id),
    conversation_id uuid REFERENCES conversations(id),
    direction text NOT NULL CHECK (direction IN ('out','in')),
    step_no integer NOT NULL DEFAULT 1,
    sender text NOT NULL DEFAULT '',
    recipient text NOT NULL DEFAULT '',
    subject text NOT NULL DEFAULT '',
    body text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued','blocked','sent','failed','received','waiting_approval')),
    blocked_reason text NOT NULL DEFAULT '',
    message_id text,
    written_by text NOT NULL DEFAULT 'ai',
    scheduled_at timestamptz,
    sent_at timestamptz,
    is_test boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE INDEX emails_due ON emails (scheduled_at) WHERE status = 'queued';
CREATE INDEX emails_conversation ON emails (conversation_id);
CREATE UNIQUE INDEX emails_message_id ON emails (tenant_id, message_id) WHERE message_id IS NOT NULL;
-- One email per person, campaign and step: a second run never sends it again.
CREATE UNIQUE INDEX emails_once_per_step ON emails (campaign_id, contact_id, step_no) WHERE direction = 'out' AND campaign_id IS NOT NULL;

CREATE TABLE suppression_list (
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    email text NOT NULL,
    reason text NOT NULL,
    words text NOT NULL DEFAULT '',
    at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, email)
);

CREATE TABLE tasks (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    title text NOT NULL,
    kind text NOT NULL DEFAULT 'other' CHECK (kind IN ('call','email','linkedin','meeting','other')),
    due_at timestamptz,
    status text NOT NULL DEFAULT 'open' CHECK (status IN ('open','done')),
    contact_id uuid REFERENCES contacts(id),
    assigned_to uuid REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE follow_ups (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    contact_id uuid REFERENCES contacts(id),
    note text NOT NULL DEFAULT '',
    due_at timestamptz,
    status text NOT NULL DEFAULT 'open' CHECK (status IN ('open','done')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE deals (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    title text NOT NULL,
    contact_id uuid REFERENCES contacts(id),
    company_id uuid REFERENCES companies(id),
    stage text NOT NULL DEFAULT 'open' CHECK (stage IN ('open','proposal','negotiation','won','lost')),
    value numeric(14,2),
    currency text NOT NULL DEFAULT 'PKR',
    close_date date,
    next_step text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE meetings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    title text NOT NULL,
    contact_id uuid REFERENCES contacts(id),
    starts_at timestamptz,
    status text NOT NULL DEFAULT 'planned' CHECK (status IN ('planned','held','cancelled','no_show')),
    host text NOT NULL DEFAULT '',
    link text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE calls (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    contact_id uuid REFERENCES contacts(id),
    direction text NOT NULL DEFAULT 'out' CHECK (direction IN ('out','in')),
    outcome text NOT NULL DEFAULT 'no_answer' CHECK (outcome IN ('no_answer','spoke','voicemail','wrong_number','booked')),
    notes text NOT NULL DEFAULT '',
    called_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE goals (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    name text NOT NULL,
    metric text NOT NULL DEFAULT 'meetings' CHECK (metric IN ('contacts','emails','replies','meetings','deals_won')),
    target integer NOT NULL DEFAULT 1,
    period_start date,
    period_end date,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);

CREATE TABLE sources (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    name text NOT NULL,
    kind text NOT NULL DEFAULT 'osm' CHECK (kind IN ('osm','overture','csv','website_list')),
    url text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'open' CHECK (status IN ('open','protected','blocked')),
    paused_until timestamptz,
    notes text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE collector_runs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    source_name text NOT NULL,
    kind text NOT NULL DEFAULT 'osm',
    status text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued','running','done','failed')),
    area text NOT NULL DEFAULT '',
    found integer NOT NULL DEFAULT 0,
    saved integer NOT NULL DEFAULT 0,
    duplicates integer NOT NULL DEFAULT 0,
    error text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE research_candidates (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    run_id uuid REFERENCES collector_runs(id),
    name text NOT NULL,
    website text NOT NULL DEFAULT '',
    email text NOT NULL DEFAULT '',
    industry text NOT NULL DEFAULT '',
    city text NOT NULL DEFAULT '',
    source_url text NOT NULL DEFAULT '',
    licence text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'needs_review' CHECK (status IN ('needs_review','saved','rejected')),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    kind text NOT NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    status text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued','running','done','failed','dead')),
    attempts integer NOT NULL DEFAULT 0,
    run_at timestamptz NOT NULL DEFAULT now(),
    locked_by text,
    locked_until timestamptz,
    last_error text NOT NULL DEFAULT '',
    dedupe_key text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX jobs_due ON jobs (run_at) WHERE status IN ('queued','failed');
CREATE UNIQUE INDEX jobs_dedupe ON jobs (tenant_id, dedupe_key) WHERE dedupe_key IS NOT NULL AND status IN ('queued','running');

CREATE TABLE ai_tasks (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    kind text NOT NULL,
    title text NOT NULL DEFAULT '',
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    status text NOT NULL DEFAULT 'waiting' CHECK (status IN ('waiting','claimed','done','failed')),
    created_by uuid REFERENCES users(id),
    result text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE ai_heartbeats (
    id bigserial PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE mcp_tokens (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    user_id uuid NOT NULL REFERENCES users(id),
    token_hash text NOT NULL UNIQUE,
    label text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz
);
CREATE TABLE mcp_tool_calls (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    user_id uuid,
    tool text NOT NULL,
    ok boolean NOT NULL DEFAULT true,
    summary text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);
CREATE TABLE quick_messages (
    id bigserial PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    user_id uuid NOT NULL REFERENCES users(id),
    role text NOT NULL CHECK (role IN ('user','quick')),
    body text NOT NULL,
    at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE notices (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    user_id uuid REFERENCES users(id),
    kind text NOT NULL,
    title text NOT NULL,
    body text NOT NULL DEFAULT '',
    link text NOT NULL DEFAULT '',
    read_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX notices_tenant ON notices (tenant_id, created_at DESC);

CREATE TABLE webhooks (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    url text NOT NULL,
    events text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'active',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    row_version bigint NOT NULL DEFAULT 1
);

-- Tenant safety net (row level security). The app sets app.tenant_id for every
-- transaction. The migration owner bypasses RLS; the runtime role does not.
DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY[
        'settings','activity_logs','companies','contacts','lists','list_members','templates',
        'mailboxes','sequences','sequence_steps','audiences','campaigns','campaign_contacts','conversations',
        'emails','suppression_list','tasks','follow_ups','deals','meetings','calls','goals','sources',
        'collector_runs','research_candidates','jobs','ai_tasks','mcp_tool_calls','notices','webhooks']
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format($p$CREATE POLICY tenant_isolation ON %I USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)$p$, t);
    END LOOP;
END $$;
