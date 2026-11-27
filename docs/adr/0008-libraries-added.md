# 0008 Libraries outside the standard list

Status: accepted.

- `argon2`: password hashing.
- `hmac`, `sha2`: signed cookie and unsubscribe tokens, hash chain of the activity log.
- `csv`: CSV import and export.
- `reqwest` (wasm only, no default features): HTTP calls from the browser build.
- `rfd` (wasm only): the CSV file picker. The feature `xdg-portal` is switched on only because the crate's build script refuses to run without a backend feature. It is not compiled into the wasm build.
- `web-sys`, `js-sys`, `wasm-bindgen-futures`, `console_error_panic_hook`: browser facts and the start function.
- `tracing`, `tracing-subscriber`: server logs.
