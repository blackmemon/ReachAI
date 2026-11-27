//! Every business rule is a setting with a default and a Reset button.
//! Defaults come from the leads skill (settings/general, lead-scoring, compliance, timers).

pub struct SettingDef {
    pub key: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// text, longtext, number, bool, choice
    pub kind: &'static str,
    pub default: &'static str,
    pub choices: &'static [&'static str],
    /// Only an admin may change it.
    pub admin_only: bool,
}

macro_rules! s {
    ($k:expr, $g:expr, $l:expr, $h:expr, $kind:expr, $d:expr) => {
        SettingDef {
            key: $k,
            group: $g,
            label: $l,
            help: $h,
            kind: $kind,
            default: $d,
            choices: &[],
            admin_only: false,
        }
    };
    ($k:expr, $g:expr, $l:expr, $h:expr, $kind:expr, $d:expr, admin) => {
        SettingDef {
            key: $k,
            group: $g,
            label: $l,
            help: $h,
            kind: $kind,
            default: $d,
            choices: &[],
            admin_only: true,
        }
    };
}

pub const GROUPS: &[(&str, &str)] = &[
    ("general", "General"),
    ("knowledge", "Company knowledge"),
    ("offers", "Offers and closing"),
    ("ai", "AI connection"),
    ("assignment", "Lead assignment"),
    ("webhooks", "Webhooks"),
];

pub const SETTINGS: &[SettingDef] = &[
    // General
    s!("target_country", "general", "Target country", "Leads are searched in this country. Another country needs an admin.", "text", "Pakistan"),
    s!("sending_paused", "general", "Pause all sending", "Stops every mailbox at once. Emails already sent are not recalled.", "bool", "false", admin),
    s!("auto_send", "general", "Auto-send", "On: emails go out after the safety checks. Off: every email waits for approval.", "bool", "true", admin),
    s!("auto_reply", "general", "AI replies", "On: the AI may reply when no employee replied in time and it is safe.", "bool", "true", admin),
    s!("employee_timer_minutes", "general", "Employee reply time (minutes)", "An employee answers first. After this time the AI may reply.", "number", "15"),
    s!("urgent_timer_minutes", "general", "Urgent reply time (minutes)", "Shorter time for urgent leads.", "number", "5"),
    s!("start_daily_limit", "general", "Daily emails per mailbox at start", "Free Gmail has low limits. Start low and raise slowly.", "number", "10"),
    s!("max_daily_limit", "general", "Highest daily emails per mailbox", "A mailbox never sends more than this.", "number", "30"),
    s!("send_from_hour", "general", "Send window starts (hour, Karachi)", "Emails are sent only inside the window.", "number", "9"),
    s!("send_to_hour", "general", "Send window ends (hour, Karachi)", "Emails are sent only inside the window.", "number", "17"),
    s!("score_threshold", "general", "Qualified score", "A lead with this score or more is qualified for email.", "number", "60"),
    s!("sample_check_done", "general", "Sample check done", "Until the owner has checked 5 researched companies, research is capped at 10 and campaigns cannot start.", "bool", "false", admin),
    s!("sample_cap", "general", "Research cap before the sample check", "Number of companies research may save before the sample check.", "number", "10"),
    s!("ooo_wait_days", "general", "Wait after out-of-office (days)", "Days a sequence waits when the lead is out of office.", "number", "3"),
    s!("score_industry", "general", "Score points: industry fit", "Full points for the industries on the company website.", "number", "25"),
    s!("score_role", "general", "Score points: role fit", "", "number", "20"),
    s!("score_size", "general", "Score points: company size and activity", "", "number", "15"),
    s!("score_email", "general", "Score points: verified email", "", "number", "15"),
    s!("score_need", "general", "Score points: website shows a data or AI need", "", "number", "15"),
    s!("score_location", "general", "Score points: located in the target country", "", "number", "10"),
    s!("ban_dashes", "general", "Writing rule: no dash between words", "ASSUMED. The owner can change it.", "bool", "true"),
    // Company knowledge
    s!("company_name", "knowledge", "Company name", "Used in the email footer.", "text", "Quickanalytics"),
    s!("company_site", "knowledge", "Website", "The AI may link only to this site, the booking page or the lead's own domain.", "text", "quickanalytics.ai"),
    s!("company_address", "knowledge", "Footer address", "No street address is available. Add one before emailing US leads.", "text", "Karachi, Sindh, Pakistan"),
    s!("sender_name", "knowledge", "Sender name", "The real person the email comes from.", "text", "Rahat Hussain"),
    s!("services", "knowledge", "Approved services", "The AI names only these. It never invents prices, clients or results.", "longtext",
        "AI Consulting\nBusiness Process Automation\nAI & Systems Integration\nAI Agent Development\nCustom AI Development\nData, Analytics & AI Insight"),
    s!("industries", "knowledge", "Industries", "The eight industries on the company website.", "longtext",
        "Retail and e-commerce\nFinancial services\nHealthcare\nLogistics and supply chain\nReal estate\nManufacturing\nProfessional services\nHospitality"),
    s!("approved_facts", "knowledge", "Other approved facts", "Add only facts that are true and approved. There are no case studies, prices or street address yet.", "longtext", ""),
    // Offers and closing
    s!("entry_offer", "offers", "Entry offer", "Offered only after real interest. Email 1 never pitches.", "text", "A free 30-minute AI assessment call"),
    s!("booking_url", "offers", "Booking link (Google Calendar)", "The free 30-minute call on Google Meet is booked here.", "text", ""),
    s!("meeting_host", "offers", "Meeting host", "Takes the meetings. Head of department.", "text", "Rahat Hussain"),
    s!("meeting_minutes", "offers", "Meeting length (minutes)", "", "number", "30"),
    s!("price_policy", "offers", "Price policy", "ASSUMED. The website publishes no prices, so the AI never states a price and hands over.", "text", "Hand over to an employee"),
    s!("nudge_once", "offers", "One gentle nudge after interest goes quiet", "Never more than one.", "bool", "true"),
    // AI connection
    s!("ai_check_minutes", "ai", "Expected Claude Desktop check interval (minutes)", "The app shows on time, late or never checked.", "number", "5"),
    s!("ai_cli_engine", "ai", "Claude Code CLI engine", "Off by default. Used only for AI Research and the Email test lab.", "bool", "false", admin),
    s!("mcp_enabled", "ai", "MCP server", "Claude Desktop connects to this app only through the MCP server.", "bool", "true", admin),
    s!("web_guard", "ai", "Flag AI research runs with no web use", "Candidates from such a run need a confirmation page.", "bool", "true", admin),
    // Lead assignment
    s!("assign_mode", "assignment", "Assignment", "How new leads are given to employees.", "choice", "round_robin"),
    s!("assign_max_open", "assignment", "Most open leads per employee", "", "number", "50"),
    s!("assign_territory", "assignment", "Use city territories", "Match the lead city to an employee.", "bool", "false"),
    // Webhooks
    s!("webhook_signing", "webhooks", "Sign outgoing webhooks", "Every call carries a signature header.", "bool", "true", admin),
    s!("webhook_events", "webhooks", "Events sent", "One event name per line.", "longtext", "reply.received\nhandover.created\nemail.blocked\ntask.overdue", admin),
];

pub const ASSIGN_CHOICES: &[&str] = &["round_robin", "least_open", "manual"];

pub fn find(key: &str) -> Option<&'static SettingDef> {
    SETTINGS.iter().find(|s| s.key == key)
}

pub fn choices_for(def: &SettingDef) -> Vec<String> {
    if def.key == "assign_mode" {
        ASSIGN_CHOICES.iter().map(|s| s.to_string()).collect()
    } else {
        def.choices.iter().map(|s| s.to_string()).collect()
    }
}

/// Check a value against the setting type. Returns an error message for the user.
pub fn validate(def: &SettingDef, value: &str) -> Result<(), String> {
    match def.kind {
        "number" => value
            .trim()
            .parse::<i64>()
            .map(|n| {
                if n < 0 {
                    Err("Use a number that is zero or more.".to_string())
                } else {
                    Ok(())
                }
            })
            .unwrap_or_else(|_| Err("Use a whole number.".to_string())),
        "bool" => {
            if value == "true" || value == "false" {
                Ok(())
            } else {
                Err("Use on or off.".to_string())
            }
        }
        "choice" => {
            if choices_for(def).iter().any(|c| c == value) {
                Ok(())
            } else {
                Err("Pick one of the listed choices.".to_string())
            }
        }
        _ => {
            if value.len() > 4000 {
                Err("This text is too long.".to_string())
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique() {
        let mut k: Vec<_> = SETTINGS.iter().map(|s| s.key).collect();
        let n = k.len();
        k.sort();
        k.dedup();
        assert_eq!(n, k.len());
    }

    #[test]
    fn every_setting_has_a_known_group_and_valid_default() {
        for s in SETTINGS {
            assert!(GROUPS.iter().any(|(g, _)| *g == s.group), "{}", s.key);
            assert!(validate(s, s.default).is_ok(), "default of {}", s.key);
        }
    }

    #[test]
    fn number_validation() {
        let d = find("score_threshold").unwrap();
        assert!(validate(d, "60").is_ok());
        assert!(validate(d, "-1").is_err());
        assert!(validate(d, "abc").is_err());
    }
}
