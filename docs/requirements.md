# Requirements: blackmemon ReachAI

Product: an AI sales outreach and lead generation web app for Quickanalytics (Karachi, Pakistan). Source of the rules: the leads skill (`ai-sales-outreach-lead-generation-platform`). Technology: the Rust skill. Look: the owner's design file (ADR 0009).

## What it does
1. Finds leads from public sources and scores them. Research results wait as candidates until a person checks them.
2. Writes a three email sequence (day 0, day 3, day 7). Email 1 never pitches. Every draft passes the writing checks.
3. Sends only after the safety checks: do not contact list, unsubscribe, daily limit (10 to 30 per mailbox), send window, sample check.
4. Handles replies: an employee answers first (15 minutes, 5 if urgent). The AI answers only through the Claude Desktop MCP queue.
5. Keeps a CRM: people, companies, lists, deals, tasks, meetings, goals.
6. Pause all sending stops everything at once. Unsubscribe is always honoured. Nothing is deleted. The activity log is append only and hash chained.

## People and Companies fields
The People screen contains Name, Job Title, Company, Emails, Phone Numbers, Links, Location, and Keywords. The Companies screen contains Name, Emails, Phone Numbers, Links, Number of Employees, Industries, Keywords, and Location. Workflow and operational fields remain internal and are not shown on these screens.

Emails, phone numbers, links, industries, and keywords accept multiple values, one per line. A person's first email remains the outreach address. A company's first link and first industry continue to populate the existing website and industry fields used by other workflows. Email, phone, links, and employee-count filters are omitted from People and Companies. Job Title, Company, Location, and Keywords accept multiple filter values: any selected value can match, while separate filters are combined.

Email and Conversations use split-pane inbox/chat layouts. Email filters include All, Read, and Unread; read state and chat archive/read state are per user. Tenant-shared custom labels organize emails and chats. Compose can queue messages only to an existing contact, and every message passes the existing send checks and worker queue. The current mail transport is a recording test double and does not deliver external mail.

Chats support one-to-one and multi-participant email conversations. Quick has AI Chat, AI Usage, and History tabs; the chat remains the existing rules-based assistant with larger requests queued for Claude Desktop. Research has AI Leads, Competitors, and Data Collections tabs backed by candidates, marked competitor companies, and the existing sources/collection-run records.

## Fixed defaults used (not asked)
Multi-tenant (each company sees only its own data), 1 million concurrent users as a target, login ends 24 hours after sign-in (idle 20 minutes), English only, times in UTC and shown in the device time zone, WCAG 2.2 AA as the goal, privacy notice, terms and cookie notice pages, CSV only for import and export, no Docker, no hand-written JavaScript, audit log kept for all time.

## Decisions made without asking (reasons in `docs/adr/`)
PostgreSQL (ADR 0002, ASSUMED), hyper without a framework, tiny-skia renderer, sessions and jobs in PostgreSQL, mail and DNS as test doubles.

## Laws that may apply (not legal advice; a lawyer must check)
Users are assumed to be in every country, so: Pakistan PECA 2016 and the planned Personal Data Protection law, EU GDPR and ePrivacy for any EU contact, UK GDPR and PECR, CAN-SPAM (US), CASL (Canada). Cold email to business addresses is allowed in some places and not in others. The app always offers unsubscribe, keeps a do not contact list and stores where each lead came from. PCI DSS does not apply (no card payments). Health laws do not apply (no health data).

Retention risk: records are kept for all time (owner rule). Some countries give people a right to deletion. The app does not delete; it stops contact and hides nothing. Purpose for each kind of data: staff sign-in (access), business contacts (outreach and follow-up), emails (record of the conversation), activity log (proof of what was done).

Accepted backup gap: a daily full backup, 7 days kept, up to 24 hours of data can be lost. Backups are not built in this release (see honest status).

## Open items

### Kya kam hai (what is missing from the requirements)
1. Company kaise join hoti hai aur kaise chhorti hai? Abhi sirf deployer `create-tenant` command se company banata hai. Chhorne ka koi tareeqa nahi hai (kuch delete nahi hota).
2. Gmail App Password ka tareeqa: har mailbox ka password kahan rakha jaye aur kaun badle? Abhi live Gmail jora nahi gaya.
3. Support staff ko ek case ka data dekhne ka tareeqa abhi requirements mein nahi likha.
4. Domain, SPF, DKIM, DMARC kaun set karega aur kis domain se email jayegi? Isko abhi koi nahi janta.
5. Claude Desktop ka scheduled task kaun banayega aur kitne minute baad chalega? (Sab se kam 1 ghanta.)
6. Backup aur restore ka server kahan hoga? Abhi sirf plan hai, drill nahi hui.
7. Public website (search engine ke liye) ke pages: abhi sirf sign-in, privacy, terms, cookies, unsubscribe hain.

### Sawal (questions for the owner)
1. Database PostgreSQL theek hai ya SQLite? Wajah: tenant isolation aur job queue PostgreSQL par bani hain. Recommended: PostgreSQL. (ASSUMED)
2. Admin panel ka look A rakhen (sidebar, top bar, cards, table, side drawer)? Wajah: aap ki design file isi par hai. Recommended: Look A. (ASSUMED)
3. Public pages ko search engine mein dikhana hai? Wajah: Iced canvas par pages ka text search engine nahi parh sakta. Recommended: sirf sign-in aur legal pages, search engine ke liye nahi. (ASSUMED)
4. Screen reader ka kya karna hai? Wajah: canvas par screen reader abhi kaam nahi karta (honest status mein likha hai). Recommended: ek alag saada HTML tareeqa baad mein, ya sirf keyboard aur zoom ka wada. (ASSUMED)
5. Company ka mulk kaun sa hai (qanoon ke liye)? Recommended: Pakistan. (ASSUMED)
6. Conversations aur Emails ke liye design file ka chat aur inbox layout banayen? Wajah: abhi ye common table aur drawer se chalte hain. Recommended: haan, agli update mein.
