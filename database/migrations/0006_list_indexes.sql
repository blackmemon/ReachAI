-- Lists show the newest rows first, 25 at a time. These indexes let PostgreSQL read just the
-- first page in order instead of sorting every row of the company for each page.
CREATE INDEX IF NOT EXISTS contacts_tenant_created ON contacts (tenant_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS companies_tenant_created ON companies (tenant_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS deals_tenant_created ON deals (tenant_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS conversations_tenant_last_message ON conversations (tenant_id, last_message_at DESC, id);
CREATE INDEX IF NOT EXISTS tasks_tenant_status_due ON tasks (tenant_id, status, due_at NULLS LAST, id);
CREATE INDEX IF NOT EXISTS emails_tenant_recent ON emails (tenant_id, (COALESCE(sent_at, scheduled_at, created_at)) DESC, id);
