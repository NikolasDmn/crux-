"""Preferences, inbox, unsubscribe and the bounce webhook (spec 8). One transaction per call."""

import base64
import hmac
from typing import Any

from pydantic import BaseModel, ConfigDict, Field
from pydantic.alias_generators import to_camel
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.domain import kinds, unsubscribe
from app.repo import notifications, preferences, recipients
from app.service.requests import inbox_item
from crux_common.auth import Principal
from crux_common.errors import NotFound, ValidationFailed
from crux_common.timeutil import utcnow


class Camel(BaseModel):
    model_config = ConfigDict(alias_generator=to_camel, populate_by_name=True)


class KindPreference(Camel):
    kind: str
    label: str
    description: str
    in_app: bool
    email: bool
    #: Shown in the inbox whatever you choose (it needs an answer).
    in_app_always: bool


class EmailStatus(Camel):
    address: str | None
    verified: bool
    #: Bounced, complained or unsubscribed from everything: no email is sent.
    suppressed: bool


class PreferencesView(Camel):
    kinds: list[KindPreference]
    email: EmailStatus


class PreferenceChange(Camel):
    kind: str
    #: InApp or Email
    channel: str
    enabled: bool


class PreferencesInput(Camel):
    preferences: list[PreferenceChange] = Field(max_length=64)


async def preferences_view(session: AsyncSession, user_ref: str) -> PreferencesView:
    prefs = await preferences.for_user(session, user_ref)
    r = await recipients.get(session, user_ref)
    suppressed = r is not None and await recipients.is_suppressed(session, r.Email)
    return PreferencesView(
        kinds=[
            KindPreference(
                kind=k.code,
                label=k.label,
                description=k.description,
                in_app=kinds.enabled(prefs, k.code, kinds.IN_APP),
                email=kinds.enabled(prefs, k.code, kinds.EMAIL),
                in_app_always=k.code in kinds.ALWAYS_IN_APP,
            )
            for k in kinds.KINDS.values()
        ],
        email=EmailStatus(address=r.Email if r else None, verified=bool(r and r.EmailVerified), suppressed=suppressed),
    )


async def get_preferences(sessions: async_sessionmaker[AsyncSession], user: Principal) -> PreferencesView:
    async with sessions() as s:
        return await preferences_view(s, user.user_ref)


async def put_preferences(
    sessions: async_sessionmaker[AsyncSession], user: Principal, body: PreferencesInput
) -> PreferencesView:
    fields = []
    for i, c in enumerate(body.preferences):
        if not kinds.is_known(c.kind):
            fields.append(
                {"field": f"preferences[{i}].kind", "code": "unknown", "message": "Unknown notification kind."}
            )
        if c.channel not in kinds.CHANNELS:
            fields.append({"field": f"preferences[{i}].channel", "code": "invalid", "message": "Use InApp or Email."})
    if fields:
        raise ValidationFailed(details={"fields": fields})
    async with sessions() as s, s.begin():
        for c in body.preferences:
            await preferences.set_pref(s, user.user_ref, c.kind, c.channel, c.enabled)
        return await preferences_view(s, user.user_ref)


class InboxItem(Camel):
    id: str
    kind: str
    title: str
    body: str
    link: str | None
    payload: dict[str, Any]
    created_at: str
    read_at: str | None


class InboxView(Camel):
    items: list[InboxItem]
    unread: int
    #: Pass as `cursor` for older items; null at the end.
    next_cursor: str | None


async def inbox(
    sessions: async_sessionmaker[AsyncSession], user: Principal, unread_only: bool, limit: int, cursor: str | None
) -> InboxView:
    if not 1 <= limit <= 100:
        raise ValidationFailed.field("limit", "range", "Between 1 and 100.")
    async with sessions() as s:
        before = None
        if cursor:
            row = await notifications.by_public(s, user.user_ref, cursor)
            if row is None:
                raise ValidationFailed.field("cursor", "invalid", "Unknown cursor.")
            before = row.Id
        rows = await notifications.inbox(s, user.user_ref, unread_only, before, limit + 1)
        r = await recipients.get(s, user.user_ref)
        tz = r.TimeZone if r else "Europe/Amsterdam"
        unread = await notifications.unread_count(s, user.user_ref)
    items = [InboxItem.model_validate(inbox_item(x, tz)) for x in rows[:limit]]
    return InboxView(items=items, unread=unread, next_cursor=rows[limit - 1].PublicId if len(rows) > limit else None)


async def mark_read(sessions: async_sessionmaker[AsyncSession], user: Principal, notification_ref: str | None) -> int:
    async with sessions() as s, s.begin():
        nid = None
        if notification_ref is not None:
            row = await notifications.by_public(s, user.user_ref, notification_ref)
            if row is None:
                raise NotFound()
            nid = row.Id
        await notifications.mark_read(s, user.user_ref, nid, utcnow())
        return await notifications.unread_count(s, user.user_ref)


async def unsubscribe_from(sessions: async_sessionmaker[AsyncSession], token: str, kind: str) -> str:
    """One-click unsubscribe (spec 8): turns off email for that kind, or every email for `all`.
    Returns the label of what was turned off."""
    async with sessions() as s, s.begin():
        r = await recipients.by_unsubscribe_hash(s, unsubscribe.token_hash(token))
        if r is None:
            raise NotFound()
        if kind == "all":
            await recipients.suppress(s, r.Email, "Unsubscribe")
            return "all Crux emails"
        if not kinds.is_known(kind):
            raise NotFound()
        await preferences.set_pref(s, r.UserRef, kind, kinds.EMAIL, False)
        return f'"{kinds.KINDS[kind].label}" emails'


class BounceEvent(Camel):
    #: bounce or complaint
    type: str
    email: str


class BounceBatch(Camel):
    events: list[BounceEvent] = Field(max_length=500)


def postmark_event(body: dict[str, Any]) -> BounceEvent | None:
    """A Postmark bounce or spam-complaint webhook (one event per request) as a suppression event.
    Only what Postmark itself deactivated suppresses: hard bounces (`Inactive`) and complaints;
    soft and transient bounces are retried by Postmark and ignored here (B6-8)."""
    email = str(body.get("Email") or "")
    if body.get("RecordType") == "SpamComplaint":
        return BounceEvent(type="complaint", email=email)
    if body.get("RecordType") == "Bounce" and body.get("Inactive") is True:
        return BounceEvent(type="bounce", email=email)
    return None


def webhook_authorized(secret: str, header: str, authorization: str) -> bool:
    """The shared secret in `X-Crux-Webhook-Secret`, or as the password of HTTP Basic auth (what
    Postmark offers, as it doesn't sign webhooks). Never open while no secret is set."""
    if not secret:
        return False
    if header and hmac.compare_digest(secret, header):
        return True
    scheme, _, value = authorization.partition(" ")
    if scheme.lower() != "basic":
        return False
    try:
        _, _, password = base64.b64decode(value, validate=True).decode().partition(":")
    except (ValueError, UnicodeDecodeError):
        return False
    return bool(password) and hmac.compare_digest(secret, password)


async def record_bounces(sessions: async_sessionmaker[AsyncSession], batch: BounceBatch) -> int:
    n = 0
    async with sessions() as s, s.begin():
        for e in batch.events:
            reason = {"bounce": "Bounce", "complaint": "Complaint"}.get(e.type.lower())
            if reason and "@" in e.email:
                await recipients.suppress(s, e.email, reason)
                n += 1
    return n
