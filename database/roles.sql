-- Run once per cluster by the deployer (roles are cluster-wide, not part of a migration).
-- leads_app: used by the gateway. Row level security applies to it.
-- leads_worker: used by the worker. It must scan all tenants, so it bypasses RLS and
--   the worker code still sets the tenant on every write it makes.
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'leads_app') THEN
    CREATE ROLE leads_app LOGIN PASSWORD 'leads_app' NOSUPERUSER NOBYPASSRLS;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'leads_worker') THEN
    CREATE ROLE leads_worker LOGIN PASSWORD 'leads_worker' NOSUPERUSER BYPASSRLS;
  END IF;
END $$;
