//! Permission risk level calculator.
//!
//! Computes a Low / Medium / High risk badge from a plugin's declared
//! permissions. Used by the permission prompt UI to help users make
//! informed decisions.

use crate::mcp::manifest::Permissions;
use serde::Serialize;

/// Visual risk level for permission prompts.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Calculate a risk score from the declared permissions.
///
/// Scoring: clipboard=1, network=2, fs-read=2/entry, fs-write=4/entry,
/// environment=2/var, shell=5. Thresholds: 0-1=Low, 2-4=Medium, 5+=High.
pub fn calculate_risk(permissions: &Permissions) -> RiskLevel {
    let mut score: u32 = 0;

    if permissions.clipboard {
        score += 1;
    }

    if permissions.network.as_ref().map_or(false, |v| !v.is_empty()) {
        score += 2;
    }

    if let Some(ref fs_perms) = permissions.filesystem {
        for perm in fs_perms {
            match perm.access.as_str() {
                "read" => score += 2,
                "write" | "read-write" => score += 4,
                _ => score += 2,
            }
        }
    }

    if let Some(ref env_vars) = permissions.environment {
        score += env_vars.len() as u32 * 2;
    }

    if permissions.shell.is_some() {
        score += 5;
    }

    match score {
        0..=1 => RiskLevel::Low,
        2..=4 => RiskLevel::Medium,
        _ => RiskLevel::High,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::manifest::{FsPerm, ShellPerm};

    #[test]
    fn clipboard_only_is_low() {
        let perms = Permissions { clipboard: true, ..Default::default() };
        assert_eq!(calculate_risk(&perms), RiskLevel::Low);
    }

    #[test]
    fn network_is_medium() {
        let perms = Permissions {
            network: Some(vec!["api.example.com".into()]),
            ..Default::default()
        };
        assert_eq!(calculate_risk(&perms), RiskLevel::Medium);
    }

    #[test]
    fn shell_is_high() {
        let perms = Permissions {
            shell: Some(ShellPerm { commands: vec!["echo".into()] }),
            ..Default::default()
        };
        assert_eq!(calculate_risk(&perms), RiskLevel::High);
    }

    #[test]
    fn filesystem_write_is_high() {
        let perms = Permissions {
            filesystem: Some(vec![
                FsPerm { path: "~/Documents".into(), access: "read-write".into() },
            ]),
            ..Default::default()
        };
        assert_eq!(calculate_risk(&perms), RiskLevel::Medium); // 4 = medium
    }

    #[test]
    fn combined_permissions_escalate() {
        let perms = Permissions {
            clipboard: true,
            network: Some(vec!["api.example.com".into()]),
            environment: Some(vec!["API_TOKEN".into()]),
            ..Default::default()
        };
        assert_eq!(calculate_risk(&perms), RiskLevel::High); // 1+2+2 = 5
    }
}
