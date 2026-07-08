fn main() {
    // If HEAD points to an annotated/lightweight tag (e.g. v1.2.3-beta.1),
    // use it as-is (strip leading 'v').  Otherwise fall back to VERSION.build.
    let git_tag = std::process::Command::new("git")
        .args(["describe", "--tags", "--exact-match", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().trim_start_matches('v').to_string())
        .filter(|s| !s.is_empty());

    let version = if let Some(tag) = git_tag {
        tag
    } else {
        let base = std::fs::read_to_string("../VERSION")
            .unwrap_or_else(|_| "0.0".to_string());
        let base = base.trim().to_string();

        // Build number = total commit count on current branch.
        let build = std::process::Command::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "0".to_string());

        format!("{}.{}", base, build)
    };

    println!("cargo:rustc-env=APP_VERSION={}", version);

    // Rustc version embedded for the About/diagnostics page
    let rustc_ver = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=RUSTC_VERSION={}", rustc_ver);

    // Parse Cargo.lock and embed versions of key dependencies.
    // Format: "name=version,name=version,..."
    let cargo_deps = parse_cargo_lock_deps("../Cargo.lock");
    println!("cargo:rustc-env=CARGO_DEPS={}", cargo_deps);

    // Rebuild when VERSION changes or a new commit is made
    println!("cargo:rerun-if-changed=../VERSION");
    println!("cargo:rerun-if-changed=../.git/refs/heads");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../Cargo.lock");
}

fn parse_cargo_lock_deps(path: &str) -> String {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    // Deps we care about, in display order
    let want: &[(&str, &str)] = &[
        ("axum",                "Axum"),
        ("tokio",               "Tokio"),
        ("serde",               "Serde"),
        ("serde_json",          "serde_json"),
        ("sqlx",                "SQLx"),
        ("reqwest",             "reqwest"),
        ("tracing",             "tracing"),
        ("tracing-subscriber",  "tracing-subscriber"),
        ("tower-http",          "tower-http"),
        ("anyhow",              "anyhow"),
        ("dashmap",             "DashMap"),
        ("flume",               "flume"),
        ("ed25519-dalek",       "ed25519-dalek"),
        ("ring",                "ring"),
        ("jsonwebtoken",        "jsonwebtoken"),
        ("bcrypt",              "bcrypt"),
        ("prometheus",          "prometheus"),
        ("clickhouse",          "clickhouse"),
        ("chrono",              "chrono"),
        ("socket2",             "socket2"),
        ("base64",              "base64"),
        ("ahash",               "ahash"),
    ];

    // Simple block parser: split on [[package]], grab name + version
    let mut versions: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for block in content.split("[[package]]") {
        let name = block.lines()
            .find(|l| l.starts_with("name = "))
            .and_then(|l| l.split('"').nth(1))
            .map(str::to_owned);
        let ver = block.lines()
            .find(|l| l.starts_with("version = "))
            .and_then(|l| l.split('"').nth(1))
            .map(str::to_owned);
        if let (Some(n), Some(v)) = (name, ver) {
            // Only keep first occurrence (direct dep wins over transitive duplicates)
            versions.entry(n).or_insert(v);
        }
    }

    want.iter()
        .filter_map(|(key, label)| {
            versions.get(*key).map(|v| format!("{}={}", label, v))
        })
        .collect::<Vec<_>>()
        .join(",")
}
