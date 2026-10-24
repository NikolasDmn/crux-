//! User events and study time (backend-spec 5.7, 5.11): `POST /events` ingest against the type
//! registry, the server-side event helper, derived sessions, and timer/manual sessions.

use std::collections::HashMap;
use std::sync::OnceLock;

use chrono::{DateTime, Duration, Utc};
use crux_domain::study::{self, Ev, SessionKind};
use crux_domain::ValidationErrors;
use crux_repo::study::{self as repo, SessionRow};
use crux_repo::user_events::{self, NewUserEvent};
use crux_repo::MySqlConnection;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::courses::readable;
use crate::time::iso;
use crate::{Actor, AppError, AppResult, Ctx};

pub const MAX_EVENTS: usize = 200;
const MAX_PAYLOAD_BYTES: usize = 2048;
/// Derived sessions are rebuilt over this window (spec 5.7: the last 2 days).
pub const DERIVE_WINDOW: Duration = Duration::days(2);

const REGISTRY: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../contracts/user-events/registry.json"));

struct TypeDef {
    client: bool,
    payload: Option<jsonschema::Validator>,
}

fn registry() -> &'static HashMap<String, TypeDef> {
    static R: OnceLock<HashMap<String, TypeDef>> = OnceLock::new();
    R.get_or_init(|| {
        let doc: serde_json::Value = serde_json::from_str(REGISTRY).expect("user-events registry JSON");
        doc["types"]
            .as_object()
            .expect("registry types")
            .iter()
            .map(|(name, def)| {
                let payload = def.get("payload").map(|s| jsonschema::validator_for(s).expect("user-event payload schema compiles"));
                (name.clone(), TypeDef { client: def["source"] == "client", payload })
            })
            .collect()
    })
}

pub fn is_registered(event_type: &str) -> bool {
    registry().contains_key(event_type)
}

/// A server-emitted event (spec 5.11), in the caller's transaction.
#[allow(clippy::too_many_arguments)]
pub async fn server_event(
    conn: &mut MySqlConnection,
    user_id: i64,
    at: DateTime<Utc>,
    event_type: &str,
    course_id: Option<i64>,
    component_id: Option<i64>,
    entity: Option<(&str, i64)>,
    payload: serde_json::Value,
) -> AppResult<()> {
    debug_assert!(is_registered(event_type), "unregistered user event type {event_type}");
    user_events::insert(
        conn,
        &NewUserEvent {
            user_id,
            actor_user_id: None,
            client_id: None,
            occurred_at: at,
            event_type,
            course_id,
            component_id,
            entity_type: entity.map(|e| e.0),
            entity_id: entity.map(|e| e.1),
            payload: Some(payload),
        },
    )
    .await?;
    Ok(())
}

// ---------- POST /events ----------

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventInput {
    /// A ULID the device made; a retry with the same id is ignored.
    pub client_id: String,
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub course_id: Option<String>,
    pub component_id: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct EventBatch {
    pub events: Vec<EventInput>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Rejected {
    pub client_id: String,
    /// unknown_type, server_only, bad_payload, unknown_course, unknown_component
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventBatchResult {
    pub accepted: usize,
    pub duplicates: usize,
    pub rejected: Vec<Rejected>,
}

pub async fn ingest(ctx: &Ctx, actor: &Actor, batch: EventBatch) -> AppResult<EventBatchResult> {
    if batch.events.is_empty() || batch.events.len() > MAX_EVENTS {
        return Err(ValidationErrors::single("events", "size", "Send 1 to 200 events.").into());
    }
    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let mut courses: HashMap<String, Option<i64>> = HashMap::new();
    let (mut accepted, mut duplicates, mut rejected) = (0, 0, Vec::new());
    let mut seen = std::collections::HashSet::new();
    for e in batch.events {
        let reject = |reason: &str| Rejected { client_id: e.client_id.clone(), reason: reason.into() };
        if e.client_id.len() != 26 {
            rejected.push(reject("bad_client_id"));
            continue;
        }
        let Some(def) = registry().get(&e.event_type) else {
            rejected.push(reject("unknown_type"));
            continue;
        };
        if !def.client {
            rejected.push(reject("server_only"));
            continue;
        }
        let payload_ok = match (&def.payload, &e.payload) {
            (_, Some(p)) if p.to_string().len() > MAX_PAYLOAD_BYTES => false,
            (Some(v), Some(p)) => v.is_valid(p),
            (Some(v), None) => v.is_valid(&serde_json::json!({})),
            (None, _) => true,
        };
        if !payload_ok {
            rejected.push(reject("bad_payload"));
            continue;
        }
        if !seen.insert(e.client_id.clone()) || user_events::client_id_seen(&mut tx, actor.user_id, &e.client_id).await? {
            duplicates += 1;
            continue;
        }
        // Course and lecture: a lecture alone names its course.
        let mut course_ref = e.course_id.clone();
        let mut component_id = None;
        if let Some(k) = &e.component_id {
            match crux_repo::catalog::component(&mut tx, k).await? {
                Some(c) if c.archived_at.is_none() => {
                    let cref = crux_repo::courses::public_id_of(&mut tx, c.course_id).await?;
                    if course_ref.as_ref().is_some_and(|r| *r != cref) {
                        rejected.push(reject("unknown_component"));
                        continue;
                    }
                    course_ref = Some(cref);
                    component_id = Some(c.id);
                }
                _ => {
                    rejected.push(reject("unknown_component"));
                    continue;
                }
            }
        }
        let course_id = match &course_ref {
            Some(r) => {
                if !courses.contains_key(r) {
                    let id = readable(&mut tx, actor, r).await.ok().map(|c| c.id);
                    courses.insert(r.clone(), id);
                }
                match courses[r] {
                    Some(id) => Some(id),
                    None => {
                        rejected.push(reject("unknown_course"));
                        continue;
                    }
                }
            }
            None => None,
        };
        // Spec 5.11: occurredAt is clamped to [now − 7 d, now + 5 min].
        let at = e.occurred_at.clamp(now - Duration::days(7), now + Duration::minutes(5));
        let mut payload = e.payload.clone().unwrap_or_else(|| serde_json::json!({}));
        if let (Some(t), Some(id), Some(obj)) = (&e.entity_type, &e.entity_id, payload.as_object_mut()) {
            // Client entity ids are PublicIds; `UserEvent.EntityId` holds internal ids, so they ride in the payload.
            obj.insert("entity".into(), serde_json::json!({ "type": t, "id": id }));
        }
        user_events::insert(
            &mut tx,
            &NewUserEvent {
                user_id: actor.user_id,
                actor_user_id: None,
                client_id: Some(&e.client_id),
                occurred_at: at,
                event_type: &e.event_type,
                course_id,
                component_id,
                entity_type: None,
                entity_id: None,
                payload: Some(payload),
            },
        )
        .await?;
        accepted += 1;
    }
    tx.commit().await?;
    Ok(EventBatchResult { accepted, duplicates, rejected })
}

// ---------- derived sessions ----------

/// Rebuilds the user's derived sessions over the last 2 days (spec 5.7). Idempotent. Returns the
/// number of sessions written.
pub async fn derive_user(conn: &mut MySqlConnection, ctx: &Ctx, user_id: i64) -> AppResult<u64> {
    let now = ctx.clock.now();
    let mut from = now - DERIVE_WINDOW;
    // A derived session spanning the window start is rebuilt whole.
    if let Some(s) = repo::sessions_between(conn, user_id, from, from + Duration::seconds(1), None)
        .await?
        .iter()
        .filter(|s| s.source == "Derived")
        .map(|s| s.started_at)
        .min()
    {
        from = from.min(s);
    }
    let events: Vec<Ev> = user_events::between(conn, user_id, from, now + Duration::minutes(5))
        .await?
        .into_iter()
        .filter_map(|e| {
            Some(Ev {
                at: e.occurred_at,
                course_id: e.course_id?,
                component_id: e.component_id,
                kind: SessionKind::of_event(&e.event_type),
            })
        })
        .collect();
    let explicit = repo::explicit_from(conn, user_id, from).await?;
    let derived = study::drop_overlapping(study::derive(&events), &explicit);
    repo::delete_derived_from(conn, user_id, from).await?;
    for d in &derived {
        repo::insert_session(
            conn,
            &repo::NewSession {
                public_id: &ctx.ids.ulid(),
                user_id,
                course_id: d.course_id,
                component_id: d.component_id,
                kind: d.kind.as_db(),
                started_at: d.started_at,
                ended_at: d.ended_at,
                source: "Derived",
                note: None,
            },
        )
        .await?;
    }
    Ok(derived.len() as u64)
}

/// Nightly `sessions.derive` (spec 10): every user with events in the window.
pub async fn derive_all(ctx: &Ctx) -> anyhow::Result<u64> {
    let mut conn = ctx.pool.acquire().await?;
    let users = user_events::active_users_since(&mut conn, ctx.clock.now() - DERIVE_WINDOW).await?;
    let mut n = 0;
    for u in users {
        let mut tx = crux_repo::begin(&ctx.pool).await?;
        n += derive_user(&mut tx, ctx, u).await.map_err(|e| anyhow::anyhow!("derive user {u}: {e}"))?;
        tx.commit().await?;
    }
    Ok(n)
}

// ---------- sessions API ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: Option<String>,
    pub course_id: String,
    pub component_id: Option<String>,
    /// Lecture, Practice, Mock, Flashcards, Review, AiChat, Other.
    pub kind: String,
    pub started_at: String,
    pub ended_at: String,
    pub minutes: i64,
    /// Timer, Manual, or Derived (from your activity).
    pub source: String,
    pub note: Option<String>,
}

impl From<&SessionRow> for SessionView {
    fn from(s: &SessionRow) -> Self {
        Self {
            id: s.public_id.clone(),
            course_id: s.course_ref.clone(),
            component_id: s.component_ref.clone(),
            kind: s.kind.clone(),
            started_at: iso(s.started_at),
            ended_at: iso(s.ended_at),
            minutes: (s.ended_at - s.started_at).num_minutes(),
            source: s.source.clone(),
            note: s.note.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SessionQuery {
    /// Default: 7 days ago.
    pub from: Option<DateTime<Utc>>,
    /// Default: now.
    pub to: Option<DateTime<Utc>>,
    pub course_id: Option<String>,
}

pub async fn list(ctx: &Ctx, actor: &Actor, q: SessionQuery) -> AppResult<Vec<SessionView>> {
    let now = ctx.clock.now();
    let to = q.to.unwrap_or(now);
    let from = q.from.unwrap_or(to - Duration::days(7));
    if to <= from || to - from > Duration::days(366) {
        return Err(ValidationErrors::single("from", "range", "Pick a range of up to a year.").into());
    }
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = match &q.course_id {
        Some(r) => Some(readable(&mut tx, actor, r).await?.id),
        None => None,
    };
    derive_user(&mut tx, ctx, actor.user_id).await?;
    let rows = repo::sessions_between(&mut tx, actor.user_id, from, to, course).await?;
    tx.commit().await?;
    Ok(rows.iter().map(SessionView::from).collect())
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionInput {
    pub course_id: String,
    pub component_id: Option<String>,
    /// Lecture, Practice, Mock, Flashcards, Review, AiChat, Other.
    pub kind: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    /// Timer (default) or Manual.
    pub source: Option<String>,
    pub note: Option<String>,
}

pub async fn create(ctx: &Ctx, actor: &Actor, input: SessionInput) -> AppResult<SessionView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, &input.course_id).await?;
    let now = ctx.clock.now();
    let mut e = ValidationErrors::new();
    let kind = SessionKind::from_db(&input.kind);
    if kind.is_none() {
        e.push("kind", "invalid", "Use Lecture, Practice, Mock, Flashcards, Review, AiChat or Other.");
    }
    let source = input.source.as_deref().unwrap_or("Timer");
    if !matches!(source, "Timer" | "Manual") {
        e.push("source", "invalid", "Use Timer or Manual.");
    }
    if input.ended_at <= input.started_at {
        e.push("endedAt", "order", "A session ends after it starts.");
    } else if input.ended_at - input.started_at >= Duration::hours(24) {
        e.push("endedAt", "too_long", "A session is shorter than 24 hours.");
    }
    if input.ended_at > now + Duration::minutes(5) {
        e.push("endedAt", "future", "This session ends in the future.");
    }
    if input.note.as_ref().is_some_and(|n| n.len() > 255) {
        e.push("note", "too_long", "Up to 255 characters.");
    }
    e.into_result()?;
    let component_id = match &input.component_id {
        Some(r) => Some(
            crux_repo::catalog::component_ids_by_public(&mut tx, course.id, std::slice::from_ref(r))
                .await?
                .first()
                .map(|c| c.0)
                .ok_or_else(|| ValidationErrors::single("componentId", "unknown", "Pick a lecture of this course."))?,
        ),
        None => None,
    };
    let public_id = ctx.ids.ulid();
    repo::insert_session(
        &mut tx,
        &repo::NewSession {
            public_id: &public_id,
            user_id: actor.user_id,
            course_id: course.id,
            component_id,
            kind: kind.expect("validated").as_db(),
            started_at: input.started_at,
            ended_at: input.ended_at,
            source,
            note: input.note.as_deref(),
        },
    )
    .await?;
    // Explicit sessions win over derived ones that overlap them.
    derive_user(&mut tx, ctx, actor.user_id).await?;
    let row = repo::sessions_between(&mut tx, actor.user_id, input.started_at, input.ended_at, Some(course.id))
        .await?
        .into_iter()
        .find(|s| s.public_id.as_deref() == Some(public_id.as_str()))
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(SessionView::from(&row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_and_knows_client_from_server_types() {
        let r = registry();
        assert!(r["material.page"].client);
        assert!(!r["mock.start"].client);
        let v = r["material.page"].payload.as_ref().unwrap();
        assert!(v.is_valid(&serde_json::json!({"page": 3})));
        assert!(!v.is_valid(&serde_json::json!({"page": 0})));
        for t in [
            "mock.start",
            "mock.answer",
            "mock.submit",
            "mock.graded",
            "mock.reviewed",
            "attempt",
            "flash.review",
            "plan.move",
            "next.step_done",
        ] {
            assert!(is_registered(t), "{t}");
        }
    }
}
