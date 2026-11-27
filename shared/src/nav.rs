//! Navigation: nine groups as in the attached design (sidebar). One list for UI and server.

pub struct NavItem {
    pub key: &'static str,
    pub label: &'static str,
    /// Lucide-style icon name drawn by the frontend icon component.
    pub icon: &'static str,
}

pub struct NavGroup {
    pub label: &'static str,
    pub items: &'static [NavItem],
}

macro_rules! item {
    ($k:expr, $l:expr, $i:expr) => {
        NavItem {
            key: $k,
            label: $l,
            icon: $i,
        }
    };
}

pub const NAV: &[NavGroup] = &[
    NavGroup {
        label: "Overview",
        items: &[
            item!("today", "Today", "home"),
            item!("tasks", "Tasks", "check"),
            item!("activity", "Activity", "activity"),
            item!("quick", "Quick", "zap"),
        ],
    },
    NavGroup {
        label: "Contacts",
        items: &[
            item!("people", "People", "users"),
            item!("companies", "Companies", "building"),
            item!("lists", "Lists", "list"),
            item!("enrichment", "Enrichment", "sparkles"),
        ],
    },
    NavGroup {
        label: "Outreach",
        items: &[
            item!("emails", "Emails", "mail"),
            item!("templates", "Templates", "file"),
            item!("domain_health", "Domain health", "shield"),
            item!("calls", "Calls", "phone"),
        ],
    },
    NavGroup {
        label: "Sales",
        items: &[
            item!("conversations", "Conversations", "chat"),
            item!("meetings", "Meetings", "calendar"),
            item!("deals", "Deals", "coins"),
            item!("follow_ups", "Follow-ups", "clock"),
            item!("goals", "Goals", "target"),
        ],
    },
    NavGroup {
        label: "Research",
        items: &[item!("research", "AI Leads", "search")],
    },
    NavGroup {
        label: "Campaigns",
        items: &[
            item!("campaigns", "Campaigns", "send"),
            item!("sequences", "Sequences", "layers"),
            item!("audiences", "Audiences", "filter"),
            item!("schedule", "Schedule", "calendar"),
            item!("performance", "Performance", "chart"),
        ],
    },
    NavGroup {
        label: "Reports",
        items: &[item!("reports", "Reports", "chart")],
    },
    NavGroup {
        label: "Settings",
        items: &[item!("settings", "Settings", "gear")],
    },
    NavGroup {
        label: "Help",
        items: &[item!("help", "Help", "help")],
    },
];

/// Every page key that exists, including pages not in the sidebar.
pub fn all_route_keys() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = NAV
        .iter()
        .flat_map(|g| g.items.iter().map(|i| i.key))
        .collect();
    v.extend([
        "quick_chat",
        "quick_queue",
        "quick_history",
        "mailboxes",
        "inbox",
        "notices",
        "profile",
        "settings_general",
        "settings_knowledge",
        "settings_offers",
        "settings_ai",
        "settings_assignment",
        "settings_webhooks",
        "sign_in",
        "new_contact",
    ]);
    v
}

pub fn label_for(key: &str) -> &'static str {
    for g in NAV {
        for i in g.items {
            if i.key == key {
                return i.label;
            }
        }
    }
    match key {
        "quick_chat" => "Quick chat",
        "quick_queue" => "AI queue",
        "quick_history" => "Agent history",
        "mailboxes" => "Mailboxes",
        "inbox" => "Mailbox",
        "notices" => "Notices",
        "profile" => "My profile",
        "settings_general" => "General",
        "settings_knowledge" => "Company knowledge",
        "settings_offers" => "Offers and closing",
        "settings_ai" => "AI connection",
        "settings_assignment" => "Lead assignment",
        "settings_webhooks" => "Webhooks",
        "new_contact" => "New contact",
        _ => "Page",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_groups_like_the_design() {
        assert_eq!(NAV.len(), 9);
    }

    #[test]
    fn route_keys_are_unique() {
        let mut v = all_route_keys();
        let n = v.len();
        v.sort();
        v.dedup();
        assert_eq!(n, v.len());
    }
}
