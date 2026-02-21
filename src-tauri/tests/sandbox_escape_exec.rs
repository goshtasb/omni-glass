//! Sandbox escape tests — env vars, shell, temp directory restrictions.
//!
//! Tests 6-10 of the sandbox escape suite. Verifies environment variable
//! filtering, shell command restrictions, and temp directory isolation.
//!
//! Test 6 is cross-platform. Tests 7-10 are macOS-only (sandbox-exec).
//! See sandbox_escape.rs for tests 1-5 (network, filesystem, reads).

mod sandbox_helpers;

use omni_glass_lib::mcp::sandbox::env_filter;

// Cross-platform imports
use omni_glass_lib::mcp::manifest::Permissions;

// macOS-only imports
#[cfg(target_os = "macos")]
use omni_glass_lib::mcp::manifest::ShellPerm;
#[cfg(target_os = "macos")]
use sandbox_helpers::{node_available, run_sandboxed, setup_test_dir, test_manifest};

// ── Test 6: Cannot read undeclared env vars (cross-platform) ───────

#[test]
fn cannot_read_undeclared_env_vars() {
    std::env::set_var("OG_TEST_SECRET_XYZ", "super-secret-value");

    let perms = Permissions::default();
    let filtered = env_filter::filter_environment(&perms, "com.test.env");

    assert!(
        !filtered.contains_key("OG_TEST_SECRET_XYZ"),
        "Undeclared env var should be stripped"
    );
    assert!(
        filtered.contains_key("PATH"),
        "Essential vars should be present"
    );
    assert_eq!(
        filtered.get("OMNI_GLASS_PLUGIN_ID").unwrap(),
        "com.test.env"
    );

    std::env::remove_var("OG_TEST_SECRET_XYZ");
}

// ── Test 7: No shell cannot spawn ──────────────────────────────────

#[test]
#[cfg(target_os = "macos")]
fn no_shell_cannot_spawn() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let dir = setup_test_dir("no-shell");
    let manifest = test_manifest("com.test.no-shell", Permissions::default());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        r#"
const { execSync } = require('child_process');
try {
    execSync('echo hello');
    process.exit(0);
} catch (e) {
    process.exit(1);
}
"#,
    )
    .unwrap();

    let env = env_filter::filter_environment(&manifest.permissions, &manifest.id);
    let (code, _, _) = run_sandboxed(&profile_path, &script, env);
    assert_ne!(code, 0, "Should not spawn without shell permission");
}

// ── Test 8: With shell can run declared command ────────────────────

#[test]
#[cfg(target_os = "macos")]
fn with_shell_can_run_declared_command() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let dir = setup_test_dir("with-shell");
    let perms = Permissions {
        shell: Some(ShellPerm {
            commands: vec!["echo".into()],
        }),
        ..Default::default()
    };
    let manifest = test_manifest("com.test.with-shell", perms.clone());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        r#"
const { execSync } = require('child_process');
try {
    const out = execSync('echo hello').toString().trim();
    process.exit(out === 'hello' ? 0 : 1);
} catch (e) {
    process.exit(1);
}
"#,
    )
    .unwrap();

    let env = env_filter::filter_environment(&perms, &manifest.id);
    let (code, _, _) = run_sandboxed(&profile_path, &script, env);
    assert_eq!(code, 0, "Should run declared shell command");
}

// ── Test 9: Cannot write to global /tmp ────────────────────────────

#[test]
#[cfg(target_os = "macos")]
fn cannot_write_global_tmp() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let dir = setup_test_dir("no-global-tmp");
    let manifest = test_manifest("com.test.no-tmp", Permissions::default());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        r#"
const fs = require('fs');
try {
    fs.writeFileSync('/tmp/og-sandbox-escape-test.txt', 'escaped!');
    process.exit(0);
} catch (e) {
    process.exit(1);
}
"#,
    )
    .unwrap();

    let env = env_filter::filter_environment(&manifest.permissions, &manifest.id);
    let (code, _, _) = run_sandboxed(&profile_path, &script, env);
    assert_ne!(code, 0, "Should not write to global /tmp");
}

// ── Test 10: Can write to own tmp directory ────────────────────────

#[test]
#[cfg(target_os = "macos")]
fn can_write_own_tmp() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let dir = setup_test_dir("own-tmp");
    let plugin_id = "com.test.own-tmp";
    let manifest = test_manifest(plugin_id, Permissions::default());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let plugin_tmp = format!("/tmp/omni-glass-{}", plugin_id);
    let _ = std::fs::create_dir_all(&plugin_tmp);

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        &format!(
            r#"
const fs = require('fs');
try {{
    fs.writeFileSync('{}/test-file.txt', 'plugin temp data');
    const data = fs.readFileSync('{}/test-file.txt', 'utf8');
    process.exit(data === 'plugin temp data' ? 0 : 1);
}} catch (e) {{
    console.error(e.message);
    process.exit(1);
}}
"#,
            plugin_tmp, plugin_tmp
        ),
    )
    .unwrap();

    let env = env_filter::filter_environment(&manifest.permissions, &manifest.id);
    let (code, _, _) = run_sandboxed(&profile_path, &script, env);
    assert_eq!(code, 0, "Should write to own temp directory");

    let _ = std::fs::remove_dir_all(&plugin_tmp);
}
