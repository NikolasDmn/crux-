//! Audit M6: with CRUX_ENV=prod, refuse to start on development secrets. Compose falls back to dev
//! values for every secret, so one forgotten line would leave a deploy open. The Python services
//! run the same rules (`crux_common.prod_guard`).

/// Values compose defaults to for development.
const DEV_VALUES: &[&str] = &[
    "dev-service-secret-change-me",
    "dev-unsubscribe-secret-change-me",
    "ZGV2LW9ubHktbWFzdGVyLWtleS1jaGFuZ2UtbWUtISE=",
    "crux-dev-secret",
    "crux-redis-dev",
    "crux-root",
    "crux-core",
    "crux-files",
    "crux-ai",
    "crux-notify",
    "change-me",
];
pub const MIN_SECRET: usize = 32;

/// The password in `scheme://user:password@host/...`, if any.
fn url_password(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let creds = rest.split_once('@')?.0;
    Some(creds.split_once(':')?.1)
}

/// What's wrong with this environment for production (empty when it isn't prod).
pub fn problems(get: impl Fn(&str) -> Option<String>) -> Vec<String> {
    if get("CRUX_ENV").as_deref() != Some("prod") {
        return Vec::new();
    }
    let dev = |v: &str| DEV_VALUES.contains(&v);
    let mut out = Vec::new();
    match get("CRUX_SERVICE_SECRET") {
        Some(s) if !dev(&s) && s.len() >= MIN_SECRET => {}
        _ => out.push(format!("CRUX_SERVICE_SECRET is missing, a development value, or shorter than {MIN_SECRET} characters")),
    }
    for name in ["CRUX_DATABASE_URL", "CRUX_REDIS_URL"] {
        if get(name).as_deref().and_then(url_password).is_some_and(dev) {
            out.push(format!("{name} uses a development password"));
        }
    }
    if get("CRUX_COOKIE_SECURE").is_none_or(|v| v.eq_ignore_ascii_case("false")) {
        out.push("CRUX_COOKIE_SECURE must be true (production is served over HTTPS)".into());
    }
    out
}

pub fn check(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<()> {
    let found = problems(get);
    anyhow::ensure!(found.is_empty(), "Refusing to start with CRUX_ENV=prod: {}", found.join("; "));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(extra: &[(&str, &str)]) -> HashMap<String, String> {
        let mut m: HashMap<String, String> = [
            ("CRUX_ENV", "prod"),
            ("CRUX_SERVICE_SECRET", "a-long-random-service-secret-of-40-chars!"),
            ("CRUX_DATABASE_URL", "mysql://crux_core:a-long-random-password@mysql:3306/crux_core"),
            ("CRUX_REDIS_URL", "redis://:another-random-password@redis:6379/0"),
            ("CRUX_COOKIE_SECURE", "true"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        for (k, v) in extra {
            m.insert(k.to_string(), v.to_string());
        }
        m
    }

    fn found(extra: &[(&str, &str)]) -> Vec<String> {
        let m = env(extra);
        problems(|k| m.get(k).cloned())
    }

    #[test]
    fn production_rules() {
        assert!(found(&[]).is_empty());
        assert!(found(&[("CRUX_ENV", "dev"), ("CRUX_SERVICE_SECRET", "dev-service-secret-change-me")]).is_empty(), "dev isn't checked");
        for (k, v) in [
            ("CRUX_SERVICE_SECRET", "dev-service-secret-change-me"),
            ("CRUX_SERVICE_SECRET", "short"),
            ("CRUX_DATABASE_URL", "mysql://crux_core:crux-core@mysql:3306/crux_core"),
            ("CRUX_REDIS_URL", "redis://:crux-redis-dev@redis:6379/0"),
            ("CRUX_COOKIE_SECURE", "false"),
        ] {
            let f = found(&[(k, v)]);
            assert!(f.len() == 1 && f[0].contains(k), "{k}={v}: {f:?}");
        }
        let m = env(&[]);
        assert!(check(|k| m.get(k).cloned()).is_ok());
    }
}
