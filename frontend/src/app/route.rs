//! Addresses. One list for the browser address and the page that opens.

use shared::nav;

#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Today,
    SignIn,
    Privacy,
    Terms,
    Cookies,
    Unsubscribe(String),
    Quick(Option<String>),
    Settings(Option<String>),
    Reports(Option<String>),
    Research(Option<String>),
    Notices,
    Profile,
    Help,
    /// A table page from the module list, with an optional ready-made search.
    Table(String),
    NotFound,
}

/// Pages that are table pages, with the module they show. Some nav keys reuse a module.
pub fn table_module(key: &str) -> Option<&'static str> {
    Some(match key {
        "tasks" => "tasks",
        "activity" => "activity",
        "people" | "new_contact" => "people",
        "companies" => "companies",
        "lists" => "lists",
        "enrichment" => "people",
        "emails" | "schedule" | "inbox" => "emails",
        "templates" => "templates",
        "domain_health" | "mailboxes" => "mailboxes",
        "calls" => "calls",
        "conversations" => "conversations",
        "meetings" => "meetings",
        "deals" => "deals",
        "follow_ups" => "follow_ups",
        "goals" => "goals",
        "campaigns" => "campaigns",
        "sequences" => "sequences",
        "audiences" => "audiences",
        _ => return None,
    })
}

pub fn parse(path: &str) -> Route {
    let p = path.split('?').next().unwrap_or("").trim_matches('/');
    let mut it = p.split('/');
    let first = it.next().unwrap_or("");
    let second = it.next().filter(|s| !s.is_empty()).map(String::from);
    match first {
        "" | "today" => Route::Today,
        "sign-in" => Route::SignIn,
        "privacy" => Route::Privacy,
        "terms" => Route::Terms,
        "cookies" => Route::Cookies,
        "unsubscribe" => match second {
            Some(t) => Route::Unsubscribe(t),
            None => Route::NotFound,
        },
        "quick" => Route::Quick(second),
        "settings" => Route::Settings(second),
        "reports" => Route::Reports(second.map(|s| s.replace("%20", " "))),
        "research" => Route::Research(second),
        "notices" => Route::Notices,
        "profile" => Route::Profile,
        "help" => Route::Help,
        "performance" => Route::Reports(Some("Campaigns".into())),
        k if table_module(k).is_some() => Route::Table(k.to_string()),
        _ => Route::NotFound,
    }
}

pub fn path(r: &Route) -> String {
    match r {
        Route::Today => "/".into(),
        Route::SignIn => "/sign-in".into(),
        Route::Privacy => "/privacy".into(),
        Route::Terms => "/terms".into(),
        Route::Cookies => "/cookies".into(),
        Route::Unsubscribe(t) => format!("/unsubscribe/{t}"),
        Route::Quick(t) => t.as_ref().map(|t| format!("/quick/{t}")).unwrap_or_else(|| "/quick".into()),
        Route::Settings(g) => g.as_ref().map(|g| format!("/settings/{g}")).unwrap_or_else(|| "/settings".into()),
        Route::Reports(t) => t.as_ref().map(|t| format!("/reports/{}", t.replace(' ', "%20"))).unwrap_or_else(|| "/reports".into()),
        Route::Research(t) => t.as_ref().map(|t| format!("/research/{t}")).unwrap_or_else(|| "/research".into()),
        Route::Notices => "/notices".into(),
        Route::Profile => "/profile".into(),
        Route::Help => "/help".into(),
        Route::Table(k) => format!("/{k}"),
        Route::NotFound => "/not-found".into(),
    }
}

/// Which sidebar item is lit for a route.
pub fn nav_key(r: &Route) -> &'static str {
    match r {
        Route::Today => "today",
        Route::Quick(_) => "quick",
        Route::Settings(_) => "settings",
        Route::Reports(_) => "reports",
        Route::Research(_) => "research",
        Route::Help => "help",
        Route::Table(k) => nav::all_route_keys().into_iter().find(|n| n == k).unwrap_or("today"),
        _ => "",
    }
}

pub fn is_public(r: &Route) -> bool {
    matches!(r, Route::SignIn | Route::Privacy | Route::Terms | Route::Cookies | Route::Unsubscribe(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_round_trip() {
        for r in [
            Route::Today, Route::SignIn, Route::Privacy, Route::Quick(Some("queue".into())), Route::Settings(Some("general".into())),
            Route::Reports(Some("AI usage".into())), Route::Table("people".into()), Route::Notices, Route::Unsubscribe("a.b.c".into()),
        ] {
            assert_eq!(parse(&path(&r)), r, "{r:?}");
        }
    }

    #[test]
    fn every_sidebar_item_opens_something() {
        for g in nav::NAV {
            for i in g.items {
                assert_ne!(parse(&format!("/{}", i.key)), Route::NotFound, "{}", i.key);
            }
        }
    }

    #[test]
    fn unknown_address_is_not_found() {
        assert_eq!(parse("/nope"), Route::NotFound);
        assert_eq!(parse("/unsubscribe"), Route::NotFound);
    }

    #[test]
    fn public_pages_are_marked() {
        assert!(is_public(&Route::Privacy) && !is_public(&Route::Today));
    }
}
