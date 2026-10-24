"""Side-panel chat (spec 7.4, 7.5): conversations scoped to a material, lecture, question, mock or
course; replies streamed as SSE `delta`, `citation`, `done` (or `error`)."""

import json
from collections.abc import AsyncIterator
from typing import Any, cast

from pydantic import Field
from sqlalchemy import select, update
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.engine import CursorResult

from app.clients.llm import LlmError, Usage
from app.domain import prompt
from app.repo.tables import citation, conversation, message, run
from app.service import runtime, tutors
from app.service.account import Camel, visible_tutor
from app.service.deps import Deps
from crux_common.auth import Principal
from crux_common.db import transaction
from crux_common.errors import AppError, NotFound, RateLimited, ValidationFailed
from crux_common.ids import new_id
from crux_common.sse import event
from crux_common.timeutil import iso, utcnow

SCOPES = {
    "Material": "material_id",
    "Component": "component_id",
    "Question": "question_id",
    "Mock": "mock_id",
    "Course": None,
}
MAX_OUTPUT_TOKENS = 2000


class ConversationInput(Camel):
    course_id: str
    #: Material, Component, Question, Mock, Course
    scope: str
    material_id: str | None = None
    component_id: str | None = None
    question_id: str | None = None
    mock_id: str | None = None
    tutor_id: str | None = None
    title: str | None = Field(default=None, max_length=255)


class CitationView(Camel):
    file_id: str
    page: int | None
    chunk_no: int
    quote: str | None


class MessageView(Camel):
    id: int
    role: str
    content: str
    anchor: dict[str, Any] | None
    feedback: str | None
    created_at: str
    citations: list[CitationView]


class ConversationView(Camel):
    id: str
    course_id: str
    scope: str
    material_id: str | None
    component_id: str | None
    question_id: str | None
    mock_id: str | None
    title: str | None
    created_at: str
    messages: list[MessageView] | None = None


def _view(c: Any, messages: list[MessageView] | None = None) -> ConversationView:
    return ConversationView(
        id=c.PublicId,
        course_id=c.CourseRef,
        scope=c.Scope,
        material_id=c.MaterialRef,
        component_id=c.ComponentRef,
        question_id=c.QuestionRef,
        mock_id=c.MockRef,
        title=c.Title,
        created_at=iso(c.CreatedAt) if c.CreatedAt else "",
        messages=messages,
    )


async def _readable(deps: Deps, user: Principal, course_ref: str) -> None:
    a = await deps.core.course_access(user.user_ref, course_ref)
    if a is None or a.role is None:
        raise NotFound()


async def create_conversation(deps: Deps, user: Principal, inp: ConversationInput) -> ConversationView:
    if inp.scope not in SCOPES:
        raise ValidationFailed.field("scope", "invalid", "Use Material, Component, Question, Mock or Course.")
    need = SCOPES[inp.scope]
    if need and not getattr(inp, need):
        raise ValidationFailed.field(
            need.replace("_id", "Id"), "required", f"A {inp.scope} conversation names its {inp.scope.lower()}."
        )
    await _readable(deps, user, inp.course_id)
    if inp.material_id:
        rows = await deps.core.materials(user.user_ref, [inp.material_id])
        if not rows or rows[0]["courseRef"] != inp.course_id:
            raise NotFound()
    async with transaction(deps.sessions) as s:
        tutor_id = None
        if inp.tutor_id:
            t = await visible_tutor(s, user.user_ref, inp.tutor_id)
            if t is None:
                raise ValidationFailed.field("tutorId", "unknown", "Pick a tutor.")
            tutor_id = t.Id
        public_id = new_id()
        await s.execute(
            insert(conversation).values(
                PublicId=public_id,
                UserRef=user.user_ref,
                CourseRef=inp.course_id,
                Scope=inp.scope,
                MaterialRef=inp.material_id,
                ComponentRef=inp.component_id,
                QuestionRef=inp.question_id,
                MockRef=inp.mock_id,
                TutorTemplateId=tutor_id,
                Title=inp.title,
                CreatedAt=utcnow(),
            )
        )
        row = await _conv(s, user, public_id)
        return _view(row, [])


async def _conv(s: Any, user: Principal, conv_ref: str) -> Any:
    row = (
        await s.execute(
            select(conversation).where(conversation.c.PublicId == conv_ref, conversation.c.UserRef == user.user_ref)
        )
    ).first()
    if row is None:
        raise NotFound()
    return row


async def list_conversations(
    deps: Deps, user: Principal, course_ref: str | None, scope: str | None, material_ref: str | None
) -> list[ConversationView]:
    stmt = select(conversation).where(conversation.c.UserRef == user.user_ref, conversation.c.ArchivedAt.is_(None))
    if course_ref:
        stmt = stmt.where(conversation.c.CourseRef == course_ref)
    if scope:
        stmt = stmt.where(conversation.c.Scope == scope)
    if material_ref:
        stmt = stmt.where(conversation.c.MaterialRef == material_ref)
    async with deps.sessions() as s:
        rows = await s.execute(stmt.order_by(conversation.c.Id.desc()).limit(100))
        return [_view(r) for r in rows]


async def _messages(s: Any, conv_id: int) -> list[MessageView]:
    msgs = (await s.execute(select(message).where(message.c.ConversationId == conv_id).order_by(message.c.Id))).all()
    ids = [m.Id for m in msgs]
    cites: dict[int, list[CitationView]] = {}
    if ids:
        for c in await s.execute(select(citation).where(citation.c.MessageId.in_(ids))):
            cites.setdefault(c.MessageId, []).append(
                CitationView(file_id=c.FileRef, page=c.PageNo, chunk_no=c.ChunkNo, quote=c.Quote)
            )
    return [
        MessageView(
            id=m.Id,
            role=m.Role,
            content=m.Content,
            anchor=m.Anchor,
            feedback=m.Feedback,
            created_at=iso(m.CreatedAt) if m.CreatedAt else "",
            citations=cites.get(m.Id, []),
        )
        for m in msgs
    ]


async def get_conversation(deps: Deps, user: Principal, conv_ref: str) -> ConversationView:
    async with deps.sessions() as s:
        row = await _conv(s, user, conv_ref)
        return _view(row, await _messages(s, row.Id))


class FeedbackInput(Camel):
    #: Up, Down, or null to clear
    feedback: str | None


async def feedback(deps: Deps, user: Principal, message_id: int, inp: FeedbackInput) -> None:
    if inp.feedback not in ("Up", "Down", None):
        raise ValidationFailed.field("feedback", "invalid", "Use Up, Down or null.")
    async with transaction(deps.sessions) as s:
        row = (
            await s.execute(
                select(message.c.Id)
                .join(conversation, conversation.c.Id == message.c.ConversationId)
                .where(
                    message.c.Id == message_id, conversation.c.UserRef == user.user_ref, message.c.Role == "Assistant"
                )
            )
        ).first()
        if row is None:
            raise NotFound()
        await s.execute(update(message).where(message.c.Id == message_id).values(Feedback=inp.feedback))


# ---------- sending ----------


class SendInput(Camel):
    content: str = Field(min_length=1, max_length=8000)
    #: {"fileRef", "page", "quote"}: the selected text in the viewer.
    anchor: dict[str, Any] | None = None


async def _retrieve(deps: Deps, ctx: dict[str, Any], conv: Any, inp: SendInput) -> list[prompt.Chunk]:
    mats = [m for m in ctx.get("materials", []) if m.get("fileRef")]
    titles = {m["fileRef"]: m["title"] for m in mats}
    if conv.MaterialRef:
        files = [m["fileRef"] for m in mats if m["materialId"] == conv.MaterialRef] or [m["fileRef"] for m in mats]
    elif conv.ComponentRef:
        files = [m["fileRef"] for m in mats if m["componentId"] == conv.ComponentRef] or [m["fileRef"] for m in mats]
    else:
        files = [m["fileRef"] for m in mats]
    chunks: list[prompt.Chunk] = []
    anchor = inp.anchor or {}
    if anchor.get("fileRef") in titles and isinstance(anchor.get("page"), int):
        for c in await deps.files.chunks(anchor["fileRef"], [anchor["page"]]):
            chunks.append(
                prompt.Chunk(c["fileRef"], c["chunkNo"], c.get("pageNo"), c["text"], titles.get(c["fileRef"], ""))
            )
    query = " ".join(x for x in (inp.content, str(anchor.get("quote") or "")) if x)
    for c in await deps.files.search(files, query, prompt.TOP_K):
        if not any(x.file_ref == c["fileRef"] and x.chunk_no == c["chunkNo"] for x in chunks):
            chunks.append(
                prompt.Chunk(c["fileRef"], c["chunkNo"], c.get("pageNo"), c["text"], titles.get(c["fileRef"], ""))
            )
    return chunks[: prompt.TOP_K + 1]


#: Spec 12: AI messages, 30 per minute per user.
MESSAGES_PER_MINUTE = 30


async def _rate_limit(deps: Deps, user_ref: str) -> None:
    if deps.redis is None:
        return
    key = f"crux:rl:ai-messages:{user_ref}"
    n = await deps.redis.incr(key)
    if n == 1:
        await deps.redis.expire(key, 60)
    if n > MESSAGES_PER_MINUTE:
        raise RateLimited(details={"retryAfterSecs": 60})


async def prepare(deps: Deps, user: Principal, conv_ref: str, inp: SendInput) -> dict[str, Any]:
    """Everything that can fail with a plain HTTP error happens before the stream starts."""
    await _rate_limit(deps, user.user_ref)
    async with deps.sessions() as s:
        conv = await _conv(s, user, conv_ref)
        history_rows = (
            await s.execute(
                select(message.c.Role, message.c.Content)
                .where(message.c.ConversationId == conv.Id)
                .order_by(message.c.Id.desc())
                .limit(prompt.MAX_HISTORY)
            )
        ).all()
    materials = [conv.MaterialRef] if conv.MaterialRef else []
    g = await runtime.check(deps, user.user_ref, "MaterialChat", conv.CourseRef, materials)
    st = g.settings
    scopes = [
        s
        for s, on in (("progress", st.ShareProgress), ("mistakes", st.ShareMistakes), ("answers", st.ShareAnswers))
        if on
    ]
    anchor = {
        "materialId": conv.MaterialRef,
        "componentId": conv.ComponentRef,
        "questionId": conv.QuestionRef,
        "mockId": conv.MockRef,
    }
    if inp.anchor and isinstance(inp.anchor.get("order"), int):
        anchor["order"] = inp.anchor["order"]
    ctx = await deps.core.context(
        user.user_ref, conv.CourseRef, scopes, {k: v for k, v in anchor.items() if v is not None}
    )
    async with deps.sessions() as s:
        t = await tutors.resolve(s, user.user_ref, conv.CourseRef, conv.TutorTemplateId)
    chunks = await _retrieve(deps, ctx, conv, inp)
    rules = dict(t.Rules or {})
    approach = bool(rules.get("approachCheckYesNoHint")) and prompt.is_approach_check(inp.content)
    user_data: dict[str, Any] = {k: ctx[k] for k in ("progress", "mistakes", "answers") if k in ctx}
    parts = prompt.PromptParts(
        tutor_text=t.PromptText,
        tutor_rules=rules,
        course=ctx.get("course", {}),
        anchor=inp.anchor,
        chunks=chunks,
        user_data=user_data,
        history=[
            {"role": "assistant" if r.Role == "Assistant" else "user", "content": r.Content}
            for r in reversed(history_rows)
        ],
        question=inp.content,
        approach_check=approach,
    )
    system, messages = prompt.build(parts)
    run_id = await runtime.start_run(
        deps,
        g,
        prompt_version=prompt.PROMPT_VERSION,
        estimate_input_tokens=sum(len(m["content"]) for m in messages) // 4 + len(system) // 4,
        max_output_tokens=MAX_OUTPUT_TOKENS,
        tutor_id=t.Id,
    )
    context_log = [("files.FileChunk", f"{c.file_ref}:{c.chunk_no}") for c in chunks]
    for key, etype in (("weakSpots", "core.WeakSpot"), ("slips", "core.Slip"), ("traps", "core.Trap")):
        context_log += [(etype, x["id"]) for x in user_data.get("mistakes", {}).get(key, []) if x.get("id")]
    if conv.QuestionRef and "answers" in user_data:
        context_log.append(("core.Question", conv.QuestionRef))
    return {
        "conv": conv,
        "gate": g,
        "run_id": run_id,
        "system": system,
        "messages": messages,
        "chunks": chunks,
        "approach": approach,
        "max_words": rules.get("maxWords"),
        "context_log": context_log,
        "tutor": t,
    }


async def _complete(deps: Deps, g: runtime.Gate, system: str, messages: list[dict[str, str]]) -> tuple[str, Usage]:
    provider, key = runtime.provider(deps, g)
    text, used = "", Usage()
    async for chunk in provider.stream_chat(key, g.model.id, system, messages, MAX_OUTPUT_TOKENS):
        if isinstance(chunk, Usage):
            used = chunk
        else:
            text += chunk
    return text, used


async def stream_reply(deps: Deps, user: Principal, inp: SendInput, prep: dict[str, Any]) -> AsyncIterator[str]:
    conv, g, run_id = prep["conv"], prep["gate"], prep["run_id"]
    total = Usage()
    reply = ""
    try:
        async with transaction(deps.sessions) as s:
            await s.execute(
                insert(message).values(
                    ConversationId=conv.Id, Role="User", Content=inp.content, Anchor=inp.anchor, CreatedAt=utcnow()
                )
            )
        provider, key = runtime.provider(deps, g)
        if prep["approach"]:
            # Spec 7.3: yes/no + one hint. Judge the reply; regenerate once if it solves the problem.
            reply, used = await _complete(deps, g, prep["system"], prep["messages"])
            total = Usage(total.input_tokens + used.input_tokens, total.output_tokens + used.output_tokens)
            verdict, used = await provider.complete_json(
                key,
                g.model.id,
                prompt.JUDGE_SYSTEM,
                [{"role": "user", "content": f"Student: {inp.content}\n\nTutor reply: {reply}"}],
                prompt.JUDGE_SCHEMA,
                200,
            )
            total = Usage(total.input_tokens + used.input_tokens, total.output_tokens + used.output_tokens)
            if verdict.get("fullSolution"):
                stricter = (
                    prep["system"]
                    + "\n\nYour previous draft solved the problem. Do NOT solve it. "
                    + "Yes or No, one hint, then a question."
                )
                reply, used = await _complete(deps, g, stricter, prep["messages"])
                total = Usage(total.input_tokens + used.input_tokens, total.output_tokens + used.output_tokens)
            reply = prompt.trim_words(reply, prep["max_words"])
            for i in range(0, len(reply), 24):
                yield event("delta", {"text": reply[i : i + 24]})
        else:
            sent_words = 0
            stopped = False
            async for chunk in provider.stream_chat(
                key, g.model.id, prep["system"], prep["messages"], MAX_OUTPUT_TOKENS
            ):
                if isinstance(chunk, Usage):
                    total = chunk
                    continue
                reply += chunk
                if stopped:
                    continue
                cap = prep["max_words"]
                if cap and sent_words + len(chunk.split()) > cap:
                    stopped = True  # the trimmed text is what's stored; the rest isn't shown
                    continue
                sent_words += len(chunk.split())
                yield event("delta", {"text": chunk})
            reply = prompt.trim_words(reply, prep["max_words"])
        transcript_ref = await _store_transcript(deps, user, run_id, prep, reply)
        async with transaction(deps.sessions) as s:
            result = await s.execute(
                insert(message).values(
                    ConversationId=conv.Id, Role="Assistant", Content=reply, AiRunId=run_id, CreatedAt=utcnow()
                )
            )
            msg_id = int(cast(CursorResult[Any], result).inserted_primary_key[0])  # type: ignore[index]
            cites: list[tuple[str, int, int | None, str]] = []
            for page in prompt.cited_pages(reply):
                c = next((x for x in prep["chunks"] if x.page_no == page), None)
                if c is not None and not any(x[0] == c.file_ref and x[1] == c.chunk_no for x in cites):
                    cites.append((c.file_ref, c.chunk_no, c.page_no, c.text[:200]))
            for file_ref, chunk_no, page_no, quote in cites:
                await s.execute(
                    insert(citation).values(
                        MessageId=msg_id, FileRef=file_ref, ChunkNo=chunk_no, PageNo=page_no, Quote=quote
                    )
                )
            await runtime.log_context(s, run_id, prep["context_log"])
        for file_ref, chunk_no, page_no, quote in cites:
            yield event("citation", {"fileId": file_ref, "page": page_no, "chunkNo": chunk_no, "quote": quote})
        cost = await runtime.finish_run(
            deps, run_id, total, transcript_ref=transcript_ref, retain_days=deps.settings.transcript_days
        )
        yield event("done", {"messageId": msg_id, "costMicroUsd": cost})
    except LlmError as e:
        await runtime.finish_run(deps, run_id, total, error=str(e))
        yield event("error", {"code": "provider_auth" if e.auth else "provider_error", "message": str(e)})
    except AppError as e:
        await runtime.finish_run(deps, run_id, total, error=e.message)
        yield event("error", {"code": e.code, "message": e.message})


async def _store_transcript(deps: Deps, user: Principal, run_id: int, prep: dict[str, Any], reply: str) -> str | None:
    """Spec 7.4: the exact prompt and reply, 90 days. Never contains the key."""
    if deps.store is None:
        return None
    key = f"transcripts/{user.user_ref}/{run_id}-{new_id()}.json"
    body = {
        "runId": run_id,
        "system": prep["system"],
        "messages": prep["messages"],
        "reply": reply,
        "model": prep["gate"].model.id,
    }
    await deps.store.put(key, json.dumps(body, ensure_ascii=False).encode())
    return key


async def purge_transcripts(deps: Deps) -> int:
    """Job `transcripts.purge` (daily): past RetainUntil, the object goes and the ref is cleared."""
    n = 0
    async with transaction(deps.sessions) as s:
        rows = (
            await s.execute(
                select(run.c.Id, run.c.TranscriptRef)
                .where(run.c.TranscriptRef.is_not(None), run.c.RetainUntil < utcnow())
                .limit(500)
            )
        ).all()
        for r in rows:
            if deps.store is not None:
                await deps.store.delete(r.TranscriptRef)
            await s.execute(update(run).where(run.c.Id == r.Id).values(TranscriptRef=None, RetainUntil=None))
            n += 1
    return n
