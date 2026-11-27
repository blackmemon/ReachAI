ALTER TABLE companies
    ADD COLUMN is_competitor boolean NOT NULL DEFAULT false;

ALTER TABLE emails
    ADD COLUMN group_message_id uuid;

CREATE INDEX emails_group_message ON emails (tenant_id, group_message_id) WHERE group_message_id IS NOT NULL;

CREATE TABLE email_user_state (
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    user_id uuid NOT NULL REFERENCES users(id),
    email_id uuid NOT NULL REFERENCES emails(id) ON DELETE CASCADE,
    read_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, email_id)
);

CREATE TABLE conversation_user_state (
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    user_id uuid NOT NULL REFERENCES users(id),
    conversation_id uuid NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    last_read_at timestamptz,
    archived boolean NOT NULL DEFAULT false,
    PRIMARY KEY (user_id, conversation_id)
);

CREATE TABLE conversation_participants (
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    conversation_id uuid NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    contact_id uuid NOT NULL REFERENCES contacts(id),
    added_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (conversation_id, contact_id)
);

INSERT INTO conversation_participants (tenant_id, conversation_id, contact_id)
SELECT tenant_id, id, contact_id FROM conversations
ON CONFLICT DO NOTHING;

CREATE TABLE labels (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    kind text NOT NULL CHECK (kind IN ('email', 'conversation')),
    name text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 40),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, id)
);

CREATE UNIQUE INDEX labels_tenant_kind_name ON labels (tenant_id, kind, lower(name));

CREATE TABLE email_labels (
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    email_id uuid NOT NULL REFERENCES emails(id) ON DELETE CASCADE,
    label_id uuid NOT NULL,
    PRIMARY KEY (email_id, label_id),
    FOREIGN KEY (tenant_id, label_id) REFERENCES labels(tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE conversation_labels (
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    conversation_id uuid NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    label_id uuid NOT NULL,
    PRIMARY KEY (conversation_id, label_id),
    FOREIGN KEY (tenant_id, label_id) REFERENCES labels(tenant_id, id) ON DELETE CASCADE
);

-- Plain SQL on purpose: no dependency on the plpgsql extension library.
ALTER TABLE email_user_state ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON email_user_state USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
ALTER TABLE conversation_user_state ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON conversation_user_state USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
ALTER TABLE conversation_participants ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON conversation_participants USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
ALTER TABLE labels ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON labels USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
ALTER TABLE email_labels ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON email_labels USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
ALTER TABLE conversation_labels ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON conversation_labels USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

DROP POLICY tenant_isolation ON email_user_state;
CREATE POLICY user_state_isolation ON email_user_state
    USING (
        tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid
        AND user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    )
    WITH CHECK (
        tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid
        AND user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    );

DROP POLICY tenant_isolation ON conversation_user_state;
CREATE POLICY user_state_isolation ON conversation_user_state
    USING (
        tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid
        AND user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    )
    WITH CHECK (
        tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid
        AND user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    );
