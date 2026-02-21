use std::time::Instant;

#[tokio::test]
async fn benchmark_registry_overhead() {
    let registry = omni_glass_lib::mcp::ToolRegistry::new();
    omni_glass_lib::mcp::builtins::register_builtins(&registry).await;

    let mut times = Vec::new();
    for _ in 0..1000 {
        let start = Instant::now();
        let all = registry.all_tools().await;
        let _ = all.iter().filter(|t| t.plugin_id != "builtin").count();
        let _ = registry.tools_for_prompt().await;
        times.push(start.elapsed());
    }
    times.sort();
    let median = times[times.len() / 2];
    let p99 = times[(times.len() as f64 * 0.99) as usize];
    let tools_count = registry.all_tools().await.len();
    let prompt = registry.tools_for_prompt().await;

    eprintln!("=== Phase 2 Registry Hot-Path Benchmark ===");
    eprintln!("Builtin tools: {}", tools_count);
    eprintln!("Median (1000 iter): {:?}", median);
    eprintln!("P99 (1000 iter):    {:?}", p99);
    eprintln!("Plugin prompt bytes: {}", prompt.len());
    eprintln!("============================================");
}

#[test]
fn benchmark_approval_check() {
    use omni_glass_lib::mcp::approval;
    use omni_glass_lib::mcp::manifest::{Permissions, PluginManifest, Runtime};

    let manifest = PluginManifest {
        id: "com.benchmark.test".to_string(),
        name: "Bench".to_string(),
        version: "1.0.0".to_string(),
        description: String::new(),
        runtime: Runtime::Node,
        entry: "index.js".to_string(),
        permissions: Permissions::default(),
        configuration: None,
    };

    let store = approval::load_approvals();

    let mut times = Vec::new();
    for _ in 0..10000 {
        let start = Instant::now();
        let _ = approval::check_approval(&store, &manifest);
        times.push(start.elapsed());
    }
    times.sort();
    let median = times[times.len() / 2];
    let p99 = times[(times.len() as f64 * 0.99) as usize];

    eprintln!("=== Approval Check Benchmark ===");
    eprintln!("Median (10000 iter): {:?}", median);
    eprintln!("P99 (10000 iter):    {:?}", p99);
    eprintln!("================================");
}

#[test]
fn benchmark_env_filter() {
    use omni_glass_lib::mcp::sandbox::env_filter;
    use omni_glass_lib::mcp::manifest::Permissions;

    let perms = Permissions::default();

    let mut times = Vec::new();
    for _ in 0..10000 {
        let start = Instant::now();
        let _ = env_filter::filter_environment(&perms, "com.benchmark.test");
        times.push(start.elapsed());
    }
    times.sort();
    let median = times[times.len() / 2];
    let p99 = times[(times.len() as f64 * 0.99) as usize];

    eprintln!("=== Env Filter Benchmark ===");
    eprintln!("Median (10000 iter): {:?}", median);
    eprintln!("P99 (10000 iter):    {:?}", p99);
    eprintln!("============================");
}

#[test]
#[cfg(target_os = "macos")]
fn benchmark_profile_generation() {
    use omni_glass_lib::mcp::sandbox::macos;
    use omni_glass_lib::mcp::manifest::{Permissions, PluginManifest, Runtime};

    let manifest = PluginManifest {
        id: "com.benchmark.test".to_string(),
        name: "Bench".to_string(),
        version: "1.0.0".to_string(),
        description: String::new(),
        runtime: Runtime::Node,
        entry: "index.js".to_string(),
        permissions: Permissions::default(),
        configuration: None,
    };
    let dir = std::env::temp_dir().join("og-bench");
    let _ = std::fs::create_dir_all(&dir);

    let mut times = Vec::new();
    for _ in 0..1000 {
        let start = Instant::now();
        let _ = macos::generate_profile(&manifest, &dir).unwrap();
        times.push(start.elapsed());
    }
    times.sort();
    let median = times[times.len() / 2];
    let p99 = times[(times.len() as f64 * 0.99) as usize];

    eprintln!("=== Profile Generation Benchmark ===");
    eprintln!("Median (1000 iter): {:?}", median);
    eprintln!("P99 (1000 iter):    {:?}", p99);
    eprintln!("====================================");
}
