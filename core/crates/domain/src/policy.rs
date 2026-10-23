//! Authorization predicates (backend-spec 4.2). Every service function calls these; list queries
//! apply the same predicate in SQL. A failed check is `404 not_found`, never 403.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemberRole {
    Viewer,
    Editor,
    Owner,
}

/// What the policy needs to know about a course and the actor's membership of it.
#[derive(Debug, Clone, Copy)]
pub struct CourseAccess {
    pub actor_is_owner: bool,
    pub member_role: Option<MemberRole>,
}

pub fn member_role(access: &CourseAccess) -> Option<MemberRole> {
    if access.actor_is_owner { Some(MemberRole::Owner) } else { access.member_role }
}

pub fn can_read_course(access: &CourseAccess) -> bool {
    member_role(access).is_some()
}

pub fn can_edit_course(access: &CourseAccess) -> bool {
    matches!(member_role(access), Some(MemberRole::Owner | MemberRole::Editor))
}

/// Sharing, colour, grade scale, storage saver, delete. Course colour is Owner-only (spec 4.2).
pub fn can_manage_course(access: &CourseAccess) -> bool {
    member_role(access) == Some(MemberRole::Owner)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewState {
    Approved,
    Draft,
    Rejected,
}

#[derive(Debug, Clone, Copy)]
pub struct QuestionVisibility {
    /// `Some(true)` = owned by the actor, `Some(false)` = someone else's private question,
    /// `None` = part of the course bank.
    pub owned_by_actor: Option<bool>,
    pub review_state: ReviewState,
}

pub fn can_see_question(course: &CourseAccess, q: &QuestionVisibility) -> bool {
    let own = q.owned_by_actor == Some(true);
    can_read_course(course) && (q.owned_by_actor.is_none() || own) && (q.review_state == ReviewState::Approved || own)
}

/// Spec 4.2 (Nick 2026-10-23): an active subscription and the mock is the actor's own.
pub fn can_open_mock(has_active_subscription: bool, mock_is_actors: bool) -> bool {
    has_active_subscription && mock_is_actors
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: CourseAccess = CourseAccess { actor_is_owner: true, member_role: None };
    const EDITOR: CourseAccess = CourseAccess { actor_is_owner: false, member_role: Some(MemberRole::Editor) };
    const VIEWER: CourseAccess = CourseAccess { actor_is_owner: false, member_role: Some(MemberRole::Viewer) };
    const STRANGER: CourseAccess = CourseAccess { actor_is_owner: false, member_role: None };

    #[test]
    fn course_predicates_by_role() {
        let table = [
            (OWNER, true, true, true),
            (EDITOR, true, true, false),
            (VIEWER, true, false, false),
            (STRANGER, false, false, false),
        ];
        for (access, read, edit, manage) in table {
            assert_eq!(can_read_course(&access), read, "{access:?}");
            assert_eq!(can_edit_course(&access), edit, "{access:?}");
            assert_eq!(can_manage_course(&access), manage, "{access:?}");
        }
    }

    #[test]
    fn owner_flag_beats_a_stale_member_row() {
        let both = CourseAccess { actor_is_owner: true, member_role: Some(MemberRole::Viewer) };
        assert_eq!(member_role(&both), Some(MemberRole::Owner));
    }

    #[test]
    fn question_visibility() {
        let bank_approved = QuestionVisibility { owned_by_actor: None, review_state: ReviewState::Approved };
        let bank_draft = QuestionVisibility { owned_by_actor: None, review_state: ReviewState::Draft };
        let mine_draft = QuestionVisibility { owned_by_actor: Some(true), review_state: ReviewState::Draft };
        let theirs = QuestionVisibility { owned_by_actor: Some(false), review_state: ReviewState::Approved };

        assert!(can_see_question(&VIEWER, &bank_approved));
        assert!(!can_see_question(&VIEWER, &bank_draft));
        assert!(can_see_question(&VIEWER, &mine_draft));
        assert!(!can_see_question(&OWNER, &theirs), "private questions stay private even from the course owner");
        assert!(!can_see_question(&STRANGER, &bank_approved));
        assert!(!can_see_question(&STRANGER, &mine_draft), "own question in a course you lost access to");
    }

    #[test]
    fn mocks_need_subscription_and_ownership() {
        assert!(can_open_mock(true, true));
        assert!(!can_open_mock(true, false));
        assert!(!can_open_mock(false, true));
    }
}
