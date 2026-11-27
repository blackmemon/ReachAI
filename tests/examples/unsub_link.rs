//! Prints an unsubscribe token for local browser checks: unsub_link <secret> <tenant id> <person id>
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() != 3 {
        eprintln!("usage: unsub_link <secret> <tenant id> <person id>");
        std::process::exit(2);
    }
    let data = format!("{}.{}", a[1], a[2]);
    let sig = auth::sign_value(&a[0], &format!("unsub:{data}"));
    println!("{data}.{sig}");
}
