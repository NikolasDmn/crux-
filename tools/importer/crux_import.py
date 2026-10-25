#!/usr/bin/env python3
"""Import a course export from the old exam board into Crux, through Crux's own API.

Internal tool; Python 3.10+, standard library only. It signs in as a normal user (the course's
owner-to-be), so everything it creates is theirs and goes through the same checks as the app.

    python tools/importer/crux_import.py imports/bsa-export --base http://localhost:8100 --login <user>
    python tools/importer/crux_import.py imports/bsa-export --dry-run        # just show the plan

The password is asked for (or read from CRUX_IMPORT_PASSWORD). Re-runnable: what it created is
recorded in <export>/.crux-import.json per server, so a second run only adds what's missing.

What it brings in: the course and its lectures (named from the export's README), concepts (from
the question tags), PDFs (slides, manual, textbook chapters by the lecture mapping, past exams;
each uploaded, virus-scanned and attached), the study-aid notes, past papers (with their
omissions and source PDF), the question bank (keys, explanations, sources, paper numbers,
figures) and the flashcards. Left out: the Obsidian vault and the exercise notebooks/data.
"""

from __future__ import annotations

import argparse
import getpass
import hashlib
import json
import os
import re
import struct
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import zlib
from pathlib import Path
from typing import Any

# ---------- the export ----------

KEY_RELIABILITY = {"official": "Official", "derived": "Derived", "disputed": "Disputed"}
QUESTION_SOURCE = {"exam": "PastExam", "practice": "PastExam", "manual": "Lecture", "generated": "Imported"}
SKIP_TEXTBOOK = ("acknowledgments", "preface", "index")


def read_json(p: Path) -> Any:
    return json.loads(p.read_text(encoding="utf-8"))


def course_identity(readme: str) -> tuple[str, str]:
    """`# BSA (Biomedical Signal Analysis) export` -> ("BSA", "Biomedical Signal Analysis")."""
    m = re.search(r"^#\s*([A-Za-z0-9]+)\s*\(([^)]+)\)", readme, re.M)
    if not m:
        raise SystemExit("The README has no '# CODE (Course name) export' title; pass --code and --name.")
    return m.group(1).upper(), m.group(2).strip()


def lecture_names(readme: str) -> dict[str, str]:
    """`Lectures: L0 = course intro, L1 empirical modelling, ...` -> {"L0": "Course intro", ...}."""
    m = re.search(r"^Lectures:\s*(.+)$", readme, re.M)
    if not m:
        raise SystemExit("The README has no 'Lectures: L0 = ..., L1 ...' line.")
    out: dict[str, str] = {}
    for part in re.split(r",\s*(?=L\d)", m.group(1).rstrip(". ")):
        pm = re.match(r"(L\d+)\s*=?\s*(.+)", part.strip())
        if pm:
            name = pm.group(2).strip().rstrip(".")
            out[pm.group(1)] = name[:1].upper() + name[1:]
    return out


def chapter_lectures(mapping_md: str) -> dict[int, str]:
    """Textbook chapter -> lecture, from the course's mapping page ('## Lecture N' sections that
    mention 'Ch. N'). The first lecture that names a chapter gets it."""
    out: dict[int, str] = {}
    lecture = None
    for line in mapping_md.splitlines():
        h = re.match(r"^##\s+Lecture\s+(\d+)", line)
        if h:
            lecture = f"L{h.group(1)}"
            continue
        if line.startswith("## "):
            lecture = None
        if lecture and "Shiavi" in line or (lecture and re.search(r"\bCh\.\s*\d", line) and "Rangayyan" not in line):
            for ch in re.findall(r"Ch\.\s*(\d+)", line) + re.findall(r"and Ch\.\s*(\d+)", line):
                out.setdefault(int(ch), lecture)
            for ch in re.findall(r"§(\d+)\.", line):
                out.setdefault(int(ch), lecture)
    return out


def humanize(tag: str) -> str:
    """`polyfit-least-squares` -> `Polyfit least squares`."""
    words = tag.replace("_", "-").split("-")
    text = " ".join(w for w in words if w)
    return text[:1].upper() + text[1:]


def clean_title(stem: str) -> str:
    return re.sub(r"\s+", " ", re.sub(r"[-_]+", " ", stem)).strip()


def plan_materials(root: Path, lectures: dict[str, str]) -> list[dict[str, Any]]:
    """(path, lecture, kind, title) for every file brought in as a material, in a stable order."""
    mats = root / "01-materials"
    mapping = mats / "lecture-textbook-mapping.md"
    chapters = chapter_lectures(mapping.read_text(encoding="utf-8")) if mapping.exists() else {}
    out: list[dict[str, Any]] = []

    def add(path: Path, lecture: str, kind: str, title: str) -> None:
        if lecture not in lectures:
            lecture = "L0" if "L0" in lectures else sorted(lectures)[0]
        out.append({"path": path.relative_to(root).as_posix(), "lecture": lecture, "kind": kind, "title": title})

    for p in sorted((mats / "Slides").glob("*.pdf")):
        m = re.search(r"lecture\s*0?(\d+)", p.stem, re.I)
        n = m.group(1) if m else "0"
        add(p, f"L{n}", "Slides", f"Lecture {n} slides")
    for p in sorted(mats.glob("*.pdf")):
        add(p, "L0", "Other", "Course manual" if "manual" in p.stem.lower() else clean_title(p.stem))
    for book in sorted(d for d in (mats / "Textbooks").glob("*") if d.is_dir()):
        for p in sorted(book.glob("*.pdf"), key=lambda x: (int(m.group(1)) if (m := re.match(r"(\d+)---", x.name)) else 99, x.name)):
            if p.stem.lower().startswith(SKIP_TEXTBOOK):
                continue
            m = re.match(r"(\d+)---([^_]+)", p.stem)
            if m:
                ch = int(m.group(1))
                add(p, chapters.get(ch, "L0"), "Textbook", f"{book.name}: ch. {ch} {clean_title(m.group(2))}")
            else:
                add(p, "L0", "Textbook", f"{book.name}: {clean_title(p.stem.split('_')[0])}")
    labels = paper_labels(root)
    for p in sorted((mats / "Past Exams").rglob("*.pdf")):
        add(p, "L0", "PastExam", exam_title(p.name, labels))
    if mapping.exists():
        add(mapping, "L0", "Notes", "Lectures, slides and textbook chapters")
    for p in sorted((root / "02-study-aids").glob("*.md")):
        m = re.search(r"lecture\s*0?(\d+)", p.stem, re.I)
        title = f"Lecture {m.group(1)} {clean_title(p.stem[m.end():])}".strip() if m else clean_title(p.stem).capitalize()
        add(p, f"L{m.group(1)}" if m else "L0", "Notes", title)
    return out


def paper_labels(root: Path) -> dict[str, str]:
    """Past-exam file name -> its paper's label (papers.json names the copy it took questions from;
    the other copies share the date and test number)."""
    out: dict[str, str] = {}
    p = root / "03-question-bank" / "papers.json"
    for key, paper in (read_json(p).items() if p.exists() else []):
        name = Path(paper.get("file", "")).name
        out[name] = paper.get("label") or key
        m = re.match(r"(\d{4}-\d{2}-\d{2}_T\d)", name)
        if m:
            out.setdefault(m.group(1), paper.get("label") or key)
    return out


def exam_title(name: str, labels: dict[str, str]) -> str:
    """`2015-10-05_T2B_Q13-24_student-marked.pdf` -> `2015 test 2 (student-marked)`."""
    m = re.match(r"(\d{4}-\d{2}-\d{2}_T\d)", name)
    base = labels.get(name) or (labels.get(m.group(1)) if m else None)
    low = name.lower()
    tag = ("answers" if "answers" in low and "noanswers" not in low else "no answers" if "noanswers" in low
           else "student-marked" if "student-marked" in low else "answer key" if "answer-key" in low
           else "solutions" if "solutions" in low else "")
    if not base:
        base = clean_title(Path(name).stem)
        return base
    return f"{base} ({tag})" if tag else base


def skipped_files(root: Path) -> list[str]:
    mats = root / "01-materials" / "Exercises"
    return sorted(p.relative_to(root).as_posix() for p in mats.rglob("*") if p.is_file()) if mats.exists() else []


# ---------- a PNG figure as a one-page PDF (the files service takes PDFs; a figure is a crop) ----------


def png_to_pdf(png: bytes) -> tuple[bytes, int, int]:
    """A one-page PDF showing the PNG at 1 pt per pixel. 8-bit RGB or grey, not interlaced: the
    PNG's compressed data goes in as is (FlateDecode with the PNG predictor)."""
    if png[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos, idat, w = 8, b"", 0
    h = bits = color = interlace = 0
    while pos < len(png):
        (length,) = struct.unpack(">I", png[pos : pos + 4])
        kind = png[pos + 4 : pos + 8]
        data = png[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            w, h, bits, color, _, _, interlace = struct.unpack(">IIBBBBB", data)
        elif kind == b"IDAT":
            idat += data
        pos += 12 + length
    if bits != 8 or interlace or color not in (0, 2):
        raise ValueError(f"unsupported PNG (bits {bits}, colour type {color}, interlace {interlace})")
    colors = 3 if color == 2 else 1
    space = "/DeviceRGB" if colors == 3 else "/DeviceGray"
    content = f"q {w} 0 0 {h} 0 0 cm /Im0 Do Q".encode()
    objs = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>".encode(),
        (
            f"<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace {space} /BitsPerComponent 8 "
            f"/Filter /FlateDecode /DecodeParms << /Predictor 15 /Colors {colors} /BitsPerComponent 8 /Columns {w} >> "
            f"/Length {len(idat)} >>\nstream\n"
        ).encode()
        + idat
        + b"\nendstream",
        f"<< /Length {len(content)} >>\nstream\n".encode() + content + b"\nendstream",
    ]
    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for i, body in enumerate(objs, 1):
        offsets.append(len(out))
        out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objs) + 1}\n0000000000 65535 f \n".encode()
    out += b"".join(f"{o:010d} 00000 n \n".encode() for o in offsets)
    out += f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    return bytes(out), w, h


# ---------- the API ----------


class Api:
    def __init__(self, base: str, login: str, password: str) -> None:
        self.base = base.rstrip("/")
        self.login_name, self.password = login, password
        self.token = ""
        self.sign_in()

    def sign_in(self) -> None:
        r = self._raw("POST", "/api/v1/auth/login", {"login": self.login_name, "password": self.password}, auth=False)
        self.token = r["accessToken"]

    def _raw(self, method: str, path: str, body: Any = None, *, auth: bool = True, headers: dict[str, str] | None = None,
             data: bytes | None = None, url: str | None = None) -> Any:
        req = urllib.request.Request(url or self.base + path, method=method)
        if body is not None:
            data = json.dumps(body).encode()
            req.add_header("content-type", "application/json")
        if auth and not url:
            req.add_header("authorization", f"Bearer {self.token}")
        for k, v in (headers or {}).items():
            req.add_header(k, v)
        with urllib.request.urlopen(req, data=data, timeout=120) as r:
            raw = r.read()
            return json.loads(raw) if raw and r.headers.get("content-type", "").startswith("application/json") else None

    def call(self, method: str, path: str, body: Any = None, *, headers: dict[str, str] | None = None) -> Any:
        """JSON API call: signs in again on an expired token, waits out a rate limit."""
        h = dict(headers or {})
        if method == "POST" and not any(k.lower() == "idempotency-key" for k in h):
            h["Idempotency-Key"] = str(uuid.uuid4())
        for attempt in range(5):
            try:
                return self._raw(method, path, body, headers=h)
            except urllib.error.HTTPError as e:
                detail = e.read().decode(errors="replace")
                if e.code == 401 and attempt == 0:
                    self.sign_in()
                    continue
                if e.code == 429:
                    wait = int(e.headers.get("retry-after") or 5)
                    print(f"  rate limited, waiting {wait} s")
                    time.sleep(wait)
                    continue
                raise RuntimeError(f"{method} {path}: {e.code} {detail[:400]}") from None
        raise RuntimeError(f"{method} {path}: gave up after retries")

    def upload(self, course_id: str, name: str, data: bytes) -> str:
        """Upload a PDF (or reuse the stored copy), wait for the virus scan; returns the file id."""
        sha = hashlib.sha256(data).hexdigest()
        r = self.call("POST", "/api/v1/files/uploads",
                      {"courseId": course_id, "fileName": name, "sizeBytes": len(data), "sha256": sha, "purpose": "Original"})
        fid = r["file"]["id"]
        if not r.get("alreadyPresent") and r.get("uploadUrl"):
            # Signed for the server's public address; send it through this base URL's host.
            u = urllib.parse.urlsplit(r["uploadUrl"])
            self._raw("PUT", "", data=data, url=f"{self.base}{u.path}?{u.query}", headers=r.get("uploadHeaders") or {})
            self.call("POST", f"/api/v1/files/{fid}/complete")
        for _ in range(200):
            f = self.call("GET", f"/api/v1/files/{fid}")
            if f["scanStatus"] == "Clean":
                return fid
            if f["scanStatus"] in ("Infected", "Failed"):
                raise RuntimeError(f"{name}: scan {f['scanStatus']}")
            time.sleep(3)
        raise RuntimeError(f"{name}: the virus scan didn't finish in 10 minutes")


# ---------- state (what this server already has from this export) ----------


class State:
    def __init__(self, root: Path, base: str) -> None:
        self.path = root / ".crux-import.json"
        self.all = read_json(self.path) if self.path.exists() else {}
        self.s = self.all.setdefault(base.rstrip("/"), {})
        for k in ("lectures", "concepts", "files", "materials", "papers", "questions", "cards"):
            self.s.setdefault(k, {})

    def save(self) -> None:
        self.path.write_text(json.dumps(self.all, indent=1), encoding="utf-8")


# ---------- the import ----------


def run(args: argparse.Namespace) -> None:
    root = Path(args.export).resolve()
    readme = (root / "README.md").read_text(encoding="utf-8")
    code, name = course_identity(readme)
    code, name = (args.code or code).upper(), args.name or name
    lectures = lecture_names(readme)
    questions = read_json(root / "03-question-bank" / "questions.json")
    papers = read_json(root / "03-question-bank" / "papers.json")
    cards = read_json(root / "04-flashcards" / "flashcards.json")
    materials = plan_materials(root, lectures)
    tags = sorted({t for q in questions for t in q.get("concepts", [])})

    print(f"{code} {name}: {len(lectures)} lectures, {len(tags)} concepts, {len(materials)} materials, "
          f"{len(papers)} papers, {len(questions)} questions, {len(cards)} flashcards")
    if args.dry_run:
        for lec, n in lectures.items():
            print(f"  {lec} {n}")
        for m in materials:
            print(f"  [{m['lecture']}] {m['kind']:9} {m['title']}  <- {m['path']}")
        print(f"  left out: the Obsidian vault and {len(skipped_files(root))} exercise files (notebooks, .mat, .py)")
        return

    password = os.environ.get("CRUX_IMPORT_PASSWORD") or getpass.getpass(f"Password for {args.login}: ")
    api = Api(args.base, args.login, password)
    st = State(root, args.base)
    s = st.s

    # Course
    if not s.get("course"):
        mine = [c for c in api.call("GET", "/api/v1/courses") if c["code"] == code and not c.get("trashedAt")]
        if mine and not args.into_existing:
            raise SystemExit(f"You already have a course {code}. Pass --code to import under another code, "
                             f"or --into-existing to add to it.")
        s["course"] = mine[0]["id"] if mine else api.call("POST", "/api/v1/courses", {"name": name, "code": code})["id"]
        st.save()
    cid = s["course"]
    print(f"course {code}: {cid}")

    # Lectures
    existing = {c["code"]: c["id"] for c in api.call("GET", f"/api/v1/courses/{cid}/components")}
    for i, (lec, title) in enumerate(lectures.items()):
        if lec in s["lectures"]:
            continue
        s["lectures"][lec] = existing.get(lec) or api.call(
            "POST", f"/api/v1/courses/{cid}/components", {"code": lec, "name": title, "sortOrder": i})["id"]
        st.save()
    lec_id = s["lectures"]

    # Concepts
    have = {c["name"].lower(): c["id"] for c in api.call("GET", f"/api/v1/courses/{cid}/concepts")}
    for t in tags:
        if t in s["concepts"]:
            continue
        n = humanize(t)
        # Two tags can read the same once humanized (`model-order` / `model_order`): one concept.
        if n.lower() not in have:
            have[n.lower()] = api.call("POST", f"/api/v1/courses/{cid}/concepts", {"name": n})["id"]
        s["concepts"][t] = have[n.lower()]
        st.save()
    print(f"lectures {len(lec_id)}, concepts {len(s['concepts'])}")

    def file_id(path: Path) -> str:
        data = path.read_bytes()
        sha = hashlib.sha256(data).hexdigest()
        if sha not in s["files"]:
            s["files"][sha] = api.upload(cid, path.name, data)
            st.save()
        return s["files"][sha]

    # Materials
    done = 0
    for i, m in enumerate(materials):
        if m["path"] in s["materials"]:
            continue
        p = root / m["path"]
        body: dict[str, Any] = {"kind": m["kind"], "title": m["title"], "sortOrder": i}
        if p.suffix.lower() == ".pdf":
            print(f"  uploading {m['path']}")
            body["fileId"] = file_id(p)
        else:
            body["content"] = p.read_text(encoding="utf-8")
        s["materials"][m["path"]] = api.call("POST", f"/api/v1/components/{lec_id[m['lecture']]}/materials", body)["id"]
        st.save()
        done += 1
    print(f"materials: {done} added, {len(materials) - done} already there")

    # Past papers
    for key, p in papers.items():
        if key in s["papers"]:
            continue
        src = p.get("file", "")
        local = next((root / "01-materials" / src.split("/", 2)[2]).parent.glob(Path(src).name), None) if src.count("/") >= 2 else None
        status = p.get("key", "").lower()
        fields = {
            "paperKey": key,
            "label": p.get("label") or key,
            "examDate": p["date"] if re.fullmatch(r"\d{4}-\d{2}-\d{2}", p.get("date", "")) else None,
            "totalQuestions": p.get("total"),
            "keyStatus": "Official" if status.startswith("official") else "Derived" if status.startswith("derived") else "Partial",
            "note": "; ".join(x for x in (p.get("key"), p.get("range"), p.get("note")) if x) or None,
            "omissions": [{"questionNumber": str(o["q"]), "reason": o["why"]} for o in p.get("missing", [])],
            "fileId": file_id(local) if local and local.exists() else None,
        }
        s["papers"][key] = api.call("POST", f"/api/v1/courses/{cid}/papers", fields)["id"]
        st.save()
    print(f"papers: {len(s['papers'])}")

    # Questions
    added = failed = 0
    for q in questions:
        if q["id"] in s["questions"]:
            continue
        try:
            s["questions"][q["id"]] = add_question(api, root, cid, q, s, file_id)
            st.save()
            added += 1
        except Exception as e:  # keep going; the run can be repeated
            failed += 1
            print(f"  ! {q['id']}: {e}")
    print(f"questions: {added} added, {failed} failed, {len(s['questions'])} in total")

    # Flashcards
    added = 0
    for c in cards:
        key = c.get("id") or c["code"]
        if key in s["cards"]:
            continue
        lec = c.get("lecture") if c.get("lecture") in lec_id else "L0"
        s["cards"][key] = api.call("POST", f"/api/v1/courses/{cid}/flashcards",
                                   {"componentId": lec_id[lec], "front": c["front"], "back": c["back"],
                                    "shared": True, "source": "Lecture"})["id"]
        st.save()
        added += 1
    print(f"flashcards: {added} added, {len(s['cards'])} in total")
    print(f"Done. Left out: the Obsidian vault and {len(skipped_files(root))} exercise files.")


def add_question(api: Api, root: Path, cid: str, q: dict[str, Any], s: dict[str, Any], file_id: Any) -> str:
    lec = q["lecture"] if q["lecture"] in s["lectures"] else "L0"
    content: dict[str, Any] = {
        "prompt": q["stem"],
        "explanation": q.get("explanation") or None,
        "keyReliability": KEY_RELIABILITY.get(q.get("key", ""), "Derived"),
    }
    if q["type"] == "mc":
        texts = q.get("options") or [f"Option {chr(65 + i)} (in the figure)" for i in range(q.get("optionCount", 0))]
        content["options"] = [{"text": t, "isCorrect": i == q.get("answer")} for i, t in enumerate(texts)]
        content["shuffleOptions"] = not q.get("optionCount")
        qtype = "MC"
    else:
        content["open"] = {"modelAnswer": q.get("modelAnswer")}
        qtype = "Open"
    src = q.get("source", {})
    paper = s["papers"].get(src.get("paper", ""))
    number = re.search(r"-q(\d+)$", q["id"])
    concepts = [s["concepts"][t] for t in q.get("concepts", []) if t in s["concepts"]]
    body = {
        "componentId": s["lectures"][lec],
        "type": qtype,
        "content": content,
        "conceptIds": concepts,
        "primaryConceptId": concepts[0] if concepts else None,
        "source": QUESTION_SOURCE.get(src.get("kind", ""), "Imported"),
        "sourceLabel": (src.get("label") or "")[:255] or None,
        "paperId": paper,
        "paperQuestionNo": number.group(1) if paper and number else None,
        "familyOfId": s["questions"].get(q["family"]) if q.get("family") else None,
    }
    created = api.call("POST", f"/api/v1/courses/{cid}/questions", body)
    if q.get("figure"):
        png = (root / "03-question-bank" / q["figure"]).read_bytes()
        pdf, w, h = png_to_pdf(png)
        tmp = root / "03-question-bank" / f".{q['id']}.figure.pdf"
        tmp.write_bytes(pdf)
        try:
            fid = file_id(tmp)
        finally:
            tmp.unlink(missing_ok=True)
        api.call("POST", f"/api/v1/questions/{created['id']}/image-crop",
                 {"fileId": fid, "page": 1, "box": {"x": 0, "y": 0, "w": w, "h": h}})
    return created["id"]


def main() -> None:
    a = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    a.add_argument("export", help="the export folder (with README.md, 01-materials, 03-question-bank, 04-flashcards)")
    a.add_argument("--base", default="http://localhost:8100", help="Crux's address (default http://localhost:8100)")
    a.add_argument("--login", help="username or email to import as (the course's owner)")
    a.add_argument("--code", help="course code (default: from the README)")
    a.add_argument("--name", help="course name (default: from the README)")
    a.add_argument("--into-existing", action="store_true", help="add to your existing course with that code")
    a.add_argument("--dry-run", action="store_true", help="show what would be imported, change nothing")
    args = a.parse_args()
    if not args.dry_run and not args.login:
        a.error("--login is required (or use --dry-run)")
    try:
        run(args)
    except RuntimeError as e:
        sys.exit(f"error: {e}")


if __name__ == "__main__":
    main()
