//! The public route table (backend-spec 15) as data: every route in the OpenAPI document has
//! exactly one policy here (unit test below), and the cross-user harness (`tests/cross_user.rs`)
//! uses it to prove that one user can't reach another's resources (404).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// No sign-in.
    Public,
    /// Signed in; acts only on the caller's own data.
    Owns,
    /// Signed in with a verified email.
    VerifiedEmail,
    /// `Users.Role = 'Admin'`.
    Admin,
    /// `can_read_course` on the course the path names (spec 4.2).
    Read,
    /// `can_edit_course`: Owner or Editor.
    Edit,
    /// `can_manage_course`: Owner only.
    Manage,
    /// A per-user resource (mock, slip, weak spot, trap): only its user, whatever their role on
    /// the course (review section 2: a subscription never shows other subscribers' mocks).
    Mine,
    /// Needs the caller's own subscription to the course.
    Subscribed,
}

/// `(METHOD, path, policy)`. Paths are as the router sees them (after `/api/v1` is stripped).
pub const ROUTES: &[(&str, &str, Policy)] = &[
    ("POST", "/auth/signup", Policy::Public),
    ("POST", "/auth/login", Policy::Public),
    ("POST", "/auth/refresh", Policy::Public),
    ("POST", "/auth/logout", Policy::Public),
    ("POST", "/auth/verify-email", Policy::Owns),
    ("POST", "/auth/verify-email/confirm", Policy::Public),
    ("POST", "/auth/password/forgot", Policy::Public),
    ("POST", "/auth/password/reset", Policy::Public),
    ("GET", "/me", Policy::Owns),
    ("PATCH", "/me", Policy::Owns),
    ("GET", "/me/settings", Policy::Owns),
    ("PATCH", "/me/settings", Policy::Owns),
    ("GET", "/me/tier", Policy::Owns),
    ("POST", "/invites", Policy::VerifiedEmail),
    ("GET", "/invites", Policy::Owns),
    ("DELETE", "/invites/{id}", Policy::Owns),
    ("POST", "/sse-ticket", Policy::Owns),
    ("GET", "/admin/users", Policy::Admin),
    ("PATCH", "/admin/users/{id}", Policy::Admin),
    ("GET", "/admin/tiers", Policy::Admin),
    ("POST", "/admin/tiers", Policy::Admin),
    ("PATCH", "/admin/tiers/{code}", Policy::Admin),
    ("GET", "/admin/invites", Policy::Admin),
    ("POST", "/admin/invites", Policy::Admin),
    ("DELETE", "/admin/invites/{id}", Policy::Admin),
    ("GET", "/courses", Policy::Owns),
    ("POST", "/courses", Policy::Owns),
    ("GET", "/courses/{id}", Policy::Read),
    ("PATCH", "/courses/{id}", Policy::Edit),
    ("DELETE", "/courses/{id}", Policy::Manage),
    ("POST", "/courses/{id}/restore", Policy::Manage),
    ("GET", "/courses/{id}/members", Policy::Manage),
    ("POST", "/courses/{id}/members", Policy::Manage),
    ("DELETE", "/courses/{id}/members/{memberId}", Policy::Manage),
    ("POST", "/courses/{id}/subscription", Policy::Read),
    ("DELETE", "/courses/{id}/subscription", Policy::Read),
    ("GET", "/course-invitations", Policy::Owns),
    ("POST", "/course-invitations/{courseId}/accept", Policy::Owns),
    ("POST", "/course-invitations/{courseId}/decline", Policy::Owns),
    ("GET", "/grade-scales", Policy::Owns),
    ("POST", "/grade-scales", Policy::Owns),
    ("GET", "/courses/{id}/components", Policy::Read),
    ("POST", "/courses/{id}/components", Policy::Edit),
    ("PATCH", "/components/{id}", Policy::Edit),
    ("DELETE", "/components/{id}", Policy::Edit),
    ("GET", "/courses/{id}/concepts", Policy::Read),
    ("POST", "/courses/{id}/concepts", Policy::Edit),
    ("GET", "/components/{id}/materials", Policy::Read),
    ("POST", "/components/{id}/materials", Policy::Edit),
    ("PATCH", "/materials/{id}", Policy::Edit),
    ("DELETE", "/materials/{id}", Policy::Edit),
    ("GET", "/courses/{id}/papers", Policy::Read),
    ("POST", "/courses/{id}/papers", Policy::Edit),
    ("PATCH", "/papers/{id}", Policy::Edit),
    ("GET", "/courses/{id}/questions", Policy::Read),
    ("POST", "/courses/{id}/questions", Policy::Read),
    ("GET", "/questions/{id}", Policy::Read),
    ("PATCH", "/questions/{id}", Policy::Edit),
    ("GET", "/questions/{id}/versions", Policy::Read),
    ("POST", "/questions/{id}/archive", Policy::Edit),
    ("POST", "/questions/{id}/approve", Policy::Edit),
    ("POST", "/questions/{id}/promote", Policy::Edit),
    ("POST", "/questions/{id}/image-crop", Policy::Edit),
    ("POST", "/courses/{id}/mocks", Policy::Subscribed),
    ("GET", "/courses/{id}/mocks", Policy::Subscribed),
    ("GET", "/mocks/{id}", Policy::Mine),
    ("POST", "/mocks/{id}/start", Policy::Mine),
    ("POST", "/mocks/{id}/submit", Policy::Mine),
    ("PUT", "/mocks/{id}/questions/{order}/answer", Policy::Mine),
    ("POST", "/mocks/{id}/questions/{order}/grade", Policy::Mine),
    ("POST", "/mocks/{id}/questions/{order}/show-answer", Policy::Mine),
    ("POST", "/mocks/{id}/questions/{order}/fixed", Policy::Mine),
    ("GET", "/mocks/{id}/result", Policy::Mine),
    ("POST", "/attempts", Policy::Owns),
    ("GET", "/slips", Policy::Owns),
    ("POST", "/slips", Policy::Owns),
    ("PATCH", "/slips/{id}", Policy::Mine),
    ("GET", "/courses/{id}/weak-spots", Policy::Read),
    ("POST", "/courses/{id}/weak-spots", Policy::Read),
    ("PATCH", "/weak-spots/{id}", Policy::Mine),
    ("GET", "/courses/{id}/traps", Policy::Read),
    ("POST", "/courses/{id}/traps", Policy::Read),
    ("DELETE", "/courses/{id}/traps/{trapId}", Policy::Mine),
    ("GET", "/insight/patterns", Policy::Owns),
    ("GET", "/courses/{id}/stats/confidence", Policy::Read),
    ("GET", "/courses/{id}/flashcards", Policy::Read),
    ("POST", "/courses/{id}/flashcards", Policy::Read),
    // Course cards: Edit. Personal cards: only their owner (also 404 to a course Editor).
    ("PATCH", "/flashcards/{id}", Policy::Edit),
    ("POST", "/flashcards/{id}/warmup-flag", Policy::Read),
    ("POST", "/flashcards/{id}/suspend", Policy::Read),
    ("GET", "/flash/queue", Policy::Owns),
    ("POST", "/flash/custom", Policy::Owns),
    ("POST", "/flash/reviews", Policy::Owns),
    ("GET", "/flash/cards", Policy::Owns),
    ("POST", "/events", Policy::Owns),
    ("GET", "/sessions", Policy::Owns),
    ("POST", "/sessions", Policy::Owns),
    // Your own progress, on a course you can read.
    ("GET", "/courses/{id}/progress", Policy::Read),
    ("PATCH", "/courses/{id}/progress", Policy::Read),
    ("GET", "/week", Policy::Owns),
    ("GET", "/courses/{id}/stats/summary", Policy::Read),
    ("GET", "/courses/{id}/deadlines", Policy::Subscribed),
    ("POST", "/courses/{id}/deadlines", Policy::Subscribed),
    ("PATCH", "/deadlines/{id}", Policy::Mine),
    ("PUT", "/deadlines/{id}/layout", Policy::Mine),
    ("GET", "/plan", Policy::Owns),
    ("POST", "/plan/rebuild", Policy::Owns),
    ("POST", "/plan/entries", Policy::Owns),
    ("PATCH", "/plan/entries/{id}", Policy::Mine),
    ("DELETE", "/plan/entries/{id}", Policy::Mine),
    ("GET", "/next", Policy::Owns),
    ("POST", "/next/advance", Policy::Owns),
    ("GET", "/courses/{id}/warmup", Policy::Subscribed),
    ("POST", "/materials/{id}/free-space", Policy::Manage),
    ("POST", "/materials/{id}/free-space/cancel", Policy::Manage),
    ("POST", "/courses/{id}/free-space", Policy::Manage),
];

pub fn policy_of(method: &str, path: &str) -> Option<Policy> {
    ROUTES.iter().find(|(m, p, _)| *m == method && *p == path).map(|(_, _, pol)| *pol)
}

/// Every `(METHOD, path)` in an OpenAPI document.
pub fn operations(api: &utoipa::openapi::OpenApi) -> Vec<(String, String)> {
    let mut ops = Vec::new();
    for (path, item) in &api.paths.paths {
        let methods = [
            ("GET", item.get.is_some()),
            ("POST", item.post.is_some()),
            ("PUT", item.put.is_some()),
            ("PATCH", item.patch.is_some()),
            ("DELETE", item.delete.is_some()),
        ];
        for (m, present) in methods {
            if present {
                ops.push((m.to_string(), path.clone()));
            }
        }
    }
    ops.sort();
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_has_exactly_one_policy_and_vice_versa() {
        let api = crate::openapi_doc();
        let ops = operations(&api);
        for (m, p) in &ops {
            assert!(policy_of(m, p).is_some(), "route {m} {p} is missing from policy_table::ROUTES");
        }
        for (m, p, _) in ROUTES {
            assert!(ops.contains(&(m.to_string(), p.to_string())), "policy for {m} {p} has no route");
        }
        let mut seen = std::collections::HashSet::new();
        for (m, p, _) in ROUTES {
            assert!(seen.insert((m, p)), "duplicate policy row {m} {p}");
        }
    }

    #[test]
    fn signed_in_routes_declare_bearer_security() {
        let api = crate::openapi_doc();
        for (path, item) in &api.paths.paths {
            for (m, op) in [("GET", &item.get), ("POST", &item.post), ("PUT", &item.put), ("PATCH", &item.patch), ("DELETE", &item.delete)]
            {
                let Some(op) = op else { continue };
                let needs_auth = policy_of(m, path).is_some_and(|p| p != Policy::Public);
                assert_eq!(op.security.is_some(), needs_auth, "{m} {path}: security annotation must match the policy");
            }
        }
    }
}
