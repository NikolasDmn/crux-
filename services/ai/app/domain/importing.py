"""Pure helpers for the course import (spec 7.5): split the files' text into model-sized parts,
and match proposals against what the course already has (`ImportProposal.DuplicateOfRef`)."""

import re
from typing import Any

from app.domain.generation import similarity

#: Near-duplicate threshold, the same as generation (spec 7.5: trigram similarity > 0.9).
SIMILAR = 0.9


def split_parts(files: list[tuple[str, list[dict[str, Any]]]], budget: int, max_parts: int) -> tuple[list[str], bool]:
    """Pages packed in file order into parts of at most `budget` characters, each page inside its
    `<file ref>` block (a file can span parts). Returns the parts and whether text was left out
    because `max_parts` ran out. A single page longer than the budget is cut to fit."""
    parts: list[list[tuple[str, str]]] = [[]]
    used = 0
    truncated = False
    for ref, chunks in files:
        for c in chunks:
            piece = f"[page {c.get('pageNo')}]\n{c['text']}"[:budget]
            if used + len(piece) > budget and parts[-1]:
                if len(parts) == max_parts:
                    truncated = True
                    break
                parts.append([])
                used = 0
            parts[-1].append((ref, piece))
            used += len(piece)
        if truncated:
            break
    out = []
    for part in parts:
        blocks: list[str] = []
        current: str | None = None
        for ref, piece in part:
            if ref != current:
                if current is not None:
                    blocks.append("</file>")
                blocks.append(f'<file ref="{ref}">')
                current = ref
            blocks.append(piece.replace("</", "<\\/"))
        if current is not None:
            blocks.append("</file>")
        out.append("\n".join(blocks))
    return [p for p in out if p], truncated


def _key(text: Any) -> str:
    return re.sub(r"[^0-9a-z]+", "", str(text or "").lower())


def duplicate_of(kind: str, payload: dict[str, Any], catalog: dict[str, Any]) -> str | None:
    """The PublicId of an existing row this proposal matches, if any:
    lectures by code (or a near-identical name), concepts by name, papers by key, questions by a
    near-identical prompt, flashcards by a near-identical front."""
    if kind == "Component":
        code, name = _key(payload.get("code")), str(payload.get("name") or "")
        for k in catalog.get("components") or []:
            if (code and _key(k.get("code")) == code) or (name and similarity(name, k.get("name") or "") > SIMILAR):
                return str(k["id"])
    elif kind == "Concept":
        name = _key(payload.get("name"))
        for c in catalog.get("concepts") or []:
            if name and _key(c.get("name")) == name:
                return str(c["id"])
    elif kind == "Paper":
        key = _key(payload.get("paperKey"))
        for p in catalog.get("papers") or []:
            if key and _key(p.get("paperKey")) == key:
                return str(p["id"])
    elif kind == "Question":
        prompt = str((payload.get("content") or {}).get("prompt") or "")
        for q in catalog.get("questions") or []:
            if prompt and similarity(prompt, q.get("prompt") or "") > SIMILAR:
                return str(q["id"])
    elif kind == "Flashcard":
        front = str(payload.get("front") or "")
        for f in catalog.get("flashcards") or []:
            if front and similarity(front, f.get("front") or "") > SIMILAR:
                return str(f["id"])
    return None


def same_as(kind: str, payload: dict[str, Any], earlier: dict[str, Any]) -> bool:
    """Two proposals of one import that say the same thing (the model repeating itself, often
    across parts): the later one is dropped."""
    if kind == "Component":
        return bool(_key(payload.get("code"))) and _key(payload.get("code")) == _key(earlier.get("code"))
    if kind in ("Concept",):
        return bool(_key(payload.get("name"))) and _key(payload.get("name")) == _key(earlier.get("name"))
    if kind == "Paper":
        return bool(_key(payload.get("paperKey"))) and _key(payload.get("paperKey")) == _key(earlier.get("paperKey"))
    if kind == "Question":
        a = str((payload.get("content") or {}).get("prompt") or "")
        b = str((earlier.get("content") or {}).get("prompt") or "")
        return bool(a) and similarity(a, b) > SIMILAR
    if kind == "Flashcard":
        return (
            bool(payload.get("front")) and similarity(str(payload["front"]), str(earlier.get("front") or "")) > SIMILAR
        )
    return False
