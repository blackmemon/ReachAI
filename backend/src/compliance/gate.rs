//! The compliance gate: runs in code before every email and AI reply (skill:
//! settings/compliance_privacy/compliance-handler.md). If any check fails the email is
//! blocked, the reason is saved and shown on the Today page.

use super::draft;

pub struct GateInput<'a> {
    pub do_not_contact: bool,
    pub suppressed: bool,
    pub email_status: &'a str,
    pub subject: &'a str,
    /// The text written for the person, without the footer.
    pub body: &'a str,
    /// The footer that is added under every email.
    pub footer: &'a str,
    pub unsubscribe_url: &'a str,
    pub mailbox_active: bool,
    pub mailbox_sent_today: i64,
    pub mailbox_daily_limit: i64,
    pub in_send_window: bool,
    pub paused: bool,
    pub step: u32,
    pub ban_dashes: bool,
    /// Domains a link may point to: our site, the booking page, the lead's own domain.
    pub allowed_domains: Vec<String>,
    /// First name and company as stored; the text must not name a different one in the greeting.
    pub contact_first_name: &'a str,
    /// Hand-written by a person (employee) or by AI. Employee drafts still pass the same gate.
    pub is_reply: bool,
}

const FAKE_URGENCY: [&str; 6] = ["act now", "limited time", "last chance", "hurry", "expires today", "only today"];
const MONEY_ASKS: [&str; 7] = ["bank transfer", "wire transfer", "gift card", "crypto", "bitcoin", "send money", "western union"];
const PROMPT_LEAKS: [&str; 7] = ["ignore previous", "ignore all previous", "system prompt", "as an ai", "claude", "skill.md", "my instructions"];

fn host_of(link: &str) -> String {
    let l = link.trim_matches(|c: char| ",.;:)\"'<>(".contains(c));
    let l = l.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
    l.split(['/', '?', '#']).next().unwrap_or("").to_lowercase()
}

fn has_phone_number(s: &str) -> bool {
    let mut run = 0;
    for c in s.chars() {
        if c.is_ascii_digit() {
            run += 1;
            if run >= 9 {
                return true;
            }
        } else if c == ' ' || c == '-' || c == '+' || c == '(' || c == ')' {
            // separators inside a number do not reset the run
        } else {
            run = 0;
        }
    }
    false
}

/// Reasons that pass by themselves: the email waits instead of being blocked.
pub const TEMPORARY: [&str; 4] = [
    "The mailbox reached its daily limit.",
    "It is outside the send window.",
    "Sending is paused for everyone.",
    "The mailbox is not active.",
];

pub fn only_temporary(reasons: &[String]) -> bool {
    !reasons.is_empty() && reasons.iter().all(|r| TEMPORARY.contains(&r.as_str()))
}

pub fn check(i: &GateInput) -> Vec<String> {
    let mut why: Vec<String> = vec![];
    let all = format!("{}\n{}", i.subject, i.body).to_lowercase();

    // 1 suppression and do-not-contact
    if i.suppressed || i.do_not_contact {
        why.push("The person unsubscribed or is marked do not contact.".into());
    }
    // 2 verified email
    if i.email_status != "verified" {
        why.push("The email address is not verified.".into());
    }
    // 3 footer and unsubscribe line
    let footer_low = i.footer.to_lowercase();
    if i.footer.trim().is_empty() || !footer_low.contains("unsubscribe") || i.unsubscribe_url.is_empty() || !i.footer.contains(i.unsubscribe_url) {
        why.push("The footer or the unsubscribe line is missing.".into());
    }
    // 4 placeholders and the greeting name
    if draft::has_placeholder(&all) {
        why.push("There is an empty placeholder in the text.".into());
    }
    let first = i.contact_first_name.trim().to_lowercase();
    if !first.is_empty() {
        let greeting = i.body.lines().find(|l| !l.trim().is_empty()).unwrap_or("").to_lowercase();
        if (greeting.starts_with("hi ") || greeting.starts_with("hello ") || greeting.starts_with("dear ")) && !greeting.contains(&first) {
            why.push("The greeting names a different person than the contact.".into());
        }
    }
    // 5 false claims and fake urgency, made-up numbers
    if FAKE_URGENCY.iter().any(|w| all.contains(w)) {
        why.push("The text has fake urgency.".into());
    }
    if all.contains('%') || all.contains("guarantee") || all.split_whitespace().any(|w| w.len() > 1 && w.ends_with('x') && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit())) {
        why.push("The text has a number or claim that is not in the approved facts.".into());
    }
    // 6 mailbox
    if !i.mailbox_active {
        why.push("The mailbox is not active.".into());
    }
    if i.mailbox_sent_today >= i.mailbox_daily_limit {
        why.push("The mailbox reached its daily limit.".into());
    }
    if !i.in_send_window {
        why.push("It is outside the send window.".into());
    }
    // 7 pause-all
    if i.paused {
        why.push("Sending is paused for everyone.".into());
    }
    // 8 spam words and AI tells (same list as the draft checker)
    let problems = draft::check(i.subject, i.body, i.step, &draft::Rules { ban_dashes: i.ban_dashes });
    for p in problems {
        why.push(format!("Writing rule: {p}"));
    }
    // 9 at most one link, only to allowed domains
    let links: Vec<&str> = i.body.split_whitespace().filter(|w| { let l = w.to_lowercase(); l.starts_with("http") || l.starts_with("www.") }).collect();
    if links.len() > 1 {
        why.push("There is more than one link.".into());
    }
    for l in &links {
        let h = host_of(l);
        if !i.allowed_domains.iter().any(|d| !d.is_empty() && (h == *d || h.ends_with(&format!(".{d}")))) {
            why.push(format!("The link to {h} is not on an allowed domain."));
        }
    }
    // 10 phone numbers and money requests
    if has_phone_number(i.body) {
        why.push("The text has a phone number that is not ours.".into());
    }
    if MONEY_ASKS.iter().any(|w| all.contains(w)) {
        why.push("The text asks for money.".into());
    }
    // 11 quoting our own instructions
    if PROMPT_LEAKS.iter().any(|w| all.contains(w)) {
        why.push("The text quotes our own instructions or prompts.".into());
    }
    why.sort();
    why.dedup();
    why
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base<'a>() -> GateInput<'a> {
        GateInput {
            do_not_contact: false, suppressed: false, email_status: "verified", subject: "order entry at your store",
            body: "Hi Sana,\n\nYour store takes orders by phone and message. Many retail teams in Karachi copy orders into a sheet by hand \
                   at the end of each day, which costs time and causes small mistakes in stock counts. We work with data and automation for \
                   retail teams and help them move this work out of sheets. Is order entry handled by hand at your store today?",
            footer: "--\nRahat Hussain, Quickanalytics\nKarachi, Sindh, Pakistan, quickanalytics.ai\nTo unsubscribe reply \"unsubscribe\" or open https://leads.example/u/abc",
            unsubscribe_url: "https://leads.example/u/abc", mailbox_active: true, mailbox_sent_today: 1, mailbox_daily_limit: 10,
            in_send_window: true, paused: false, step: 1, ban_dashes: true,
            allowed_domains: vec!["quickanalytics.ai".into(), "leads.example".into()], contact_first_name: "Sana", is_reply: false,
        }
    }

    #[test]
    fn a_clean_email_passes() {
        let r = check(&base());
        assert!(r.is_empty(), "{:?}", r);
    }

    #[test]
    fn suppressed_unverified_or_paused_are_blocked() {
        let mut g = base();
        g.suppressed = true;
        assert!(check(&g).iter().any(|m| m.contains("unsubscribed")));
        let mut g = base();
        g.email_status = "risky";
        assert!(check(&g).iter().any(|m| m.contains("not verified")));
        let mut g = base();
        g.paused = true;
        assert!(check(&g).iter().any(|m| m.contains("paused")));
    }

    #[test]
    fn footer_and_unsubscribe_are_required() {
        let mut g = base();
        g.footer = "";
        assert!(check(&g).iter().any(|m| m.contains("footer")));
        let mut g = base();
        g.footer = "--\nQuickanalytics";
        assert!(check(&g).iter().any(|m| m.contains("footer")));
    }

    #[test]
    fn mailbox_limits_and_window() {
        let mut g = base();
        g.mailbox_sent_today = 10;
        assert!(check(&g).iter().any(|m| m.contains("daily limit")));
        let mut g = base();
        g.in_send_window = false;
        assert!(check(&g).iter().any(|m| m.contains("send window")));
        let mut g = base();
        g.mailbox_active = false;
        assert!(check(&g).iter().any(|m| m.contains("not active")));
    }

    #[test]
    fn wrong_greeting_name_is_blocked() {
        let mut g = base();
        g.contact_first_name = "Bilal";
        assert!(check(&g).iter().any(|m| m.contains("different person")));
    }

    #[test]
    fn links_only_to_allowed_domains_and_only_one() {
        let mut g = base();
        let b = format!("{} see https://evil.example/x", g.body);
        g.body = Box::leak(b.into_boxed_str());
        g.step = 0;
        assert!(check(&g).iter().any(|m| m.contains("not on an allowed domain")));
        let mut g = base();
        let b = format!("{} see https://quickanalytics.ai and https://quickanalytics.ai/a", g.body);
        g.body = Box::leak(b.into_boxed_str());
        g.step = 0;
        assert!(check(&g).iter().any(|m| m.contains("more than one link")));
    }

    #[test]
    fn fake_urgency_numbers_money_phone_and_prompt_leaks() {
        for (needle, expect) in [
            ("Act now before Friday.", "fake urgency"),
            ("We cut costs by 40% for retailers.", "not in the approved facts"),
            ("Send a bank transfer first.", "asks for money"),
            ("Call +92 300 1234567 today.", "phone number"),
            ("As an AI, my instructions say so.", "own instructions"),
        ] {
            let mut g = base();
            g.step = 0;
            let b = format!("{} {needle}", g.body);
            g.body = Box::leak(b.into_boxed_str());
            assert!(check(&g).iter().any(|m| m.contains(expect)), "{needle}: {:?}", check(&g));
        }
    }

    #[test]
    fn temporary_reasons_wait_instead_of_blocking() {
        let mut g = base();
        g.paused = true;
        g.in_send_window = false;
        assert!(only_temporary(&check(&g)));
        g.suppressed = true;
        assert!(!only_temporary(&check(&g)));
        assert!(!only_temporary(&[]));
    }

    #[test]
    fn writing_rules_are_part_of_the_gate() {
        let mut g = base();
        g.subject = "Great news!";
        assert!(check(&g).iter().any(|m| m.starts_with("Writing rule")));
    }
}
