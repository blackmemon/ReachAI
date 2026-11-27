//! Command line tasks for the deployer. They use the owner database role, never the runtime role.

fn owner_url() -> String {
    std::env::var("LEADS_OWNER_URL").unwrap_or_else(|_| "postgres://leads:leads@127.0.0.1:5433/leads_dev".into())
}

pub async fn run(cmd: &str, args: &[String]) -> i32 {
    match cmd {
        "migrate" => match database::migrate(&owner_url()).await {
            Ok(_) => {
                println!("Migrations are up to date.");
                0
            }
            Err(e) => {
                eprintln!("Migration failed: {e}");
                1
            }
        },
        "create-tenant" => {
            let [company, email, name, password] = args else {
                eprintln!("Usage: blackmemon-leads create-tenant <company> <admin email> <admin name> <password>");
                return 2;
            };
            let pool = match database::connect(&owner_url(), 2).await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Cannot reach the database: {e}");
                    return 1;
                }
            };
            match backend::accounts::create_tenant(&pool, company, email, name, password).await {
                Ok((t, _)) => {
                    println!("Company created: {t}");
                    0
                }
                Err(e) => {
                    eprintln!("{}", e.body().message);
                    1
                }
            }
        }
        "seed" => {
            let pool = match database::connect(&owner_url(), 2).await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Cannot reach the database: {e}");
                    return 1;
                }
            };
            let tenant = args.first().map(String::as_str);
            match backend::seed::run(&pool, tenant).await {
                Ok(msg) => {
                    println!("{msg}");
                    0
                }
                Err(e) => {
                    eprintln!("{}", e.body().message);
                    1
                }
            }
        }
        _ => {
            eprintln!("Unknown command. Use: serve, migrate, create-tenant, seed");
            2
        }
    }
}
