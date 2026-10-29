//! Courses, sharing, subscriptions, grade scales (backend-spec 4.2, 5.1; decisions D2-1..D2-6).

use chrono::Duration;
use crux_domain::auth::{normalize_invite_code, token_hash};
use crux_domain::catalog::{self, CourseFields, GradeScaleFields, ShareTarget};
use crux_domain::policy::{can_edit_course, can_manage_course, can_read_course, CourseAccess, MemberRole};
use crux_domain::tier::{may_create_course, TierLimits};
use crux_domain::{topics, ValidationErrors};
use crux_repo::courses::{self, CourseRow};
use crux_repo::{invites, tiers, users, MySqlConnection, RepoError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::events::{emit, request_account_email};
use crate::time::{iso, iso_opt};
use crate::{Actor, AppError, AppResult, Ctx};

// ---------- views ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Ref {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CourseView {
    pub id: String,
    pub name: String,
    pub code: String,
    pub institution: Option<String>,
    pub term: Option<String>,
    pub color: String,
    pub grade_scale: Ref,
    pub owner: Ref,
    /// Owner, Editor or Viewer.
    pub my_role: String,
    /// Active, Passed, Failed, Archived; absent when not subscribed.
    pub my_subscription: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Set while the course is in the trash (D2-5).
    pub trashed_at: Option<String>,
    pub purge_at: Option<String>,
    pub version: i32,
}

impl From<&CourseRow> for CourseView {
    fn from(c: &CourseRow) -> Self {
        Self {
            id: c.public_id.clone(),
            name: c.name.clone(),
            code: c.code.clone(),
            institution: c.institution.clone(),
            term: c.term.clone(),
            color: c.color.clone(),
            grade_scale: Ref { id: c.grade_scale_ref.clone().unwrap_or_default(), name: c.grade_scale_name.clone() },
            owner: Ref { id: c.owner_ref.clone(), name: c.owner_username.clone() },
            my_role: c.my_role.clone(),
            my_subscription: c.my_subscription.clone(),
            created_at: iso(c.created_at),
            updated_at: iso(c.updated_at),
            trashed_at: iso_opt(c.archived_at),
            purge_at: c.archived_at.map(|a| iso(catalog::in_trash_until(a))),
            version: c.row_version,
        }
    }
}

pub fn access_of(c: &CourseRow) -> CourseAccess {
    let member_role = match c.my_role.as_str() {
        "Editor" => Some(MemberRole::Editor),
        "Viewer" => Some(MemberRole::Viewer),
        _ => None,
    };
    CourseAccess { actor_is_owner: c.my_role == "Owner", member_role }
}

// ---------- access helpers used across the catalog ----------

/// Loads the course the actor can read, or 404.
pub async fn readable(conn: &mut MySqlConnection, actor: &Actor, course_ref: &str) -> AppResult<CourseRow> {
    let c = courses::for_actor(conn, actor.user_id, course_ref, false).await?.ok_or(AppError::NotFound)?;
    if can_read_course(&access_of(&c)) {
        Ok(c)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn editable(conn: &mut MySqlConnection, actor: &Actor, course_ref: &str) -> AppResult<CourseRow> {
    let c = readable(conn, actor, course_ref).await?;
    if can_edit_course(&access_of(&c)) {
        Ok(c)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn manageable(conn: &mut MySqlConnection, actor: &Actor, course_ref: &str) -> AppResult<CourseRow> {
    let c = readable(conn, actor, course_ref).await?;
    if can_manage_course(&access_of(&c)) {
        Ok(c)
    } else {
        Err(AppError::NotFound)
    }
}

/// The `course.*` payload: owner + accepted members (spec 3.3).
async fn emit_course(
    conn: &mut MySqlConnection,
    ctx: &Ctx,
    topic: &str,
    course_id: i64,
    course_ref: &str,
    owner_ref: &str,
    color: &str,
) -> AppResult<()> {
    let members = courses::member_refs(conn, course_id).await?;
    emit(
        conn,
        ctx,
        topic,
        Some(course_ref),
        json!({ "courseRef": course_ref, "ownerRef": owner_ref, "memberRefs": members, "color": color }),
    )
    .await
}

fn code_taken(e: RepoError) -> AppError {
    match e {
        RepoError::Duplicate(k) if k.contains("owner_code") => {
            ValidationErrors::single("code", "taken", "You already have a course with this code.").into()
        }
        other => other.into(),
    }
}

// ---------- courses ----------

pub async fn list(ctx: &Ctx, actor: &Actor, trashed: bool) -> AppResult<Vec<CourseView>> {
    let mut conn = ctx.pool.acquire().await?;
    Ok(courses::list_for_actor(&mut conn, actor.user_id, trashed).await?.iter().map(CourseView::from).collect())
}

pub async fn get(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<CourseView> {
    let mut conn = ctx.pool.acquire().await?;
    Ok(CourseView::from(&readable(&mut conn, actor, course_ref).await?))
}

pub async fn create(ctx: &Ctx, actor: &Actor, f: CourseFields) -> AppResult<CourseView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let id = create_in(&mut tx, ctx, actor, f).await?;
    let c = courses::by_internal_id(&mut tx, actor.user_id, id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(CourseView::from(&c))
}

/// Creating a course inside the caller's transaction (also used by the AI import apply).
pub(crate) async fn create_in(tx: &mut MySqlConnection, ctx: &Ctx, actor: &Actor, f: CourseFields) -> AppResult<i64> {
    catalog::validate_course(&f, true).into_result()?;
    let user = users::by_id(&mut *tx, actor.user_id).await?.ok_or(AppError::NotFound)?;
    let tier = tiers::by_id(&mut *tx, user.tier_id).await?.ok_or(AppError::NotFound)?;
    let limits = TierLimits { storage_quota_bytes: tier.storage_quota_bytes, max_courses: tier.max_courses, extra: None };
    if !may_create_course(&limits, courses::count_owned_active(&mut *tx, actor.user_id).await?) {
        return Err(ValidationErrors::single("tier", "max_courses", "Your plan's course limit is reached.").into());
    }
    let scale_ref = f.grade_scale_id.as_deref().unwrap_or(catalog::DEFAULT_GRADE_SCALE_PUBLIC_ID);
    let scale = courses::grade_scale_usable_by(&mut *tx, actor.user_id, scale_ref)
        .await?
        .ok_or_else(|| ValidationErrors::single("gradeScaleId", "unknown", "No such grade scale."))?;
    let color = match &f.color {
        Some(c) => c.to_lowercase(),
        None => catalog::next_color(courses::count_owned_ever(&mut *tx, actor.user_id).await?).to_string(),
    };
    let public_id = ctx.ids.ulid();
    let id = courses::insert(
        &mut *tx,
        &courses::NewCourse {
            public_id: &public_id,
            owner: actor.user_id,
            name: f.name.as_deref().unwrap_or_default().trim(),
            code: &f.code.as_deref().unwrap_or_default().to_uppercase(),
            institution: f.institution.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            term: f.term.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            color: &color,
            grade_scale_id: scale.id,
        },
    )
    .await
    .map_err(code_taken)?;
    // The owner studies their own course: subscribe them straight away.
    courses::subscribe(&mut *tx, actor.user_id, id, ctx.clock.now().date_naive()).await?;
    emit_course(&mut *tx, ctx, topics::COURSE_CREATED, id, &public_id, &actor.public_id, &color).await?;
    Ok(id)
}

/// Editors may change name, code, institution, term; colour and grade scale are owner-only.
pub async fn patch(ctx: &Ctx, actor: &Actor, course_ref: &str, version: i32, f: CourseFields) -> AppResult<CourseView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let c = courses::for_actor_for_update(&mut tx, actor.user_id, course_ref).await?.ok_or(AppError::NotFound)?;
    let access = access_of(&c);
    if !can_edit_course(&access) || (catalog::touches_owner_only_fields(&f) && !can_manage_course(&access)) {
        return Err(AppError::NotFound);
    }
    catalog::validate_course(&f, false).into_result()?;
    if c.row_version != version {
        return Err(AppError::Conflict {
            message: "Changed somewhere else.".into(),
            current: serde_json::to_value(CourseView::from(&c)).ok(),
        });
    }
    let scale_id = match &f.grade_scale_id {
        Some(r) => {
            courses::grade_scale_usable_by(&mut tx, actor.user_id, r)
                .await?
                .ok_or_else(|| ValidationErrors::single("gradeScaleId", "unknown", "No such grade scale."))?
                .id
        }
        None => courses::grade_scale_by_public_id(&mut tx, c.grade_scale_ref.as_deref().unwrap_or_default())
            .await?
            .map(|g| g.id)
            .ok_or(AppError::NotFound)?,
    };
    let color = f.color.as_deref().map(str::to_lowercase).unwrap_or_else(|| c.color.clone());
    let code = f.code.as_deref().map(str::to_uppercase).unwrap_or_else(|| c.code.clone());
    courses::update(
        &mut tx,
        c.id,
        version,
        &courses::CourseUpdate {
            name: f.name.as_deref().map(str::trim).unwrap_or(&c.name),
            code: &code,
            institution: match &f.institution {
                Some(i) => Some(i.trim()).filter(|s| !s.is_empty()),
                None => c.institution.as_deref(),
            },
            term: match &f.term {
                Some(t) => Some(t.trim()).filter(|s| !s.is_empty()),
                None => c.term.as_deref(),
            },
            color: &color,
            grade_scale_id: scale_id,
        },
    )
    .await
    .map_err(code_taken)?;
    emit_course(&mut tx, ctx, topics::COURSE_UPDATED, c.id, &c.public_id, &c.owner_ref, &color).await?;
    let fresh = courses::by_internal_id(&mut tx, actor.user_id, c.id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(CourseView::from(&fresh))
}

/// D2-5: owner moves the course to the trash; hidden from everyone for 30 days, then purged.
pub async fn trash(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<CourseView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let c = manageable(&mut tx, actor, course_ref).await?;
    courses::set_archived(&mut tx, c.id, Some(ctx.clock.now())).await?;
    emit_course(&mut tx, ctx, topics::COURSE_ARCHIVED, c.id, &c.public_id, &c.owner_ref, &c.color).await?;
    let fresh = courses::by_internal_id(&mut tx, actor.user_id, c.id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(CourseView::from(&fresh))
}

pub async fn restore(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<CourseView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let c = courses::for_actor(&mut tx, actor.user_id, course_ref, true).await?.ok_or(AppError::NotFound)?;
    if c.my_role != "Owner" || c.archived_at.is_none() {
        return Err(AppError::NotFound);
    }
    courses::set_archived(&mut tx, c.id, None).await?;
    emit_course(&mut tx, ctx, topics::COURSE_UPDATED, c.id, &c.public_id, &c.owner_ref, &c.color).await?;
    let fresh = courses::by_internal_id(&mut tx, actor.user_id, c.id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(CourseView::from(&fresh))
}

/// Purge job (daily): trashed courses past their 30 days are deleted for good.
pub async fn purge_trashed(ctx: &Ctx) -> anyhow::Result<u64> {
    let cutoff = ctx.clock.now() - Duration::days(catalog::TRASH_DAYS);
    let mut n = 0;
    let mut conn = ctx.pool.acquire().await?;
    for (id, public_id) in courses::due_for_purge(&mut conn, cutoff).await? {
        let mut tx = crux_repo::begin(&ctx.pool).await?;
        let owner: Option<(String, String)> = owner_and_color(&mut tx, id).await?;
        courses::hard_delete(&mut tx, id).await?;
        if let Some((owner_ref, color)) = owner {
            emit(
                &mut tx,
                ctx,
                topics::COURSE_DELETED,
                Some(&public_id),
                json!({ "courseRef": public_id, "ownerRef": owner_ref, "memberRefs": [], "color": color }),
            )
            .await?;
        }
        tx.commit().await?;
        n += 1;
    }
    Ok(n)
}

async fn owner_and_color(conn: &mut MySqlConnection, course_id: i64) -> AppResult<Option<(String, String)>> {
    Ok(crux_repo::courses::owner_ref_and_color(conn, course_id).await?)
}

// ---------- subscriptions ----------

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInput {
    /// Active (default) or Archived.
    #[serde(default)]
    pub status: Option<String>,
}

/// Subscribe (or re-activate / archive) the actor's subscription. Needs read access.
pub async fn subscribe(ctx: &Ctx, actor: &Actor, course_ref: &str, input: SubscriptionInput) -> AppResult<CourseView> {
    let status = input.status.unwrap_or_else(|| "Active".into());
    if !matches!(status.as_str(), "Active" | "Archived") {
        return Err(ValidationErrors::single("status", "invalid", "Use Active or Archived.").into());
    }
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let c = readable(&mut tx, actor, course_ref).await?;
    courses::subscribe(&mut tx, actor.user_id, c.id, ctx.clock.now().date_naive()).await?;
    if status == "Archived" {
        courses::set_subscription_status(&mut tx, actor.user_id, c.id, "Archived").await?;
    }
    crate::flash::on_subscription_status(&mut tx, actor.user_id, c.id, status == "Archived").await?;
    // A course added or archived changes the plan (spec 5.8).
    crate::plan::rebuild_in(&mut tx, ctx, actor.user_id).await?;
    let fresh = courses::by_internal_id(&mut tx, actor.user_id, c.id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(CourseView::from(&fresh))
}

pub async fn unsubscribe(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<()> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let c = readable(&mut tx, actor, course_ref).await?;
    courses::unsubscribe(&mut tx, actor.user_id, c.id).await?;
    crate::plan::rebuild_in(&mut tx, ctx, actor.user_id).await?;
    tx.commit().await?;
    Ok(())
}

// ---------- sharing ----------

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShareInput {
    /// A username, or an email address (D2-1: emails without an account get an invite link).
    pub username_or_email: String,
    /// Editor or Viewer.
    pub role: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberView {
    pub user_id: String,
    pub username: String,
    pub name: String,
    pub role: String,
    /// Pending until they accept (D2-2), then Accepted.
    pub status: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmailShareView {
    /// Pass to `DELETE /courses/{id}/members/{memberId}` to withdraw it.
    pub id: String,
    pub email: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MembersView {
    pub owner: Ref,
    pub members: Vec<MemberView>,
    /// Shared with an email that has no account yet.
    pub pending_emails: Vec<EmailShareView>,
}

async fn members_view(conn: &mut MySqlConnection, c: &CourseRow) -> AppResult<MembersView> {
    let members = courses::members(conn, c.id).await?;
    let emails = courses::open_share_invites(conn, c.id).await?;
    Ok(MembersView {
        owner: Ref { id: c.owner_ref.clone(), name: c.owner_username.clone() },
        members: members
            .iter()
            .map(|m| MemberView {
                user_id: m.user_ref.clone(),
                username: m.username.clone(),
                name: format!("{} {}", m.first_name, m.last_name),
                role: m.role.clone(),
                status: m.status.clone(),
                added_at: iso(m.added_at),
            })
            .collect(),
        pending_emails: emails
            .iter()
            .map(|s| EmailShareView {
                id: s.public_id.clone(),
                email: s.email.clone(),
                role: s.role.clone(),
                created_at: iso(s.created_at),
            })
            .collect(),
    })
}

pub async fn members(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<MembersView> {
    let mut conn = ctx.pool.acquire().await?;
    let c = manageable(&mut conn, actor, course_ref).await?;
    members_view(&mut conn, &c).await
}

/// Invite expiry for course-share invite links.
const SHARE_INVITE_DAYS: i64 = 30;

pub async fn share(ctx: &Ctx, actor: &Actor, course_ref: &str, input: ShareInput) -> AppResult<MembersView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let c = manageable(&mut tx, actor, course_ref).await?;
    if !matches!(input.role.as_str(), "Editor" | "Viewer") {
        return Err(ValidationErrors::single("role", "invalid", "Use Editor or Viewer.").into());
    }
    let me = users::by_id(&mut tx, actor.user_id).await?.ok_or(AppError::NotFound)?;
    let target = catalog::share_target(&input.username_or_email);
    let user = match &target {
        ShareTarget::Email(e) => {
            crux_domain::auth::validate_email(e)
                .into_result()
                .map_err(|_| ValidationErrors::single("usernameOrEmail", "invalid", "Enter a username or an email."))?;
            users::by_email(&mut tx, e).await?
        }
        ShareTarget::Username(u) => Some(users::by_username(&mut tx, u).await?.ok_or_else(|| {
            ValidationErrors::single("usernameOrEmail", "unknown", "No Crux user with that username. Use their email to invite them.")
        })?),
    };
    match user {
        Some(u) if u.id == actor.user_id => {
            return Err(ValidationErrors::single("usernameOrEmail", "self", "You own this course.").into());
        }
        Some(u) => {
            let existing = courses::member(&mut tx, c.id, u.id).await?;
            courses::upsert_member(&mut tx, c.id, u.id, &input.role, actor.user_id, false).await?;
            match existing {
                // A role change for someone who already accepted takes effect now.
                Some(m) if m.status == "Accepted" && m.role != input.role => {
                    emit(
                        &mut tx,
                        ctx,
                        topics::COURSE_MEMBER_CHANGED,
                        Some(&c.public_id),
                        json!({ "courseRef": c.public_id, "userRef": u.public_id, "role": input.role }),
                    )
                    .await?;
                }
                Some(_) => {}
                None => {
                    // "Course shared with you": always shown in-app (spec 8), email if enabled.
                    emit(
                        &mut tx,
                        ctx,
                        topics::NOTIFY_REQUESTED,
                        Some(&u.public_id),
                        json!({
                            "userRef": u.public_id,
                            "kind": "course.shared",
                            "dedupeKey": format!("course.shared:{}:{}", c.public_id, u.public_id),
                            "dueAt": iso(ctx.clock.now()),
                            "payload": {
                                "courseId": c.public_id, "courseName": c.name, "role": input.role,
                                "inviterName": me.username, "byUsername": me.username,
                            },
                        }),
                    )
                    .await?;
                }
            }
        }
        None => {
            let ShareTarget::Email(email) = target else { unreachable!("usernames resolve or error above") };
            if !actor.email_verified {
                return Err(AppError::EmailUnverified);
            }
            if courses::open_share_invite_for_email(&mut tx, c.id, &email).await?.is_none() {
                share_by_email(&mut tx, ctx, actor, &me, &c, &email, &input.role).await?;
            }
        }
    }
    let view = members_view(&mut tx, &c).await?;
    tx.commit().await?;
    Ok(view)
}

/// D2-1: an email-bound invite + a CourseShareInvite, and the email with the link.
async fn share_by_email(
    tx: &mut MySqlConnection,
    ctx: &Ctx,
    actor: &Actor,
    me: &users::UserRow,
    c: &CourseRow,
    email: &str,
    role: &str,
) -> AppResult<()> {
    // Audit M5: shares by email count against the same daily limit as invite emails.
    crate::ratelimit::hit(ctx, &crate::ratelimit::INVITE_EMAIL_PER_USER, &actor.public_id).await?;
    let code = ctx.ids.invite_code();
    let invite_ref = ctx.ids.ulid();
    let invite_id = invites::insert(
        tx,
        &invites::NewInvite {
            public_id: &invite_ref,
            code_hash: &token_hash(&normalize_invite_code(&code)),
            email: Some(email),
            invited_by: Some(actor.user_id),
            tier_id: None,
            max_uses: 1,
            expires_at: Some(ctx.clock.now() + Duration::days(SHARE_INVITE_DAYS)),
        },
    )
    .await?;
    courses::insert_share_invite(
        tx,
        &courses::NewShareInvite { public_id: &ctx.ids.ulid(), course_id: c.id, email, role, invite_id, invited_by: actor.user_id },
    )
    .await?;
    request_account_email(
        tx,
        ctx,
        &actor.public_id,
        "invite.course_shared",
        &format!("invite.course_shared:{invite_ref}"),
        json!({
            "to": email,
            "inviterName": format!("{} {}", me.first_name, me.last_name),
            "courseName": c.name,
            "link": format!("{}/signup?invite={code}", ctx.config.app_base_url),
        }),
    )
    .await
}

/// `memberId` is a member's user id, or a pending email share's id. The owner removes anyone; a
/// member may remove themselves (leave).
pub async fn remove_member(ctx: &Ctx, actor: &Actor, course_ref: &str, member_ref: &str) -> AppResult<()> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let c = readable(&mut tx, actor, course_ref).await?;
    let is_owner = can_manage_course(&access_of(&c));
    if member_ref == actor.public_id && !is_owner {
        courses::remove_member(&mut tx, c.id, actor.user_id).await?;
        emit(
            &mut tx,
            ctx,
            topics::COURSE_MEMBER_CHANGED,
            Some(&c.public_id),
            json!({ "courseRef": c.public_id, "userRef": actor.public_id, "role": null }),
        )
        .await?;
        tx.commit().await?;
        return Ok(());
    }
    if !is_owner {
        return Err(AppError::NotFound);
    }
    if let Some(share) = courses::share_invite_by_public_id(&mut tx, c.id, member_ref).await? {
        courses::mark_share_invite(&mut tx, share.id, false, ctx.clock.now()).await?;
        invites::revoke(&mut tx, share.invite_id, ctx.clock.now()).await?;
    } else {
        let user = users::by_public_id(&mut tx, member_ref).await?.ok_or(AppError::NotFound)?;
        let existing = courses::member(&mut tx, c.id, user.id).await?.ok_or(AppError::NotFound)?;
        courses::remove_member(&mut tx, c.id, user.id).await?;
        if existing.status == "Accepted" {
            emit(
                &mut tx,
                ctx,
                topics::COURSE_MEMBER_CHANGED,
                Some(&c.public_id),
                json!({ "courseRef": c.public_id, "userRef": user.public_id, "role": null }),
            )
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationView {
    /// The course id; accept or decline with it.
    pub course_id: String,
    pub course_name: String,
    pub course_code: String,
    pub color: String,
    pub role: String,
    pub invited_by: Option<String>,
    pub invited_at: String,
}

pub async fn invitations(ctx: &Ctx, actor: &Actor) -> AppResult<Vec<InvitationView>> {
    let mut conn = ctx.pool.acquire().await?;
    Ok(courses::pending_invitations(&mut conn, actor.user_id)
        .await?
        .into_iter()
        .map(|i| InvitationView {
            course_id: i.course_ref,
            course_name: i.course_name,
            course_code: i.course_code,
            color: i.color,
            role: i.role,
            invited_by: i.invited_by_username,
            invited_at: iso(i.added_at),
        })
        .collect())
}

/// D2-2: accepting a share makes the course appear. Subscribing (to track progress) is separate.
pub async fn respond_to_invitation(ctx: &Ctx, actor: &Actor, course_ref: &str, accept: bool) -> AppResult<Option<CourseView>> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let inv = courses::pending_invitation(&mut tx, actor.user_id, course_ref).await?.ok_or(AppError::NotFound)?;
    let out = if accept {
        courses::accept_member(&mut tx, inv.course_id, actor.user_id, ctx.clock.now()).await?;
        emit(
            &mut tx,
            ctx,
            topics::COURSE_MEMBER_CHANGED,
            Some(&inv.course_ref),
            json!({ "courseRef": inv.course_ref, "userRef": actor.public_id, "role": inv.role }),
        )
        .await?;
        Some(CourseView::from(&courses::by_internal_id(&mut tx, actor.user_id, inv.course_id).await?.ok_or(AppError::NotFound)?))
    } else {
        courses::remove_member(&mut tx, inv.course_id, actor.user_id).await?;
        None
    };
    tx.commit().await?;
    Ok(out)
}

/// D2-3: called inside signup. Shares attached to the invite become accepted memberships.
pub async fn accept_shares_for_invite(
    conn: &mut MySqlConnection,
    ctx: &Ctx,
    invite_id: i64,
    user_id: i64,
    user_ref: &str,
    email: &str,
) -> AppResult<()> {
    for share in courses::share_invites_for_invite(conn, invite_id).await? {
        if share.email != email {
            continue;
        }
        let added_by = share.invited_by.unwrap_or(user_id);
        courses::upsert_member(conn, share.course_id, user_id, &share.role, added_by, true).await?;
        courses::mark_share_invite(conn, share.id, true, ctx.clock.now()).await?;
        let course_ref: String = courses::public_id_of(conn, share.course_id).await?;
        emit(
            conn,
            ctx,
            topics::COURSE_MEMBER_CHANGED,
            Some(&course_ref),
            json!({ "courseRef": course_ref, "userRef": user_ref, "role": share.role }),
        )
        .await?;
    }
    Ok(())
}

// ---------- grade scales ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GradeScaleView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub built_in: bool,
    pub scale_min: Option<f64>,
    pub scale_max: Option<f64>,
    pub step: Option<f64>,
    pub pass_value: Option<f64>,
    #[schema(value_type = Option<Object>)]
    pub labels: Option<serde_json::Value>,
}

impl From<&courses::GradeScaleRow> for GradeScaleView {
    fn from(g: &courses::GradeScaleRow) -> Self {
        Self {
            id: g.public_id.clone().unwrap_or_default(),
            name: g.name.clone(),
            kind: g.kind.clone(),
            built_in: g.owner_user_id.is_none(),
            scale_min: g.scale_min,
            scale_max: g.scale_max,
            step: g.step,
            pass_value: g.pass_value,
            labels: g.labels.as_ref().map(|j| j.0.clone()),
        }
    }
}

pub async fn grade_scales(ctx: &Ctx, actor: &Actor) -> AppResult<Vec<GradeScaleView>> {
    let mut conn = ctx.pool.acquire().await?;
    Ok(courses::grade_scales_for(&mut conn, actor.user_id).await?.iter().map(GradeScaleView::from).collect())
}

pub async fn create_grade_scale(ctx: &Ctx, actor: &Actor, f: GradeScaleFields) -> AppResult<GradeScaleView> {
    catalog::validate_grade_scale(&f).into_result()?;
    let numeric = f.kind == "Numeric";
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let public_id = ctx.ids.ulid();
    courses::insert_grade_scale(
        &mut tx,
        &courses::NewGradeScale {
            public_id: &public_id,
            owner: actor.user_id,
            name: f.name.trim(),
            kind: &f.kind,
            scale_min: f.scale_min.filter(|_| numeric),
            scale_max: f.scale_max.filter(|_| numeric),
            step: f.step.filter(|_| numeric),
            pass_value: f.pass_value.filter(|_| numeric),
            labels: (!numeric).then(|| {
                serde_json::to_value(
                    f.labels
                        .as_ref()
                        .map(|ls| ls.iter().map(|l| json!({"label": l.label, "value": l.value, "pass": l.pass})).collect::<Vec<_>>()),
                )
                .unwrap_or_default()
            }),
        },
    )
    .await?;
    let g = courses::grade_scale_by_public_id(&mut tx, &public_id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(GradeScaleView::from(&g))
}
