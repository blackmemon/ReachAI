//! Privacy notice, terms and cookie notice. Plain English. Short on purpose.

use crate::components::*;
use crate::theme::sp;
use iced::widget::{column, container};
use iced::{Element, Length};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Privacy,
    Terms,
    Cookies,
}

fn section<'a, M: 'a>(title: &str, text: &str) -> Element<'a, M> {
    column![heading(title.to_string()), body(text.to_string())].spacing(sp::S1).into()
}

pub fn view<'a, M: 'a>(kind: Kind) -> Element<'a, M> {
    let (title, parts): (&str, Vec<(&str, &str)>) = match kind {
        Kind::Privacy => ("Privacy notice", vec![
            ("Who we are", "blackmemon ReachAI is a sales outreach tool. It is run by the company that gave you your sign-in (the account owner). That company decides why and how data in the tool is used."),
            ("What is stored", "Sign-in details for staff (name, work email, a password stored only as a hash). Business contacts found in public sources or added by staff (name, work email, job title, company, city, notes). Emails sent and received through the tool. A log of who did what."),
            ("Why", "To find companies that may need our services, to send a short, honest email, to follow up, and to keep a record of the conversation."),
            ("Where the data comes from", "Public sources such as OpenStreetMap, open datasets, files that staff upload, and website lists. LinkedIn is not scraped. Protected sites are not forced."),
            ("How long", "Records are kept for as long as the account is open. Nothing is deleted by the tool: people who ask to stop are put on a do not contact list so they are never emailed again."),
            ("Your choices", "Every email has an unsubscribe link. You can also reply and ask us to stop. To see, correct or remove your data, write to the sender of the email or to the account owner."),
            ("Security", "Connections use HTTPS. Passwords are hashed. Each company sees only its own data. Staff see only what their role allows."),
        ]),
        Kind::Terms => ("Terms of use", vec![
            ("Use of the tool", "Only people invited by an account admin may sign in. There is no public sign-up. Keep your password private and sign out on shared computers."),
            ("Allowed use", "Send honest, relevant business email to business addresses. Do not send spam, mislead anyone, or collect data from sources that forbid it."),
            ("Unsubscribe", "A request to stop is always honoured. The tool will not email an address that is on the do not contact list."),
            ("Our work", "The tool follows safety checks before every email. These checks help, but the account owner stays responsible for what is sent."),
            ("Availability", "We aim to keep the tool running but do not promise it will never stop. Emails already sent cannot be recalled."),
        ]),
        Kind::Cookies => ("Cookie notice", vec![
            ("What we set", "Two small cookies, both needed for the tool to work: one keeps you signed in, the other protects sign-in forms from forgery."),
            ("What we do not set", "No advertising cookies, no analytics cookies, and nothing from other websites."),
            ("How long", "The sign-in cookie ends when you sign out, after 20 minutes without use, or 24 hours after you signed in, whichever comes first."),
        ]),
    };
    let mut col = column![page_title(title.to_string())].spacing(sp::S4);
    for (t, p) in parts {
        col = col.push(section(t, p));
    }
    col = col.push(muted("Last updated 21 Nov 2026."));
    container(container(card(col).padding(sp::S5)).max_width(crate::theme::dim::FORM_MAX).width(Length::Fill)).width(Length::Fill).center_x(Length::Fill).into()
}
