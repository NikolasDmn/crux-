"""Public routes under /api/v1/ai (spec 15), and the internal gate."""

from fastapi import APIRouter, Request, Response
from fastapi.responses import StreamingResponse

from app.service import account, chat, grading, runtime, tutors
from app.service.deps import Deps
from crux_common.auth import Caller, User

router = APIRouter(tags=["ai"])
internal = APIRouter(prefix="/internal/v1", tags=["internal"])


def deps(request: Request) -> Deps:
    d: Deps = request.app.state.deps
    return d


# ---------- settings, keys, usage ----------


@router.get("/settings", response_model=account.SettingsView, response_model_by_alias=True)
async def get_settings(user: User, request: Request) -> account.SettingsView:
    return await account.get_settings(deps(request), user)


@router.patch("/settings", response_model=account.SettingsView, response_model_by_alias=True)
async def patch_settings(body: account.SettingsPatch, user: User, request: Request) -> account.SettingsView:
    """Turning AI on needs `consentPolicyVersion` (the version shown); every change is logged."""
    return await account.patch_settings(deps(request), user, body)


@router.get("/keys", response_model=list[account.KeyView], response_model_by_alias=True)
async def list_keys(user: User, request: Request) -> list[account.KeyView]:
    """Never returns key material: provider, label, last 4 characters, status."""
    return await account.list_keys(deps(request), user)


@router.post("/keys", response_model=account.KeyView, response_model_by_alias=True, status_code=201)
async def add_key(body: account.KeyInput, user: User, request: Request) -> account.KeyView:
    """Stored encrypted and checked with one cheap call (list models)."""
    return await account.add_key(deps(request), user, body)


@router.delete("/keys/{key_id}", status_code=204)
async def revoke_key(key_id: str, user: User, request: Request) -> Response:
    await account.revoke_key(deps(request), user, key_id)
    return Response(status_code=204)


@router.post("/keys/{key_id}/verify", response_model=account.KeyView, response_model_by_alias=True)
async def verify_key(key_id: str, user: User, request: Request) -> account.KeyView:
    return await account.verify_key(deps(request), user, key_id)


@router.get("/usage", response_model=list[account.MonthUsage], response_model_by_alias=True)
async def usage(user: User, request: Request, months: int = 6) -> list[account.MonthUsage]:
    """Estimated spend per month and feature (your provider bills you)."""
    return await account.usage(deps(request), user, max(1, min(months, 24)))


# ---------- tutors ----------


@router.get("/tutors", response_model=list[tutors.TutorView], response_model_by_alias=True)
async def list_tutors(user: User, request: Request) -> list[tutors.TutorView]:
    return await tutors.list_tutors(deps(request), user)


@router.post("/tutors", response_model=tutors.TutorView, response_model_by_alias=True, status_code=201)
async def create_tutor(body: tutors.TutorInput, user: User, request: Request) -> tutors.TutorView:
    return await tutors.create(deps(request), user, body)


@router.patch("/tutors/{tutor_id}", response_model=tutors.TutorView, response_model_by_alias=True)
async def edit_tutor(tutor_id: str, body: tutors.TutorInput, user: User, request: Request) -> tutors.TutorView:
    """Saves a new version; conversations pick it up on their next message."""
    return await tutors.edit(deps(request), user, tutor_id, body)


# ---------- per-course ----------


@router.get("/courses/{course_id}/settings", response_model=account.CourseSettingsView, response_model_by_alias=True)
async def get_course_settings(course_id: str, user: User, request: Request) -> account.CourseSettingsView:
    return await account.get_course_settings(deps(request), user, course_id)


@router.put("/courses/{course_id}/settings", response_model=account.CourseSettingsView, response_model_by_alias=True)
async def put_course_settings(
    course_id: str, body: account.CourseSettingsInput, user: User, request: Request
) -> account.CourseSettingsView:
    return await account.put_course_settings(deps(request), user, course_id, body)


# ---------- chat ----------


@router.get("/conversations", response_model=list[chat.ConversationView], response_model_by_alias=True)
async def list_conversations(
    user: User,
    request: Request,
    courseId: str | None = None,
    scope: str | None = None,
    materialId: str | None = None,
) -> list[chat.ConversationView]:
    return await chat.list_conversations(deps(request), user, courseId, scope, materialId)


@router.post("/conversations", response_model=chat.ConversationView, response_model_by_alias=True, status_code=201)
async def create_conversation(body: chat.ConversationInput, user: User, request: Request) -> chat.ConversationView:
    return await chat.create_conversation(deps(request), user, body)


@router.get("/conversations/{conversation_id}", response_model=chat.ConversationView, response_model_by_alias=True)
async def get_conversation(conversation_id: str, user: User, request: Request) -> chat.ConversationView:
    return await chat.get_conversation(deps(request), user, conversation_id)


@router.post("/conversations/{conversation_id}/messages")
async def send_message(conversation_id: str, body: chat.SendInput, user: User, request: Request) -> StreamingResponse:
    """SSE: `delta` {text}, `citation` {fileId, page, chunkNo, quote}, then `done` {messageId} or
    `error`. Gate, budget and access errors are plain HTTP errors before the stream starts."""
    d = deps(request)
    prep = await chat.prepare(d, user, conversation_id, body)
    return StreamingResponse(
        chat.stream_reply(d, user, body, prep), media_type="text/event-stream", headers={"cache-control": "no-cache"}
    )


@router.post("/messages/{message_id}/feedback", status_code=204)
async def message_feedback(message_id: int, body: chat.FeedbackInput, user: User, request: Request) -> Response:
    await chat.feedback(deps(request), user, message_id, body)
    return Response(status_code=204)


# ---------- grading ----------


@router.get("/grades", response_model=list[grading.GradeView], response_model_by_alias=True)
async def grades(mockId: str, user: User, request: Request) -> list[grading.GradeView]:
    """AI grading suggestions for the review screen; decide on them in core (ai-grade routes)."""
    return await grading.grades_for(deps(request), user, mockId)


# ---------- internal ----------


@internal.get("/ai-allowed")
async def ai_allowed(
    request: Request, caller: Caller, user: str, feature: str, course: str | None = None, materials: str = ""
) -> dict[str, object]:
    """Spec 7.1, for core (which proxies it for the frontend). Service token required."""
    if feature not in ("MaterialChat", "Grading", "CourseImport", "PastExamImport", "Generation"):
        return {"allowed": False, "reason": "feature_off"}
    refs = [m for m in materials.split(",") if m]
    reason, _ = await runtime.decide(deps(request), user, feature, course, refs)
    return {"allowed": reason is None, "reason": reason}


@internal.get("/grades", response_model=list[grading.GradeView], response_model_by_alias=True)
async def internal_grades(request: Request, caller: Caller, mock: str, user: str) -> list[grading.GradeView]:
    """For core, which decides on a suggestion and must see exactly what ai suggested."""
    return await grading.list_grades(deps(request), user, mock)
