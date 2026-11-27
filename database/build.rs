// Rebuild when a migration file is added or changed. Without this, `sqlx::migrate!` keeps the old
// list and a new migration file is silently left out of the binary.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
