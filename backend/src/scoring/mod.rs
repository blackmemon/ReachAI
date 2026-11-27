//! Lead score 0 to 100 from editable weights. Unknown data gives 0 points for that part.
//! It never gives guessed points. Each part keeps a short reason.

use crate::error::Result;
use crate::settings;
use serde_json::json;
use sqlx::{PgConnection, Row};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Weights {
    pub industry: i64,
    pub role: i64,
    pub size: i64,
    pub email: i64,
    pub need: i64,
    pub location: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Input {
    pub industry: String,
    pub job_title: String,
    pub company_size: String,
    pub website: String,
    pub email_status: String,
    pub research_note: String,
    pub country: String,
    pub approved_industries: Vec<String>,
    pub target_country: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub points: i64,
    pub reason: String,
}

const TOP_ROLES: [&str; 11] = ["owner", "founder", "ceo", "chief", "cto", "coo", "cio", "managing director", "director", "head", "president"];
const MID_ROLES: [&str; 3] = ["manager", "lead", "vp"];
const NEED_WORDS: [&str; 14] = [
    "data", "ai ", "automation", "automate", "report", "excel", "manual", "spreadsheet", "dashboard", "integration", "chatbot", "support", "analytics", "workflow",
];

fn pct(w: i64, p: i64) -> i64 {
    w * p / 100
}

fn words_match(industry: &str, approved: &str) -> bool {
    let a = industry.to_lowercase();
    let b = approved.to_lowercase();
    if a.contains(&b) || b.contains(&a) {
        return true;
    }
    // "Retail" matches "Retail and e-commerce": compare the first word.
    b.split_whitespace().next().map(|w| a.contains(w)).unwrap_or(false)
}

pub fn score(i: &Input, w: &Weights) -> (i64, BTreeMap<&'static str, Part>) {
    let mut parts = BTreeMap::new();
    let ind = i.industry.trim();
    parts.insert(
        "industry",
        if ind.is_empty() {
            Part { points: 0, reason: "Industry not available.".into() }
        } else if i.approved_industries.iter().any(|a| words_match(ind, a)) {
            Part { points: w.industry, reason: "Industry is one of the approved industries.".into() }
        } else {
            Part { points: pct(w.industry, 40), reason: "Industry is not on the approved list.".into() }
        },
    );
    let title = i.job_title.to_lowercase();
    parts.insert(
        "role",
        if title.trim().is_empty() {
            Part { points: 0, reason: "Job title not available.".into() }
        } else if TOP_ROLES.iter().any(|r| title.contains(r)) {
            Part { points: w.role, reason: "Decision-maker title (a search term, not proof of authority).".into() }
        } else if MID_ROLES.iter().any(|r| title.split_whitespace().any(|t| t == *r)) {
            Part { points: pct(w.role, 60), reason: "Manager-level title.".into() }
        } else {
            Part { points: pct(w.role, 20), reason: "Other title.".into() }
        },
    );
    let mut size_pts = 0;
    let mut size_why = vec![];
    if !i.company_size.trim().is_empty() {
        size_pts += pct(w.size, 60);
        size_why.push("company size is known");
    }
    if !i.website.trim().is_empty() {
        size_pts += pct(w.size, 40);
        size_why.push("website is known");
    }
    parts.insert(
        "size",
        Part { points: size_pts, reason: if size_why.is_empty() { "Size and activity not available.".into() } else { format!("{}.", size_why.join(", ")) } },
    );
    parts.insert(
        "email",
        if i.email_status == "verified" {
            Part { points: w.email, reason: "Email is verified.".into() }
        } else {
            Part { points: 0, reason: "Email is not verified.".into() }
        },
    );
    let note = format!("{} ", i.research_note.to_lowercase());
    parts.insert(
        "need",
        if NEED_WORDS.iter().any(|k| note.contains(k)) {
            Part { points: w.need, reason: "The research note shows a data or AI need.".into() }
        } else {
            Part { points: 0, reason: "No data or AI need found in the research note.".into() }
        },
    );
    parts.insert(
        "location",
        if !i.country.trim().is_empty() && i.country.eq_ignore_ascii_case(i.target_country.trim()) {
            Part { points: w.location, reason: "Located in the target country.".into() }
        } else {
            Part { points: 0, reason: "Not in the target country, or country not available.".into() }
        },
    );
    let total: i64 = parts.values().map(|p| p.points).sum();
    (total.clamp(0, 100), parts)
}

pub async fn weights(conn: &mut PgConnection, tenant: &str) -> Result<(Weights, i64, Vec<String>, String)> {
    let s = settings::all(conn, tenant).await?;
    let w = Weights {
        industry: settings::int(&s, "score_industry"),
        role: settings::int(&s, "score_role"),
        size: settings::int(&s, "score_size"),
        email: settings::int(&s, "score_email"),
        need: settings::int(&s, "score_need"),
        location: settings::int(&s, "score_location"),
    };
    let inds = settings::text(&s, "industries").lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    Ok((w, settings::int(&s, "score_threshold"), inds, settings::text(&s, "target_country").to_string()))
}

/// Score again after new data arrives. A new person who qualifies moves from New to To write.
pub async fn rescore_contact(conn: &mut PgConnection, tenant: &str, id: &str) -> Result<i64> {
    let (w, threshold, inds, country) = weights(conn, tenant).await?;
    let r = sqlx::query(
        "SELECT COALESCE(co.industry,''), c.job_title, COALESCE(co.size,''), COALESCE(co.website,''), c.email_status, c.research_note, c.country, c.stage \
         FROM contacts c LEFT JOIN companies co ON co.id = c.company_id WHERE c.id = $1::uuid AND c.tenant_id = $2::uuid",
    )
    .bind(id)
    .bind(tenant)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(r) = r else { return Ok(0) };
    let input = Input {
        industry: r.get(0), job_title: r.get(1), company_size: r.get(2), website: r.get(3), email_status: r.get(4),
        research_note: r.get(5), country: r.get(6), approved_industries: inds, target_country: country,
    };
    let (total, parts) = score(&input, &w);
    let stage: String = r.get(7);
    let json_parts: serde_json::Value = parts.iter().map(|(k, p)| ((*k).to_string(), json!({"points": p.points, "reason": p.reason}))).collect::<serde_json::Map<_, _>>().into();
    let new_stage = if stage == "new" && total >= threshold && input.email_status == "verified" { "to_write" } else { stage.as_str() };
    sqlx::query("UPDATE contacts SET score = $3, score_parts = $4::jsonb, stage = $5 WHERE id = $1::uuid AND tenant_id = $2::uuid")
        .bind(id)
        .bind(tenant)
        .bind(total as i32)
        .bind(json_parts.to_string())
        .bind(new_stage)
        .execute(&mut *conn)
        .await?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w() -> Weights {
        Weights { industry: 25, role: 20, size: 15, email: 15, need: 15, location: 10 }
    }

    fn full() -> Input {
        Input {
            industry: "Retail".into(), job_title: "Founder and CEO".into(), company_size: "11-50".into(), website: "https://x.pk".into(),
            email_status: "verified".into(), research_note: "Their team builds reports by hand in Excel.".into(), country: "Pakistan".into(),
            approved_industries: vec!["Retail and e-commerce".into(), "Healthcare".into()], target_country: "Pakistan".into(),
        }
    }

    #[test]
    fn perfect_lead_scores_100() {
        assert_eq!(score(&full(), &w()).0, 100);
    }

    #[test]
    fn unknown_data_gives_zero_not_a_guess() {
        let (t, parts) = score(&Input { target_country: "Pakistan".into(), ..Default::default() }, &w());
        assert_eq!(t, 0);
        assert!(parts.values().all(|p| p.points == 0));
        assert!(parts["industry"].reason.contains("not available"));
    }

    #[test]
    fn unapproved_industry_gets_fewer_points() {
        let mut i = full();
        i.industry = "Mining".into();
        assert_eq!(score(&i, &w()).1["industry"].points, 10);
    }

    #[test]
    fn manager_title_gets_partial_points() {
        let mut i = full();
        i.job_title = "Operations Manager".into();
        assert_eq!(score(&i, &w()).1["role"].points, 12);
    }

    #[test]
    fn unverified_email_gets_no_email_points() {
        let mut i = full();
        i.email_status = "risky".into();
        assert_eq!(score(&i, &w()).1["email"].points, 0);
        assert_eq!(score(&i, &w()).0, 85);
    }

    #[test]
    fn other_country_gets_no_location_points() {
        let mut i = full();
        i.country = "UAE".into();
        assert_eq!(score(&i, &w()).1["location"].points, 0);
    }

    #[test]
    fn score_never_exceeds_100() {
        let big = Weights { industry: 90, role: 90, size: 90, email: 90, need: 90, location: 90 };
        assert_eq!(score(&full(), &big).0, 100);
    }
}
