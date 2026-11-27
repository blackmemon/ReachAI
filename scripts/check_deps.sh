#!/usr/bin/env bash
# Technology rules from the standard. Fails on a forbidden crate, a hand-written web file,
# or an implementation crate inside `shared`.
set -uo pipefail
cd "$(dirname "$0")/.."
fail=0
tree="$(cargo tree --workspace --prefix none --edges normal,build 2>/dev/null | awk '{print $1}' | sort -u)"
for bad in axum egui eframe tantivy leptos yew trunk dioxus tauri wry gtk actix-web rocket warp tide redis duckdb sqlite3 rusqlite; do
  if echo "$tree" | grep -qx "$bad"; then echo "FAIL forbidden crate in the build: $bad"; fail=1; fi
done
# Hand-written HTML, CSS and JavaScript (generated files in dist/ and build output are fine).
found="$(find . -type f \( -name '*.html' -o -name '*.css' -o -name '*.js' -o -name '*.ts' -o -name '*.jsx' -o -name '*.tsx' \) -not -path './dist/*' -not -path './target/*' -not -path './.git/*' -not -path './vendor/*' | head)"
if [ -n "$found" ]; then echo "FAIL hand-written web files:"; echo "$found"; fail=1; fi
# Docker is not allowed.
if find . -iname 'Dockerfile*' -o -iname 'docker-compose*' | grep -q .; then echo "FAIL Docker file found"; fail=1; fi
# `shared` may only hold types: no implementation crates.
for dep in tokio sqlx hyper reqwest iced argon2 backend gateway database; do
  if grep -E "^\s*$dep\s*(=|\.)" shared/Cargo.toml >/dev/null 2>&1; then echo "FAIL shared depends on $dep"; fail=1; fi
done
if [ "$fail" = 0 ]; then echo "check_deps: OK"; fi
exit $fail
