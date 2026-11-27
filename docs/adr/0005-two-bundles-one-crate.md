# 0005 Two bundles from one crate

Status: accepted, with a known gap.

`frontend` is built twice: `--features public` (sign-in, legal pages, unsubscribe) and `--features app` (signed in). `scripts/build_wasm.sh` writes `dist/public` and `dist/app`. The gateway gives the public bundle to visitors and the app bundle only to a signed-in session.

Gap: both bundles still contain the same code, so both are about 1.24 MB with Brotli. The public bundle only differs in what it starts. Making it smaller means putting the app pages behind `cfg(feature = "app")`. That is not done. See `docs/honest-status.md`.
