# 0011 AI research as conversations that run in the background

Status: accepted.

The first version of AI Leads called Claude CLI inside one HTTP request. The browser waited up to 100 seconds with no progress, nothing was remembered between messages, and a refresh lost the answer.

Now every message starts a run in a background task of the gateway process (`backend/src/research/session.rs`):

- Tables `research_sessions`, `research_messages`, `research_session_runs`, `research_events`, `research_results` (migration 0005). A conversation belongs to one person and one mode (`leads` or `competitors`); the two modes share the screen and keep separate conversations. Row level security fences every table by company, like the rest.
- The CLI runs with `--output-format stream-json --verbose`. Its `WebSearch` and `WebFetch` calls become events (the step, the page being analysed). Lines the model writes that start with "Found:" become "spotted" events. The screen polls `GET /research/runs/{id}?after=N` once a second.
- At the end the structured result is checked one business at a time: invalid emails, phone numbers and websites are dropped (never guessed), the website's domain is looked up (a dead domain lowers "verified" to "partial"), duplicates are dropped (same site domain, else same name and place), and the gaps are reported in the answer and on each record.
- The answer message is saved before the run is marked done, because the screen reloads the conversation as soon as it sees "done".
- One run per conversation (a partial unique index) and two at once for the whole server. A run left "running" by a restart is marked failed the next time it is looked at. Stop kills the CLI within two seconds.
- Results are not people or companies until a person selects them and presses "Send to AI Leads review" or "Mark as competitors". That keeps the existing review list and the sample-check cap as the gate.
- Export: CSV (UTF-8 with a byte order mark) and a real `.xlsx` written by hand (a zip with uncompressed entries, no new crate). Cells that start like a spreadsheet formula are kept as text.
- The old synchronous `POST /research/chat` was removed.

Not done: a conversation cannot be renamed or deleted; progress is polled, not pushed; with the gateway in several processes a run is tied to the process that started it.
