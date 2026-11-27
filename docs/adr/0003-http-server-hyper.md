# 0003 HTTP server: hyper, no framework

Status: accepted. The standard bans Axum. The gateway uses `hyper` 1.x with `tokio`. Routing is a plain match on the path segments in `gateway/src/api/mod.rs`. Limits, headers, CSRF and cookies are written once in `gateway/src/http` and `gateway/src/server`.
