//! `crux-core`: the core service binary.
//!
//!   crux-core serve            HTTP API (runs migrations first unless --no-migrate)
//!   crux-core jobs             outbox relay + scheduled jobs
//!   crux-core migrate          apply migrations and exit
//!   crux-core gen-keys PATH    write a new Ed25519 JWT signing key (PKCS#8 PEM)
//!   crux-core create-invite    print a fresh invite code (bootstrap, seeding)
//!   crux-core make-admin EMAIL give an existing user the Admin role

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use crux_api::state::Peers;
use crux_api::AppState;
use crux_clients::bus::RedisBus;
use crux_clients::clock::SystemClock;
use crux_clients::ids::{IdGen, UlidGen};
use crux_clients::kv::RedisKv;
use crux_service::ctx::{load_breached_passwords, Config};
use crux_service::jwt::JwtKeys;
use crux_service::Ctx;

#[derive(Parser)]
#[command(name = "crux-core", version)]
struct Cli {
    #[arg(long, env = "CRUX_DATABASE_URL")]
    database_url: Option<String>,
    #[arg(long, env = "CRUX_REDIS_URL", default_value = "redis://127.0.0.1:6379/0")]
    redis_url: String,
    #[arg(long, env = "CRUX_JWT_PRIVATE_KEY_FILE", default_value = "secrets/jwt_ed25519.pem")]
    jwt_key_file: String,
    #[arg(long, env = "CRUX_BREACHED_PASSWORDS_FILE", default_value = "data/breached-passwords.txt")]
    breached_file: String,
    #[arg(long, env = "CRUX_APP_BASE_URL", default_value = "http://localhost:8000")]
    app_base_url: String,
    #[arg(long, env = "CRUX_SERVICE_SECRET", default_value = "")]
    service_secret: String,
    #[arg(long, env = "CRUX_COOKIE_SECURE", default_value_t = true)]
    cookie_secure: bool,
    /// Have I Been Pwned range API; empty disables the online check (offline list only).
    #[arg(long, env = "CRUX_HIBP_URL", default_value = "https://api.pwnedpasswords.com")]
    hibp_url: String,
    #[arg(long, env = "CRUX_LISTEN", default_value = "0.0.0.0:8080")]
    listen: String,
    #[arg(long, env = "CRUX_FILES_INTERNAL_URL", default_value = "http://files:8080")]
    files_url: String,
    #[arg(long, env = "CRUX_AI_INTERNAL_URL", default_value = "http://ai:8080")]
    ai_url: String,
    #[arg(long, env = "CRUX_NOTIFY_INTERNAL_URL", default_value = "http://notify:8080")]
    notify_url: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Serve {
        #[arg(long)]
        no_migrate: bool,
    },
    Jobs,
    Migrate,
    GenKeys {
        path: String,
    },
    CreateInvite {
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        tier: Option<String>,
        #[arg(long, default_value_t = 1)]
        max_uses: i32,
        #[arg(long, default_value_t = 14)]
        days: i64,
    },
    MakeAdmin {
        email: String,
    },
    /// Mark a user's email verified (a trial without email): `verify-user <username or email>`.
    VerifyUser {
        login: String,
    },
    /// Set a temporary password, print it, and sign the user out everywhere.
    ResetPassword {
        login: String,
    },
    /// Container health check: exits 0 when this process's `/healthz` answers on CRUX_LISTEN's port
    /// (the image has no curl).
    Healthcheck,
    /// Load testing: create users and print `username token` lines (refused when CRUX_ENV=prod).
    LoadUsers {
        #[arg(long, default_value_t = 500)]
        count: u32,
        #[arg(long, default_value = "load")]
        prefix: String,
        #[arg(long, default_value_t = 2)]
        hours: i64,
    },
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into());
    tracing_subscriber::fmt().json().flatten_event(true).with_current_span(false).with_env_filter(filter).init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    if let Cmd::Healthcheck = &cli.cmd {
        let port = cli.listen.rsplit(':').next().unwrap_or("8080");
        let ok = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?
            .get(format!("http://127.0.0.1:{port}/healthz"))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success());
        std::process::exit(if ok { 0 } else { 1 });
    }
    if let Cmd::GenKeys { path } = &cli.cmd {
        if std::path::Path::new(path).exists() {
            anyhow::bail!("{path} exists; refusing to overwrite a signing key");
        }
        if let Some(dir) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, JwtKeys::generate_pem())?;
        println!("wrote {path}");
        return Ok(());
    }

    let db_url = cli.database_url.clone().context("CRUX_DATABASE_URL is required")?;
    let pool = crux_repo::db::connect(&db_url, 20).await.context("connect to MySQL")?;

    match &cli.cmd {
        Cmd::Migrate => {
            crux_repo::db::migrate_url(&db_url).await?;
            println!("migrations applied");
            return Ok(());
        }
        Cmd::MakeAdmin { email } => {
            let n = crux_repo::users::make_admin_by_email(&mut *pool.acquire().await?, email).await?;
            anyhow::ensure!(n == 1, "no user with email {email}");
            println!("{email} is now an admin");
            return Ok(());
        }
        Cmd::VerifyUser { login } => {
            let mut conn = pool.acquire().await?;
            let u = crux_repo::users::by_login(&mut conn, login).await?.with_context(|| format!("no user {login}"))?;
            crux_repo::users::set_email_verified(&mut conn, u.id, chrono::Utc::now()).await?;
            println!("{} ({}) is verified", u.username, u.email);
            return Ok(());
        }
        Cmd::ResetPassword { login } => {
            let mut conn = pool.acquire().await?;
            let u = crux_repo::users::by_login(&mut conn, login).await?.with_context(|| format!("no user {login}"))?;
            let temp = crux_service::password::temporary();
            let hash = crux_service::password::hash_blocking(temp.clone()).await?;
            crux_repo::users::set_password_hash(&mut conn, u.id, &hash).await?;
            crux_repo::sessions::revoke_all_for_user(&mut conn, u.id, chrono::Utc::now(), crux_domain::auth::RevokeReason::PasswordReset)
                .await?;
            println!("{}: temporary password {temp} (signed out everywhere; ask them to change it)", u.username);
            return Ok(());
        }
        Cmd::CreateInvite { email, tier, max_uses, days } => {
            let code = create_invite_cli(&pool, email.as_deref(), tier.as_deref(), *max_uses, *days).await?;
            println!("{code}");
            return Ok(());
        }
        _ => {}
    }

    let ctx = build_ctx(&cli, pool.clone()).await?;
    if let Cmd::LoadUsers { count, prefix, hours } = &cli.cmd {
        anyhow::ensure!(std::env::var("CRUX_ENV").as_deref() != Ok("prod"), "load-users is for test stacks only");
        for (username, token) in crux_service::auth::load_test_users(&ctx, prefix, *count, *hours).await? {
            println!("{username} {token}");
        }
        return Ok(());
    }
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = shutdown_tx.send(true);
    });

    match cli.cmd {
        Cmd::Serve { no_migrate } => {
            if !no_migrate {
                crux_repo::db::migrate_url(&db_url).await.context("migrations")?;
            }
            let peers = Peers {
                openapi_sources: vec![
                    ("/files".into(), cli.files_url.clone()),
                    ("/ai".into(), cli.ai_url.clone()),
                    ("/notify".into(), cli.notify_url.clone()),
                ],
            };
            let state = AppState { ctx, peers: Arc::new(peers), cookie_secure: cli.cookie_secure };
            let listener = tokio::net::TcpListener::bind(&cli.listen).await?;
            tracing::info!(listen = %cli.listen, "core listening");
            let mut rx = shutdown_rx.clone();
            axum::serve(listener, crux_api::app(state))
                .with_graceful_shutdown(async move {
                    let _ = rx.changed().await;
                })
                .await?;
        }
        Cmd::Jobs => {
            // Jobs may start before `serve` has migrated an empty database; the migrator's lock
            // makes this safe to run from both.
            crux_repo::db::migrate_url(&db_url).await.context("migrations")?;
            run_jobs(&cli, ctx, shutdown_rx).await?
        }
        _ => unreachable!(),
    }
    Ok(())
}

async fn build_ctx(cli: &Cli, pool: crux_repo::MySqlPool) -> anyhow::Result<Ctx> {
    let pem = std::fs::read_to_string(&cli.jwt_key_file)
        .with_context(|| format!("read JWT key {} (run `crux-core gen-keys`)", cli.jwt_key_file))?;
    let breached = std::fs::read_to_string(&cli.breached_file).unwrap_or_else(|_| {
        tracing::warn!(file = %cli.breached_file, "breached-password list missing; only length is checked");
        String::new()
    });
    anyhow::ensure!(!cli.service_secret.is_empty(), "CRUX_SERVICE_SECRET is required");
    let kv = RedisKv::connect(&cli.redis_url).await.context("connect to Redis")?;
    Ok(Ctx {
        pool,
        clock: Arc::new(SystemClock),
        ids: Arc::new(UlidGen::default()),
        kv: Arc::new(kv),
        keys: Arc::new(JwtKeys::from_pkcs8_pem(&pem)?),
        files: Arc::new(crux_clients::files::HttpFiles::new(&cli.files_url)),
        ai: Arc::new(crux_clients::ai::HttpAi::new(&cli.ai_url)),
        exports: Arc::new(crux_clients::export::HttpExport::new(&cli.files_url, &cli.ai_url, &cli.notify_url)),
        breach_check: (!cli.hibp_url.is_empty())
            .then(|| Arc::new(crux_clients::pwned::Hibp::new(&cli.hibp_url)) as Arc<dyn crux_clients::pwned::BreachCheck>),
        breached_passwords: Arc::new(load_breached_passwords(&breached)),
        config: Arc::new(Config {
            app_base_url: cli.app_base_url.trim_end_matches('/').to_string(),
            service_secret: cli.service_secret.clone(),
        }),
    })
}

async fn run_jobs(cli: &Cli, ctx: Ctx, shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
    let redis = RedisKv::connect(&cli.redis_url).await?;
    let bus: Arc<dyn crux_clients::bus::EventBus> = Arc::new(RedisBus::new(redis.connection()));
    // The consumer gets its own connection: its blocking XREADGROUP would otherwise hold up the
    // relay's XADDs on the shared multiplexed connection while the relay holds Outbox row locks.
    let consumer_redis = RedisKv::connect(&cli.redis_url).await?;
    let consumer_bus = RedisBus::new(consumer_redis.connection());
    let file_events = Arc::new(crux_jobs::files::FileEvents);
    consumer_bus.ensure_group("core", &crux_jobs::consumer::Handler::topics(file_events.as_ref())).await?;
    let consumer_name = format!("core-{}", std::env::var("HOSTNAME").unwrap_or_else(|_| "jobs".into()));
    let files_consumer = tokio::spawn(crux_jobs::consumer::run(
        ctx.pool.clone(),
        Arc::new(consumer_bus),
        "core",
        consumer_name,
        file_events,
        shutdown.clone(),
    ));
    let relay = tokio::spawn(crux_jobs::relay::run(ctx.pool.clone(), bus, ctx.clock.clone(), "core", shutdown.clone()));
    let pool = ctx.pool.clone();
    let clock = ctx.clock.clone();
    let tokens = tokio::spawn(crux_jobs::every("purge.tokens", Duration::from_secs(24 * 3600), shutdown.clone(), move || {
        let (pool, clock) = (pool.clone(), clock.clone());
        async move { crux_jobs::tokens::purge(&pool, clock.as_ref()).await }
    }));
    let purge_ctx = ctx.clone();
    let trash = tokio::spawn(crux_jobs::every("courses.purge_trashed", Duration::from_secs(24 * 3600), shutdown.clone(), move || {
        let ctx = purge_ctx.clone();
        async move { crux_service::courses::purge_trashed(&ctx).await }
    }));
    let derive_ctx = ctx.clone();
    let derive = tokio::spawn(crux_jobs::daily_at("sessions.derive", 2, shutdown.clone(), move || {
        let ctx = derive_ctx.clone();
        async move { crux_service::activity::derive_all(&ctx).await }
    }));
    let weeks_ctx = ctx.clone();
    let weeks = tokio::spawn(crux_jobs::daily_at("readmodels.rebuild", 3, shutdown.clone(), move || {
        let ctx = weeks_ctx.clone();
        async move { crux_service::stats::rebuild_all(&ctx).await }
    }));
    let tick_ctx = ctx.clone();
    let tick = tokio::spawn(crux_jobs::every("deadlines.tick", Duration::from_secs(15 * 60), shutdown.clone(), move || {
        let ctx = tick_ctx.clone();
        async move { crux_service::deadlines::tick(&ctx).await }
    }));
    let export_ctx = ctx.clone();
    let exports = tokio::spawn(crux_jobs::every("account.export", Duration::from_secs(30), shutdown.clone(), move || {
        let ctx = export_ctx.clone();
        async move { crux_service::account::run_exports(&ctx).await }
    }));
    let deletion_ctx = ctx.clone();
    let deletions = tokio::spawn(crux_jobs::every("account.delete", Duration::from_secs(3600), shutdown.clone(), move || {
        let ctx = deletion_ctx.clone();
        async move { crux_service::account::run_deletions(&ctx).await }
    }));
    let _ = tokio::join!(relay, files_consumer, tokens, trash, derive, weeks, tick, exports, deletions);
    Ok(())
}

async fn create_invite_cli(
    pool: &crux_repo::MySqlPool,
    email: Option<&str>,
    tier: Option<&str>,
    max_uses: i32,
    days: i64,
) -> anyhow::Result<String> {
    use crux_domain::auth::{normalize_email, normalize_invite_code, token_hash};
    let ids = UlidGen::default();
    let code = ids.invite_code();
    let mut conn = pool.acquire().await?;
    let tier_id = match tier {
        Some(t) => Some(crux_repo::tiers::by_code(&mut conn, t).await?.with_context(|| format!("no tier {t}"))?.id),
        None => None,
    };
    let email = email.map(normalize_email);
    crux_repo::invites::insert(
        &mut conn,
        &crux_repo::invites::NewInvite {
            public_id: &ids.ulid(),
            code_hash: &token_hash(&normalize_invite_code(&code)),
            email: email.as_deref(),
            invited_by: None,
            tier_id,
            max_uses,
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(days)),
        },
    )
    .await?;
    Ok(code)
}
