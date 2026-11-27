ALTER TABLE contacts
    ADD COLUMN display_name text NOT NULL DEFAULT '',
    ADD COLUMN emails text NOT NULL DEFAULT '',
    ADD COLUMN phone_numbers text NOT NULL DEFAULT '',
    ADD COLUMN links text NOT NULL DEFAULT '',
    ADD COLUMN keywords text NOT NULL DEFAULT '';

UPDATE contacts
SET display_name = full_name,
    emails = COALESCE(email, ''),
    phone_numbers = COALESCE(phone, '');

ALTER TABLE companies
    ADD COLUMN emails text NOT NULL DEFAULT '',
    ADD COLUMN phone_numbers text NOT NULL DEFAULT '',
    ADD COLUMN links text NOT NULL DEFAULT '',
    ADD COLUMN employees integer,
    ADD COLUMN industries text NOT NULL DEFAULT '',
    ADD COLUMN keywords text NOT NULL DEFAULT '';

UPDATE companies
SET phone_numbers = COALESCE(phone, ''),
    links = COALESCE(website, ''),
    industries = COALESCE(industry, '');
