-- Sample data for testing lists, search, filters, sorting and paging with a larger dataset.
-- Everything is made up. It goes only into its own company, "ReachAI Sample Data", so it never
-- mixes with real records, and scripts/sample_data_remove.sql takes it out again.
-- Plain SQL on purpose (no plpgsql in this PostgreSQL build). Run with the owner role:
--   psql "$LEADS_OWNER_URL" -v ON_ERROR_STOP=1 -f scripts/sample_data.sql
-- Create the company first:
--   blackmemon-leads create-tenant "ReachAI Sample Data" sample@sample.example "Sample Admin" "<password>"
-- Running it twice adds nothing the second time.
BEGIN;
CREATE TEMP TABLE sd_t AS SELECT t.id AS tid, (SELECT u.id FROM users u WHERE u.tenant_id = t.id ORDER BY u.created_at LIMIT 1) AS admin
  FROM tenants t WHERE t.name = 'ReachAI Sample Data'
   AND NOT EXISTS (SELECT 1 FROM companies c WHERE c.tenant_id = t.id AND c.source = 'sample_data');
CREATE TEMP TABLE sd_a AS SELECT
  ARRAY['James','Maria','Robert','Linda','Michael','Priya','David','Sofia','Daniel','Aisha','Kevin','Elena','Brian','Nadia','Carlos','Hannah','Omar','Grace','Victor','Mei','Anthony','Isabel','Tariq','Laura','Marcus','Chloe','Samuel','Diana','Raj','Olivia'] AS fnames,
  ARRAY['Rossi','Nguyen','Patel','Johnson','Garcia','Kim','Martinez','Cohen','Williams','Haddad','Brown','Lopez','Singh','Murphy','Silva','Anderson','Khan','Davis','Moreau','Tanaka','Clark','Romero','Ahmed','Bennett','Wright','Castillo','Foster','Ortiz','Hughes','Walker'] AS lnames,
  ARRAY['Owner','General Manager','Executive Chef','Operations Manager','Founder','Catering Director','Marketing Manager','Restaurant Manager','Franchise Owner','Purchasing Manager'] AS titles,
  ARRAY['Italian','Mexican','Thai','Steakhouse','Seafood','Indian','Sushi','Bakery','Vegan','Burgers','Mediterranean','Barbecue','Diner','Ramen','Tapas'] AS cuisines,
  ARRAY['Blue','Golden','Rustic','Urban','Little','Grand','Copper','Saffron','Maple','Harbor','Olive','Ember','Willow','Crimson','Juniper'] AS p1,
  ARRAY['Table','Kitchen','Grill','Bistro','Cantina','House','Garden','Tavern','Eatery','Fork','Spoon','Oven','Plate','Cafe','Smokehouse'] AS p2,
  ARRAY['New York','Los Angeles','Chicago','Houston','Phoenix','San Diego','Dallas','Austin','San Francisco','Seattle','Miami','Boston','Denver','Atlanta','Portland'] AS cities,
  ARRAY['NY','CA','IL','TX','AZ','CA','TX','TX','CA','WA','FL','MA','CO','GA','OR'] AS states,
  ARRAY['new','to_write','contacted','replied','interested','ready_to_meet','won','closed_out'] AS stages,
  ARRAY['verified','verified','verified','unknown','risky','invalid'] AS verif,
  ARRAY['open','proposal','negotiation','won','lost'] AS dstage,
  ARRAY['call','email','linkedin','meeting','other'] AS tkind;

INSERT INTO companies (tenant_id, name, website, domain, industry, city, size, phone, source, status, notes, emails, phone_numbers, links, employees, industries, keywords, is_competitor)
SELECT t.tid,
       a.p1[1 + (ci*3)%15] || ' ' || a.p2[1 + (ci*5 + ci/15)%15] || ' ' || a.cuisines[1 + (ci*7)%15],
       'https://' || d.dom, d.dom, 'Restaurants - ' || a.cuisines[1 + (ci*7)%15], a.cities[1 + (ci*7)%15] || ', ' || a.states[1 + (ci*7)%15],
       (ARRAY['1-10','11-50','51-200','201-500'])[1 + ci%4], '+1 555 01' || lpad((ci%100)::text,2,'0') || ' ' || lpad((1000 + ci*37%9000)::text,4,'0'),
       'sample_data', 'active', 'Sample record ' || ci, 'info@' || d.dom, '+1 555 01' || lpad((ci%100)::text,2,'0'),
       'https://' || d.dom, 5 + (ci*13)%180, 'Restaurants' || E'\n' || a.cuisines[1 + (ci*7)%15], lower(a.cuisines[1 + (ci*7)%15]) || E'\ncatering', ci%11 = 0
FROM sd_t t, sd_a a, generate_series(1,160) ci,
     LATERAL (SELECT lower(a.p1[1 + (ci*3)%15] || a.p2[1 + (ci*5 + ci/15)%15]) || ci || '.example' AS dom) d;

INSERT INTO contacts (tenant_id, company_id, first_name, last_name, email, email_status, job_title, phone, city, country, stage, score, source, source_url, research_note, owner_id, do_not_contact, display_name, emails, phone_numbers, links, keywords, created_at)
SELECT t.tid, c.id, f.fn, f.ln, lower(f.fn || '.' || f.ln) || x.ci || '@' || c.domain, a.verif[1 + (x.ci + i)%6], a.titles[1 + (x.ci*i)%10],
       '+1 555 02' || lpad(((x.ci*i)%100)::text,2,'0') || ' ' || lpad((2000 + x.ci*53%7000)::text,4,'0'), split_part(c.city, ',', 1), 'United States',
       a.stages[1 + (x.ci + i)%8], (x.ci*17 + i*11)%100, 'sample_data', c.website, 'Sample record for testing.', t.admin, (x.ci%37 = 0 AND i = 2),
       f.fn || ' ' || f.ln, lower(f.fn || '.' || f.ln) || x.ci || '@' || c.domain, '+1 555 02' || lpad(((x.ci*i)%100)::text,2,'0') || ' ' || lpad((2000 + x.ci*53%7000)::text,4,'0'), 'https://linkedin.com/in/' || lower(f.fn || f.ln) || x.ci, lower(a.cuisines[1 + (x.ci*7)%15]),
       now() - make_interval(days => (x.ci*3 + i)%120)
FROM sd_t t, sd_a a, companies c, generate_series(1,2) i,
     LATERAL (SELECT split_part(c.notes, ' ', 3)::int AS ci) x,
     LATERAL (SELECT a.fnames[1 + (x.ci*i*3)%30] AS fn, a.lnames[1 + (x.ci*7 + i*5)%30] AS ln) f
WHERE c.tenant_id = t.tid AND c.source = 'sample_data';

INSERT INTO deals (tenant_id, title, contact_id, company_id, stage, value, currency, close_date, next_step)
SELECT t.tid, c.name || ' catering contract', k.id, c.id, a.dstage[1 + x.ci%5], 1500 + (x.ci*271)%48000, 'USD', current_date + (x.ci%90), 'Send proposal'
FROM sd_t t, sd_a a, companies c, LATERAL (SELECT split_part(c.notes, ' ', 3)::int AS ci) x,
     LATERAL (SELECT id FROM contacts WHERE company_id = c.id ORDER BY email LIMIT 1) k
WHERE c.tenant_id = t.tid AND c.source = 'sample_data' AND x.ci%2 = 0;

INSERT INTO tasks (tenant_id, title, kind, due_at, status, contact_id, assigned_to)
SELECT t.tid, 'Follow up with ' || k.first_name || ' ' || k.last_name, a.tkind[1 + (x.ci + k.score)%5], now() + make_interval(days => (x.ci%21) - 7),
       CASE WHEN x.ci%4 = 0 THEN 'done' ELSE 'open' END, k.id, t.admin
FROM sd_t t, sd_a a, contacts k, LATERAL (SELECT split_part(c.notes, ' ', 3)::int AS ci FROM companies c WHERE c.id = k.company_id) x
WHERE k.tenant_id = t.tid AND k.source = 'sample_data' AND x.ci%3 = 0;

INSERT INTO conversations (tenant_id, contact_id, subject, status, label, urgent, last_message_at)
SELECT t.tid, k.id, 'Question about the menu offer at ' || c.name, 'open', 'other', x.ci%10 = 0, now() - make_interval(hours => x.ci%200)
FROM sd_t t, companies c, LATERAL (SELECT split_part(c.notes, ' ', 3)::int AS ci) x,
     LATERAL (SELECT id FROM contacts WHERE company_id = c.id ORDER BY email LIMIT 1) k
WHERE c.tenant_id = t.tid AND c.source = 'sample_data' AND x.ci%5 = 0;
COMMIT;
