//! Demo data for a local run. Every company and person here is made up.
//! It uses the owner database role (a setup task, not part of normal use).

use crate::error::{Error, Result};
use rand::RngCore;
use sqlx::PgPool;

pub const EMAIL1_SUBJECT: &str = "Weekly reports at {company}";
pub const EMAIL1: &str = "Hi {first_name},\n\n{personal_line} {company} in {city} probably keeps a lot of numbers in spreadsheets, and many teams there lose an hour each week copying them into reports. A short script can often prepare those reports before anyone arrives in the morning. Which report takes your team the longest to build today?\n\n{sender_name}";
pub const EMAIL2_SUBJECT: &str = "Reports at {company}";
pub const EMAIL2: &str = "Hi {first_name},\n\nA note from a few days ago about weekly reports at {company} may have been buried. If it is not a priority now, that is fine. If it is, I can show how a similar team cut that work to minutes. Would a short call this week suit you?\n\n{sender_name}";
pub const EMAIL3_SUBJECT: &str = "Closing the loop";
pub const EMAIL3: &str = "Hi {first_name},\n\nThis is my last note about reports at {company}. If the timing is wrong, I will not write again. If it helps, reply and I will send two examples. Is that useful?\n\n{sender_name}";

const COMPANIES: [(&str, &str, &str, &str); 8] = [
    ("Sindh Fabrics Trading", "sindhfabrics.example", "Textiles", "Karachi"),
    ("Lahore Logistics Hub", "lahorelogistics.example", "Logistics", "Lahore"),
    ("Indus Dental Care", "indusdental.example", "Healthcare", "Karachi"),
    ("Punjab Agro Supplies", "punjabagro.example", "Agriculture", "Faisalabad"),
    ("Margalla Software House", "margallasoft.example", "Software", "Islamabad"),
    ("Clifton Retail Group", "cliftonretail.example", "Retail", "Karachi"),
    ("Ravi Steel Works", "ravisteel.example", "Manufacturing", "Lahore"),
    ("Harbour Freight Agents", "harbourfreight.example", "Logistics", "Karachi"),
];

const PEOPLE: [(&str, &str, &str); 8] = [
    ("Ayesha", "Siddiqui", "Operations Manager"),
    ("Bilal", "Chaudhry", "Founder"),
    ("Hira", "Farooqui", "Practice Manager"),
    ("Usman", "Gill", "Director"),
    ("Sara", "Naqvi", "Head of Product"),
    ("Imran", "Qureshi", "Owner"),
    ("Fatima", "Zahid", "General Manager"),
    ("Danish", "Memon", "Finance Manager"),
];

pub async fn run(owner: &PgPool, tenant: Option<&str>) -> Result<String> {
    let mut note = String::new();
    let tid = match tenant {
        Some(t) => t.to_string(),
        None => {
            let mut raw = [0u8; 9];
            rand::rngs::OsRng.fill_bytes(&mut raw);
            let password = format!("Demo-{}-ok", hex::encode(raw));
            let (t, _) = crate::accounts::create_tenant(owner, "Quickanalytics Demo", "admin@demo.example", "Demo Admin", &password).await?;
            note = format!("Demo company created. Sign in with admin@demo.example and the password {password} (shown once).\n");
            t
        }
    };
    let admin: String = sqlx::query_scalar("SELECT id::text FROM users WHERE tenant_id = $1::uuid ORDER BY created_at LIMIT 1").bind(&tid).fetch_one(owner).await.map_err(|_| Error::NotFound)?;
    let mut tx = owner.begin().await?;
    let mut company_ids = vec![];
    for (name, domain, industry, city) in COMPANIES {
        let id: String = sqlx::query_scalar(
            "INSERT INTO companies (tenant_id, name, website, domain, industry, city) VALUES ($1::uuid, $2, $3, $4, $5, $6) \
             ON CONFLICT (tenant_id, lower(domain)) WHERE domain IS NOT NULL AND domain <> '' DO UPDATE SET updated_at = now() RETURNING id::text",
        )
        .bind(&tid).bind(name).bind(format!("https://{domain}")).bind(domain).bind(industry).bind(city).fetch_one(&mut *tx).await?;
        company_ids.push((id, domain, city));
    }
    for (i, (first, last, title)) in PEOPLE.iter().enumerate() {
        let (cid, domain, city) = &company_ids[i];
        sqlx::query(
            "INSERT INTO contacts (tenant_id, company_id, first_name, last_name, email, job_title, city, owner_id, email_status, stage, research_note) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8::uuid, $9, 'new', '') ON CONFLICT DO NOTHING",
        )
        .bind(&tid).bind(cid).bind(first).bind(last).bind(format!("{}@{domain}", first.to_lowercase())).bind(title).bind(city).bind(&admin)
        .bind(if i % 3 == 2 { "unknown" } else { "verified" }).execute(&mut *tx).await?;
    }
    let mut tpl = vec![];
    for (name, kind, subject, body) in [("Email 1 weekly reports", "email1", EMAIL1_SUBJECT, EMAIL1), ("Email 2 follow-up", "email2", EMAIL2_SUBJECT, EMAIL2), ("Email 3 last note", "email3", EMAIL3_SUBJECT, EMAIL3)] {
        let id: String = sqlx::query_scalar("INSERT INTO templates (tenant_id, name, kind, subject, body) SELECT $1::uuid, $2, $3, $4, $5 WHERE NOT EXISTS (SELECT 1 FROM templates WHERE tenant_id = $1::uuid AND name = $2) RETURNING id::text")
            .bind(&tid).bind(name).bind(kind).bind(subject).bind(body).fetch_optional(&mut *tx).await?.unwrap_or_default();
        tpl.push(id);
    }
    if !tpl.iter().any(|t| t.is_empty()) {
        let seq: String = sqlx::query_scalar("INSERT INTO sequences (tenant_id, name, description) VALUES ($1::uuid, 'Three emails', 'Day 0, day 3 and day 7') RETURNING id::text").bind(&tid).fetch_one(&mut *tx).await?;
        for (n, (day, t)) in [(0, &tpl[0]), (3, &tpl[1]), (7, &tpl[2])].iter().enumerate() {
            sqlx::query("INSERT INTO sequence_steps (sequence_id, tenant_id, step_no, day_offset, template_id) VALUES ($1::uuid, $2::uuid, $3, $4, $5::uuid)").bind(&seq).bind(&tid).bind(n as i32 + 1).bind(*day).bind(t).execute(&mut *tx).await?;
        }
        let aud: String = sqlx::query_scalar("INSERT INTO audiences (tenant_id, name, min_score, stage) VALUES ($1::uuid, 'Verified new people', 0, 'new') RETURNING id::text").bind(&tid).fetch_one(&mut *tx).await?;
        let mb: String = sqlx::query_scalar("INSERT INTO mailboxes (tenant_id, email, display_name) VALUES ($1::uuid, 'sender@demo.example', 'Demo sender') ON CONFLICT (tenant_id, email) DO UPDATE SET display_name = EXCLUDED.display_name RETURNING id::text").bind(&tid).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO campaigns (tenant_id, name, sequence_id, audience_id, mailbox_id) VALUES ($1::uuid, 'Demo campaign', $2::uuid, $3::uuid, $4::uuid)").bind(&tid).bind(&seq).bind(&aud).bind(&mb).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO sources (tenant_id, name, kind, url) SELECT $1::uuid, 'OpenStreetMap', 'osm', 'https://www.openstreetmap.org' WHERE NOT EXISTS (SELECT 1 FROM sources WHERE tenant_id = $1::uuid AND name = 'OpenStreetMap')").bind(&tid).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO sources (tenant_id, name, kind, url) SELECT $1::uuid, 'CSV upload', 'csv', '' WHERE NOT EXISTS (SELECT 1 FROM sources WHERE tenant_id = $1::uuid AND name = 'CSV upload')").bind(&tid).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(format!("{note}Demo data added: {} companies, {} people, 3 templates, 1 sequence, 1 campaign.", COMPANIES.len(), PEOPLE.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::draft::{check, Rules};
    use crate::outbox::merge;
    use std::collections::HashMap;

    #[test]
    fn demo_templates_follow_the_writing_rules() {
        let mut f: HashMap<&str, String> = HashMap::new();
        for (k, v) in [("first_name", "Ayesha"), ("company", "Sindh Fabrics Trading"), ("city", "Karachi"), ("personal_line", "Runs a wholesale fabric shop."), ("sender_name", "Rahat")] {
            f.insert(k, v.to_string());
        }
        for (step, s, b) in [(1, EMAIL1_SUBJECT, EMAIL1), (2, EMAIL2_SUBJECT, EMAIL2), (3, EMAIL3_SUBJECT, EMAIL3)] {
            let subject = merge(s, &f).unwrap();
            let body = merge(b, &f).unwrap();
            let problems = check(&subject, &body, step, &Rules { ban_dashes: true });
            assert!(problems.is_empty(), "email {step}: {problems:?}");
        }
    }
}
