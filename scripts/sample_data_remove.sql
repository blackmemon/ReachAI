-- Removes only what scripts/sample_data.sql added: rows with source 'sample_data' and the deals,
-- tasks and conversations that belong to them, in the "ReachAI Sample Data" company.
-- Real companies are not touched. Plain SQL (no plpgsql in this PostgreSQL build).
BEGIN;
CREATE TEMP TABLE sd_ids AS SELECT t.id AS tid FROM tenants t WHERE t.name = 'ReachAI Sample Data';
DELETE FROM tasks WHERE tenant_id IN (SELECT tid FROM sd_ids) AND contact_id IN (SELECT id FROM contacts WHERE source = 'sample_data');
DELETE FROM deals WHERE tenant_id IN (SELECT tid FROM sd_ids) AND (contact_id IN (SELECT id FROM contacts WHERE source = 'sample_data') OR company_id IN (SELECT id FROM companies WHERE source = 'sample_data'));
DELETE FROM conversations WHERE tenant_id IN (SELECT tid FROM sd_ids) AND contact_id IN (SELECT id FROM contacts WHERE source = 'sample_data');
DELETE FROM contacts WHERE tenant_id IN (SELECT tid FROM sd_ids) AND source = 'sample_data';
DELETE FROM companies WHERE tenant_id IN (SELECT tid FROM sd_ids) AND source = 'sample_data';
COMMIT;
