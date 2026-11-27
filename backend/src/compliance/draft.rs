//! Draft checker: the writing rules of the skill (conversations/email), kept as code.
//! The same rules run on a template, on a draft written by Claude Desktop, and before sending.

pub const AI_TELLS: &[&str] = &[
    "i hope this finds you well", "just following up", "i'd love to", "i would love to", "picture this", "imagine if",
    "what if i told you", "in today's", "reaching out because", "just wanted to", "no pressure", "no obligation",
    "let me know if you're interested", "let me know if you are interested", "quick question",
];
pub const HYPE: &[&str] = &[
    "game-changer", "game changer", "revolutionary", "cutting-edge", "cutting edge", "supercharge", "unlock", "guaranteed",
    "act now", "limited time", "free money", "last chance", "hurry",
];
pub const FLATTERY: &[&str] = &["you've clearly built something real", "solid reputation in your market", "companies like yours"];
/// Email 1 never pitches: no offer, price, demo or booking link.
pub const EMAIL1_BLOCKED: &[&str] = &[
    "free", "demo", "assessment", "discount", "price", "pricing", "quote", "trial", "book a", "calendar", "schedule a", "calendly", "meeting",
];

pub struct Rules {
    pub ban_dashes: bool,
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

fn first_content_line(body: &str) -> &str {
    for line in body.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let low = l.to_lowercase();
        let greeting = (low.starts_with("hi ") || low.starts_with("hello ") || low.starts_with("dear ") || low.starts_with("salam")) && l.ends_with(',');
        if greeting {
            continue;
        }
        return l;
    }
    ""
}

pub fn has_placeholder(s: &str) -> bool {
    s.contains('{') || s.contains('}') || s.contains("[[") || s.contains("<name>") || s.contains("<company>") || s.contains("%name%")
}

pub fn dash_between_words(s: &str) -> bool {
    s.contains('\u{2014}') || s.contains('\u{2013}') || s.contains(" - ") || s.contains(" -- ")
}

pub fn count_links(s: &str) -> usize {
    s.split_whitespace().filter(|w| { let l = w.to_lowercase(); l.starts_with("http://") || l.starts_with("https://") || l.starts_with("www.") }).count()
}

/// `step` is 1, 2 or 3 for sequence emails and 0 for a reply.
pub fn check(subject: &str, body: &str, step: u32, rules: &Rules) -> Vec<String> {
    let mut p: Vec<String> = vec![];
    let low = body.to_lowercase();
    let sub_low = subject.to_lowercase();
    let all = format!("{sub_low}\n{low}");

    if step > 0 {
        if subject.trim().is_empty() {
            p.push("The subject is empty.".into());
        }
        if word_count(subject) > 6 {
            p.push("The subject has more than 6 words.".into());
        }
    }
    if body.trim().is_empty() {
        p.push("The email is empty.".into());
        return p;
    }
    if has_placeholder(&all) {
        p.push("There is an empty placeholder such as {name} in the text.".into());
    }
    if all.contains('!') {
        p.push("Remove exclamation marks.".into());
    }
    if body.matches('?').count() > 1 {
        p.push("Ask only one question.".into());
    }
    if rules.ban_dashes && dash_between_words(&all) {
        p.push("Use a full stop or a comma, not a dash between words.".into());
    }
    for t in AI_TELLS.iter().chain(FLATTERY.iter()) {
        if all.contains(t) {
            p.push(format!("Remove the phrase \"{t}\"."));
        }
    }
    for h in HYPE {
        if all.contains(h) {
            p.push(format!("Remove the hype word \"{h}\"."));
        }
    }
    for line in body.lines() {
        let l = line.trim_start();
        if l.starts_with("- ") || l.starts_with("* ") || l.starts_with('\u{2022}') || l.starts_with("# ") || l.contains("**") {
            p.push("Use plain text only: no bullets, bold or headers.".into());
            break;
        }
    }
    let opener = first_content_line(body).to_lowercase();
    let first = opener.split_whitespace().next().unwrap_or("");
    if matches!(first, "i" | "we" | "i'm" | "i've" | "we're" | "we've" | "our" | "my") {
        p.push("Start with the lead, not with I or We.".into());
    }
    let n = word_count(body);
    match step {
        1 => {
            if !(50..=100).contains(&n) {
                p.push(format!("Email 1 must be 50 to 100 words. It has {n}."));
            }
            for b in EMAIL1_BLOCKED {
                if low.split(|c: char| !c.is_alphanumeric() && c != '\'').any(|w| w == *b) || (b.contains(' ') && low.contains(b)) {
                    p.push(format!("Email 1 never pitches. Remove \"{b}\"."));
                    break;
                }
            }
            if count_links(body) > 0 {
                p.push("Email 1 has no links.".into());
            }
        }
        2 => {
            if !(15..=75).contains(&n) {
                p.push(format!("Email 2 must be 15 to 75 words. It has {n}."));
            }
        }
        3 => {
            if !(10..=60).contains(&n) {
                p.push(format!("Email 3 must be 10 to 60 words. It has {n}."));
            }
        }
        _ => {
            if n > 150 {
                p.push(format!("A reply should be 150 words or fewer. It has {n}."));
            }
        }
    }
    p.sort();
    p.dedup();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rules = Rules { ban_dashes: true };

    fn good1() -> String {
        // 60 words, plain, one question, starts with the lead.
        "Hi Sana,\n\nYour store lists hundreds of products but orders seem to arrive only by phone and message. \
         Many retail teams in Karachi still copy these orders into a sheet by hand at the end of every day, \
         which takes time and causes small mistakes in stock counts. We work with data and automation for \
         retail and e-commerce teams. Is the daily order entry something your team handles by hand today?"
            .to_string()
    }

    #[test]
    fn a_good_first_email_passes() {
        let p = check("order entry at your store", &good1(), 1, &R);
        assert!(p.is_empty(), "{:?}", p);
    }

    #[test]
    fn email_one_never_pitches() {
        let body = good1().replace("data and automation", "free assessment");
        let p = check("order entry", &body, 1, &R);
        assert!(p.iter().any(|m| m.contains("never pitches")), "{:?}", p);
    }

    #[test]
    fn rejects_exclamation_dash_and_two_questions() {
        let body = good1().replace("Is the daily", "Great stuff! Is it fast? Is the daily").replace("by hand today", "by hand \u{2014} today");
        let p = check("order entry", &body, 1, &R);
        assert!(p.iter().any(|m| m.contains("exclamation")));
        assert!(p.iter().any(|m| m.contains("one question")));
        assert!(p.iter().any(|m| m.contains("dash")));
    }

    #[test]
    fn dash_rule_can_be_switched_off() {
        let body = good1().replace("by hand today", "by hand \u{2014} today");
        assert!(!check("order entry", &body, 1, &Rules { ban_dashes: false }).iter().any(|m| m.contains("dash")));
    }

    #[test]
    fn rejects_ai_tells_and_hype() {
        let body = good1().replace("Your store", "I hope this finds you well. Your store").replace("small mistakes", "a game-changer");
        let p = check("order entry", &body, 1, &R);
        assert!(p.iter().any(|m| m.contains("i hope this finds you well")));
        assert!(p.iter().any(|m| m.contains("game-changer")));
    }

    #[test]
    fn subject_rules() {
        let p = check("one two three four five six seven", &good1(), 1, &R);
        assert!(p.iter().any(|m| m.contains("6 words")));
        assert!(check("", &good1(), 1, &R).iter().any(|m| m.contains("subject is empty")));
        assert!(check("quick question", &good1(), 1, &R).iter().any(|m| m.contains("quick question")));
    }

    #[test]
    fn starts_with_lead_not_i_or_we() {
        let body = good1().replace("Your store lists", "We saw that your store lists");
        assert!(check("order entry", &body, 1, &R).iter().any(|m| m.contains("Start with the lead")));
    }

    #[test]
    fn placeholders_are_caught() {
        let body = good1().replace("Sana", "{first_name}");
        assert!(check("order entry", &body, 1, &R).iter().any(|m| m.contains("placeholder")));
    }

    #[test]
    fn word_ceilings() {
        assert!(check("s", "short text only.", 1, &R).iter().any(|m| m.contains("50 to 100")));
        let long = "word ".repeat(120);
        assert!(check("s", &long, 1, &R).iter().any(|m| m.contains("50 to 100")));
        assert!(check("s", &"word ".repeat(80), 2, &R).iter().any(|m| m.contains("15 to 75")));
        assert!(check("s", &"word ".repeat(70), 3, &R).iter().any(|m| m.contains("10 to 60")));
        assert!(check("", &"word ".repeat(160), 0, &R).iter().any(|m| m.contains("150 words")));
    }

    #[test]
    fn email_one_has_no_link() {
        let body = format!("{} See https://example.com now", good1());
        assert!(check("order entry", &body, 1, &R).iter().any(|m| m.contains("no links")));
    }

    #[test]
    fn bullets_are_refused() {
        let body = format!("{}\n- one\n- two", good1());
        assert!(check("order entry", &body, 1, &R).iter().any(|m| m.contains("plain text")));
    }

    #[test]
    fn empty_body() {
        assert!(!check("s", "  ", 1, &R).is_empty());
    }
}
