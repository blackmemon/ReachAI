//! Roles and permissions. The catalogue is code; tenants cannot invent permissions.
//! Roles come from settings/users_and_permissions/role-based-access.md of the leads skill.

pub const ROLES: [&str; 4] = ["admin", "manager", "employee", "viewer"];

pub const PERMISSIONS: &[&str] = &[
    "record.read",
    "record.write",
    "record.export",
    "campaign.run",
    "sending.pause",
    "settings.business",
    "settings.admin",
    "user.manage",
    "report.read",
    "conversation.reply",
    "ai.use",
];

/// Permissions for a role. Unknown roles get nothing (deny by default).
pub fn for_role(role: &str) -> Vec<&'static str> {
    match role {
        "admin" => PERMISSIONS.to_vec(),
        "manager" => PERMISSIONS
            .iter()
            .copied()
            .filter(|p| !matches!(*p, "settings.admin" | "user.manage"))
            .collect(),
        "employee" => vec![
            "record.read",
            "record.write",
            "conversation.reply",
            "ai.use",
            "report.read",
        ],
        "viewer" => vec!["record.read", "report.read"],
        _ => vec![],
    }
}

pub fn allows(role: &str, permission: &str) -> bool {
    for_role(role).contains(&permission)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_has_everything() {
        for p in PERMISSIONS {
            assert!(allows("admin", p));
        }
    }

    #[test]
    fn manager_cannot_manage_users_or_admin_settings() {
        assert!(!allows("manager", "user.manage"));
        assert!(!allows("manager", "settings.admin"));
        assert!(allows("manager", "campaign.run"));
    }

    #[test]
    fn viewer_is_read_only() {
        assert!(allows("viewer", "report.read"));
        assert!(!allows("viewer", "record.write"));
        assert!(!allows("viewer", "sending.pause"));
    }

    #[test]
    fn unknown_role_is_denied() {
        assert!(for_role("root").is_empty());
    }
}
