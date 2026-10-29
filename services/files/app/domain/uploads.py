"""Upload rules (spec 6.1). Pure: no I/O."""

import re
from dataclasses import dataclass

#: Extension → MIME. The allowlist itself is config (Settings.allowed_original_types).
MIME_BY_EXTENSION = {".pdf": "application/pdf"}
#: Question images uploaded directly (crops of a PDF page are made server-side).
IMAGE_MIME_BY_EXTENSION = {".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".webp": "image/webp"}
MAX_IMAGE_BYTES = 5 * 1024 * 1024

_SHA256 = re.compile(r"^[0-9a-f]{64}$")


@dataclass(frozen=True)
class Rejection:
    reason: str  # 'type' | 'size' | 'checksum' (file_rejected details.reason; spec 3.1)
    message: str


def mime_for(file_name: str) -> str | None:
    name = file_name.lower().strip()
    for ext, mime in MIME_BY_EXTENSION.items():
        if name.endswith(ext):
            return mime
    return None


def check_start(file_name: str, size_bytes: int, sha256: str, allowed: set[str], max_bytes: int) -> Rejection | str:
    """Validates an upload request. Returns the MIME type, or why it's rejected."""
    mime = mime_for(file_name)
    if mime is None or mime not in allowed:
        return Rejection("type", "Only PDF files can be uploaded for now.")
    if size_bytes <= 0 or size_bytes > max_bytes:
        return Rejection("size", f"Files must be under {max_bytes // (1024 * 1024)} MB.")
    if not _SHA256.match(sha256.lower()):
        return Rejection("checksum", "sha256 must be 64 hex characters.")
    return mime


def check_image_start(file_name: str, size_bytes: int, sha256: str) -> Rejection | str:
    """A question image: PNG, JPEG or WebP up to 5 MB. Returns the MIME type, or why not."""
    name = file_name.lower().strip()
    mime = next((m for ext, m in IMAGE_MIME_BY_EXTENSION.items() if name.endswith(ext)), None)
    if mime is None:
        return Rejection("type", "Use a PNG, JPEG or WebP image.")
    if size_bytes <= 0 or size_bytes > MAX_IMAGE_BYTES:
        return Rejection("size", f"Images must be under {MAX_IMAGE_BYTES // (1024 * 1024)} MB.")
    if not _SHA256.match(sha256.lower()):
        return Rejection("checksum", "sha256 must be 64 hex characters.")
    return mime


def sniff_ok(mime: str, head: bytes) -> bool:
    """Magic bytes (spec 6.1): the content must be what the name says."""
    if mime == "application/pdf":
        return head.startswith(b"%PDF-")
    if mime == "image/png":
        return head.startswith(b"\x89PNG\r\n\x1a\n")
    if mime == "image/jpeg":
        return head.startswith(b"\xff\xd8\xff")
    if mime == "image/webp":
        return head[:4] == b"RIFF" and head[8:12] == b"WEBP"
    return False


def safe_file_name(name: str) -> str:
    """Keeps a display name sane: no path parts, no control characters, ≤ 255 chars."""
    base = name.replace("\\", "/").rsplit("/", 1)[-1]
    cleaned = "".join(c for c in base if c.isprintable()).strip() or "file.pdf"
    return cleaned[:255]


@dataclass(frozen=True)
class QuotaState:
    quota: int
    used: int
    reserved: int

    def can_reserve(self, size: int) -> bool:
        return self.used + self.reserved + size <= self.quota

    @property
    def free(self) -> int:
        return max(0, self.quota - self.used - self.reserved)


OCR_MIN_CHARS = 20


def needs_ocr(page_text: str) -> bool:
    """Spec 6.1: pages with under 20 characters of text get OCR."""
    return len(page_text.strip()) < OCR_MIN_CHARS
