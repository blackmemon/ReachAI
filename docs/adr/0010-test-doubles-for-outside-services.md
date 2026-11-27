# 0010 Mail and DNS are test doubles

Status: accepted. Live Gmail (SMTP and IMAP), live OpenStreetMap calls and live DNS checks are not built. The sender is `Transport::Recording` (keeps messages in memory) and the domain check is `DomainCheck::Fake`. The rules around them (send gate, limits, windows, unsubscribe, pause-all) are real and tested. `LEADS_SMTP_LIVE` is read but no live sender exists.
