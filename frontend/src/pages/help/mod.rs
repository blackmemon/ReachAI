use crate::components::*;
use crate::theme::sp;
use iced::widget::column;
use iced::Element;

const FAQ: &[(&str, &str)] = &[
    ("How does outreach work?", "The app finds companies from public sources, scores them, and writes a three email sequence: day 0, day 3 and day 7. The first email never pitches. A reply, a bounce or an unsubscribe stops the sequence."),
    ("Who answers replies?", "A person answers first. If nobody replies within 15 minutes (5 for urgent ones), Claude Desktop may answer through the AI queue, only when the safety checks pass."),
    ("What stops an email from going out?", "The safety checks before every email: do not contact list, unsubscribe, daily limit, send window, writing rules and the sample check. A blocked email shows the reason."),
    ("How do I stop all sending?", "Press Pause all sending in the top bar. It stops every mailbox at once. Emails already sent are not recalled."),
    ("Is anything deleted?", "No. People who ask to stop go on the do not contact list. The activity log can only grow."),
    ("Why can I not start a campaign?", "Until the owner marks 5 researched companies as good in Research, campaigns cannot start and research stops at 10 companies."),
    ("How do I connect Claude Desktop?", "Open Settings, then AI connection, create a token, and add the MCP address and token in Claude Desktop. The page shows when Claude last checked the queue."),
];

pub fn view<'a, M: 'a>() -> Element<'a, M> {
    let mut col = column![].spacing(sp::S4);
    for (q, a) in FAQ {
        col = col.push(column![heading(q.to_string()), body(a.to_string())].spacing(sp::S1));
    }
    column![page_header("Help", "Short answers to common questions.", vec![]), card(col)].spacing(sp::S4).into()
}
