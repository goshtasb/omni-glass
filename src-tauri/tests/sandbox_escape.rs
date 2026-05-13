//! Sandbox escape tests — network, filesystem, read restrictions.
//!
//! Tests 1-5 of the sandbox escape suite. Verifies that sandbox-exec
//! profiles correctly block network access, filesystem reads/writes,
//! and allow declared permissions.
//!
//! macOS-only: all tests use sandbox-exec which is macOS-specific.
//! See sandbox_escape_exec.rs for tests 6-10 (env vars, shell, tmp).

#![cfg(target_os = "macos")]

mod sandbox_helpers;

use omni_glass_lib::mcp::manifest::{FsPerm, Permissions};
use omni_glass_lib::mcp::sandbox::env_filter;
use sandbox_helpers::{node_available, run_sandboxed, setup_test_dir, test_manifest};

// ── Test 1: No network cannot connect ──────────────────────────────

#[test]
fn no_network_cannot_connect() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let dir = setup_test_dir("no-net");
    let manifest = test_manifest("com.test.no-net", Permissions::default());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        r#"
const http = require('http');
const req = http.get('http://httpbin.org/get', (res) => {
    process.exit(0);
});
req.on('error', () => { process.exit(1); });
req.setTimeout(5000, () => { process.exit(1); });
"#,
    )
    .unwrap();

    let env = env_filter::filter_environment(&manifest.permissions, &manifest.id);
    let (code, _, _) = run_sandboxed(&profile_path, &script, env);
    assert_ne!(code, 0, "Network request should fail without network permission");
}

// ── Test 2: With network can connect ───────────────────────────────

#[test]
fn with_network_can_connect() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let dir = setup_test_dir("with-net");
    let perms = Permissions {
        network: Some(vec!["httpbin.org".into()]),
        ..Default::default()
    };
    let manifest = test_manifest("com.test.with-net", perms.clone());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        r#"
const dns = require('dns');
dns.lookup('httpbin.org', (err) => {
    process.exit(err ? 1 : 0);
});
"#,
    )
    .unwrap();

    let env = env_filter::filter_environment(&perms, &manifest.id);
    let (code, _, _) = run_sandboxed(&profile_path, &script, env);
    assert_eq!(code, 0, "DNS lookup should succeed with network permission");
}

// ── Test 3: Cannot read ~/.ssh ─────────────────────────────────────

#[test]
fn cannot_read_home_ssh() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let dir = setup_test_dir("no-ssh");
    let manifest = test_manifest("com.test.no-ssh", Permissions::default());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let home = dirs::home_dir().unwrap();
    let ssh_path = home.join(".ssh");

    if !ssh_path.exists() {
        eprintln!("SKIP: ~/.ssh doesn't exist");
        return;
    }

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        &format!(
            r#"
const fs = require('fs');
try {{
    fs.readdirSync('{}');
    process.exit(0);
}} catch (e) {{
    process.exit(1);
}}
"#,
            ssh_path.to_string_lossy()
        ),
    )
    .unwrap();

    let env = env_filter::filter_environment(&manifest.permissions, &manifest.id);
    let (code, _, _) = run_sandboxed(&profile_path, &script, env);
    assert_ne!(code, 0, "Should not be able to read ~/.ssh");
}

// ── Test 3b: Cannot read ~/.aws/credentials ─────────────────────────

#[test]
fn cannot_read_aws_credentials() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let home = dirs::home_dir().unwrap();
    let aws_creds = home.join(".aws/credentials");
    if !aws_creds.exists() {
        eprintln!("SKIP: ~/.aws/credentials doesn't exist");
        return;
    }

    let dir = setup_test_dir("no-aws");
    let manifest = test_manifest("com.test.no-aws", Permissions::default());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        &format!(
            r#"
const fs = require('fs');
try {{
    const data = fs.readFileSync('{}', 'utf8');
    console.log('LEAKED: ' + data.substring(0, 20));
    process.exit(0);
}} catch (e) {{
    process.exit(1);
}}
"#,
            aws_creds.to_string_lossy()
        ),
    )
    .unwrap();

    let env = env_filter::filter_environment(&manifest.permissions, &manifest.id);
    let (code, stdout, _) = run_sandboxed(&profile_path, &script, env);
    assert_ne!(code, 0, "Should not read ~/.aws/credentials");
    assert!(
        !stdout.contains("LEAKED"),
        "Credentials must not appear in stdout"
    );
}

// ── Test 4: Can read own directory ─────────────────────────────────

#[test]
fn can_read_own_directory() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let dir = setup_test_dir("read-own");
    std::fs::write(dir.join("data.txt"), "test data").unwrap();

    let manifest = test_manifest("com.test.read-own", Permissions::default());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        &format!(
            r#"
const fs = require('fs');
try {{
    const data = fs.readFileSync('{}/data.txt', 'utf8');
    process.exit(data === 'test data' ? 0 : 1);
}} catch (e) {{
    process.exit(1);
}}
"#,
            dir.to_string_lossy()
        ),
    )
    .unwrap();

    let env = env_filter::filter_environment(&manifest.permissions, &manifest.id);
    let (code, _, _) = run_sandboxed(&profile_path, &script, env);
    assert_eq!(code, 0, "Plugin should be able to read its own directory");
}

// ── Test 5: Read-only filesystem cannot write ──────────────────────

#[test]
fn readonly_filesystem_cannot_write() {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return;
    }

    use omni_glass_lib::mcp::sandbox::macos;

    let dir = setup_test_dir("readonly-fs");
    let target_dir = std::env::temp_dir().join("og-sandbox-test-readonly-target");
    let _ = std::fs::create_dir_all(&target_dir);

    let perms = Permissions {
        filesystem: Some(vec![FsPerm {
            path: target_dir.to_string_lossy().to_string(),
            access: "read".into(),
        }]),
        ..Default::default()
    };
    let manifest = test_manifest("com.test.readonly", perms.clone());
    let profile = macos::generate_profile(&manifest, &dir).unwrap();
    let profile_path = macos::write_profile(&manifest.id, &profile).unwrap();

    let script = dir.join("test.js");
    std::fs::write(
        &script,
        &format!(
            r#"
const fs = require('fs');
try {{
    fs.writeFileSync('{}/evil.txt', 'hacked');
    process.exit(0);
}} catch (e) {{
    process.exit(1);
}}
"#,
            target_dir.to_string_lossy()
        ),
    )
    .unwrap();

    let env = env_filter::filter_environment(&perms, &manifest.id);
    let (code, _, _) = run_sandboxed(&profile_path, &script, env);
    assert_ne!(code, 0, "Read-only filesystem should prevent writes");
}
