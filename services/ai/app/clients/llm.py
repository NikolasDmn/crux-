"""Provider adapters (spec 7.2): `verify`, `stream_chat`, `complete_json`. Token counts and prices
come from app.domain.models. The plaintext key is passed per call and never stored or logged.

- anthropic: the official SDK (AsyncAnthropic). Claude Opus 5.5 and Sonnet 5.5 requests carry the
  server-side refusal fallback (beta, `fallbacks: "default"`), so a declined request is re-run on
  another model inside the same call.
- openai / mistral: chat completions over httpx (Mistral's API has the same shape).
- google: Gemini generateContent over httpx.
- fake: deterministic, no network; dev and tests only.
"""

import json
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass, field
from typing import Any, Protocol

import anthropic
import httpx


@dataclass
class Usage:
    input_tokens: int = 0
    output_tokens: int = 0


class LlmError(Exception):
    def __init__(self, message: str, *, auth: bool = False, retryable: bool = False) -> None:
        super().__init__(message)
        self.auth = auth
        self.retryable = retryable


Message = dict[str, str]  # {"role": "user"|"assistant", "content": "..."}
Chunk = str | Usage  # text deltas, then one Usage at the end


class Provider(Protocol):
    async def verify(self, api_key: str) -> bool: ...
    def stream_chat(
        self, api_key: str, model: str, system: str, messages: list[Message], max_tokens: int
    ) -> AsyncIterator[Chunk]: ...
    async def complete_json(
        self, api_key: str, model: str, system: str, messages: list[Message], schema: dict[str, Any], max_tokens: int
    ) -> tuple[dict[str, Any], Usage]: ...


# ---------- Anthropic (official SDK) ----------

FALLBACK_BETA = "server-side-fallback-2026-07-01"
#: Models that take `fallbacks: "default"` (the API picks the fallback by refusal category).
FALLBACK_MODELS = frozenset({"claude-opus-5-5", "claude-sonnet-5-5"})


def _anthropic_error(e: Exception) -> LlmError:
    if isinstance(e, anthropic.AuthenticationError | anthropic.PermissionDeniedError):
        return LlmError("the provider rejected the key", auth=True)
    if isinstance(e, anthropic.RateLimitError):
        return LlmError("rate limited by the provider", retryable=True)
    if isinstance(e, anthropic.APIStatusError):
        return LlmError(f"provider error {e.status_code}", retryable=e.status_code >= 500)
    if isinstance(e, anthropic.APIConnectionError):
        return LlmError("can't reach the provider", retryable=True)
    return LlmError(repr(e))


class AnthropicProvider:
    def _client(self, api_key: str) -> anthropic.AsyncAnthropic:
        return anthropic.AsyncAnthropic(api_key=api_key, max_retries=2)

    async def verify(self, api_key: str) -> bool:
        try:
            await self._client(api_key).models.list(limit=1)
            return True
        except (anthropic.AuthenticationError, anthropic.PermissionDeniedError):
            return False
        except Exception as e:
            raise _anthropic_error(e) from e

    def _stream(self, client: anthropic.AsyncAnthropic, **kw: Any) -> Any:
        if kw["model"] in FALLBACK_MODELS:
            return client.beta.messages.stream(betas=[FALLBACK_BETA], fallbacks="default", **kw)
        return client.messages.stream(**kw)

    async def stream_chat(
        self, api_key: str, model: str, system: str, messages: list[Message], max_tokens: int
    ) -> AsyncIterator[Chunk]:
        client = self._client(api_key)
        try:
            async with self._stream(client, model=model, max_tokens=max_tokens, system=system, messages=messages) as s:
                async for text in s.text_stream:
                    yield text
                final = await s.get_final_message()
        except Exception as e:
            raise _anthropic_error(e) from e
        if final.stop_reason == "refusal":
            raise LlmError("the model declined this request")
        yield Usage(final.usage.input_tokens, final.usage.output_tokens)

    async def complete_json(
        self, api_key: str, model: str, system: str, messages: list[Message], schema: dict[str, Any], max_tokens: int
    ) -> tuple[dict[str, Any], Usage]:
        client = self._client(api_key)
        try:
            async with self._stream(
                client,
                model=model,
                max_tokens=max_tokens,
                system=system,
                messages=messages,
                output_config={"format": {"type": "json_schema", "schema": schema}},
            ) as s:
                final = await s.get_final_message()
        except Exception as e:
            raise _anthropic_error(e) from e
        if final.stop_reason == "refusal":
            raise LlmError("the model declined this request")
        text = next((b.text for b in final.content if b.type == "text"), "")
        try:
            data = json.loads(text)
        except json.JSONDecodeError as e:
            raise LlmError("the model returned invalid JSON") from e
        return data, Usage(final.usage.input_tokens, final.usage.output_tokens)


# ---------- OpenAI-shaped chat completions (OpenAI, Mistral) ----------


def _http_error(r: httpx.Response) -> LlmError:
    if r.status_code in (401, 403):
        return LlmError("the provider rejected the key", auth=True)
    return LlmError(f"provider error {r.status_code}", retryable=r.status_code == 429 or r.status_code >= 500)


class ChatCompletionsProvider:
    def __init__(self, base_url: str, json_mode: str = "json_schema") -> None:
        self._base = base_url.rstrip("/")
        self._json_mode = json_mode

    def _headers(self, api_key: str) -> dict[str, str]:
        return {"authorization": f"Bearer {api_key}", "content-type": "application/json"}

    async def verify(self, api_key: str) -> bool:
        async with httpx.AsyncClient(timeout=10.0) as c:
            r = await c.get(f"{self._base}/models", headers=self._headers(api_key))
        if r.status_code in (401, 403):
            return False
        if r.status_code >= 400:
            raise _http_error(r)
        return True

    async def stream_chat(
        self, api_key: str, model: str, system: str, messages: list[Message], max_tokens: int
    ) -> AsyncIterator[Chunk]:
        body = {
            "model": model,
            "max_tokens": max_tokens,
            "stream": True,
            "stream_options": {"include_usage": True},
            "messages": [{"role": "system", "content": system}, *messages],
        }
        usage = Usage()
        async with httpx.AsyncClient(timeout=httpx.Timeout(120.0, connect=10.0)) as c:
            async with c.stream(
                "POST", f"{self._base}/chat/completions", headers=self._headers(api_key), json=body
            ) as r:
                if r.status_code >= 400:
                    await r.aread()
                    raise _http_error(r)
                async for line in r.aiter_lines():
                    if not line.startswith("data: ") or line == "data: [DONE]":
                        continue
                    event = json.loads(line[6:])
                    for choice in event.get("choices") or []:
                        text = (choice.get("delta") or {}).get("content")
                        if text:
                            yield text
                    if event.get("usage"):
                        usage = Usage(
                            event["usage"].get("prompt_tokens", 0), event["usage"].get("completion_tokens", 0)
                        )
        yield usage

    async def complete_json(
        self, api_key: str, model: str, system: str, messages: list[Message], schema: dict[str, Any], max_tokens: int
    ) -> tuple[dict[str, Any], Usage]:
        fmt: dict[str, Any] = (
            {"type": "json_schema", "json_schema": {"name": "result", "schema": schema, "strict": True}}
            if self._json_mode == "json_schema"
            else {"type": "json_object"}
        )
        body = {
            "model": model,
            "max_tokens": max_tokens,
            "response_format": fmt,
            "messages": [{"role": "system", "content": system + "\nAnswer with JSON only."}, *messages],
        }
        async with httpx.AsyncClient(timeout=httpx.Timeout(180.0, connect=10.0)) as c:
            r = await c.post(f"{self._base}/chat/completions", headers=self._headers(api_key), json=body)
        if r.status_code >= 400:
            raise _http_error(r)
        d = r.json()
        try:
            data = json.loads(d["choices"][0]["message"]["content"])
        except (KeyError, IndexError, json.JSONDecodeError) as e:
            raise LlmError("the model returned invalid JSON") from e
        u = d.get("usage") or {}
        return data, Usage(u.get("prompt_tokens", 0), u.get("completion_tokens", 0))


# ---------- Google Gemini ----------


class GoogleProvider:
    BASE = "https://generativelanguage.googleapis.com/v1beta"

    @staticmethod
    def _contents(messages: list[Message]) -> list[dict[str, Any]]:
        return [
            {"role": "model" if m["role"] == "assistant" else "user", "parts": [{"text": m["content"]}]}
            for m in messages
        ]

    async def verify(self, api_key: str) -> bool:
        async with httpx.AsyncClient(timeout=10.0) as c:
            r = await c.get(f"{self.BASE}/models", headers={"x-goog-api-key": api_key})
        if r.status_code in (400, 401, 403):
            return False
        if r.status_code >= 400:
            raise _http_error(r)
        return True

    async def _generate(self, api_key: str, model: str, body: dict[str, Any]) -> dict[str, Any]:
        async with httpx.AsyncClient(timeout=httpx.Timeout(180.0, connect=10.0)) as c:
            r = await c.post(
                f"{self.BASE}/models/{model}:generateContent", headers={"x-goog-api-key": api_key}, json=body
            )
        if r.status_code >= 400:
            raise _http_error(r)
        result: dict[str, Any] = r.json()
        return result

    @staticmethod
    def _text_and_usage(d: dict[str, Any]) -> tuple[str, Usage]:
        parts = ((d.get("candidates") or [{}])[0].get("content") or {}).get("parts") or []
        u = d.get("usageMetadata") or {}
        return "".join(p.get("text", "") for p in parts), Usage(
            u.get("promptTokenCount", 0), u.get("candidatesTokenCount", 0)
        )

    async def stream_chat(
        self, api_key: str, model: str, system: str, messages: list[Message], max_tokens: int
    ) -> AsyncIterator[Chunk]:
        # One response, yielded as one delta: the endpoint streams in large pieces anyway.
        body = {
            "systemInstruction": {"parts": [{"text": system}]},
            "contents": self._contents(messages),
            "generationConfig": {"maxOutputTokens": max_tokens},
        }
        text, usage = self._text_and_usage(await self._generate(api_key, model, body))
        if text:
            yield text
        yield usage

    async def complete_json(
        self, api_key: str, model: str, system: str, messages: list[Message], schema: dict[str, Any], max_tokens: int
    ) -> tuple[dict[str, Any], Usage]:
        body = {
            "systemInstruction": {"parts": [{"text": system}]},
            "contents": self._contents(messages),
            "generationConfig": {"maxOutputTokens": max_tokens, "responseMimeType": "application/json"},
        }
        text, usage = self._text_and_usage(await self._generate(api_key, model, body))
        try:
            return json.loads(text), usage
        except json.JSONDecodeError as e:
            raise LlmError("the model returned invalid JSON") from e


# ---------- fake (dev/tests) ----------


@dataclass
class FakeProvider:
    """Keys starting with `fake-valid` verify. Replies echo the last user message; `json_replies`
    (a queue) or `json_fn(schema, messages)` answers complete_json. `calls` records every request."""

    json_replies: list[dict[str, Any]] = field(default_factory=list)
    json_fn: Callable[[dict[str, Any], list[Message], str], dict[str, Any]] | None = None
    chat_reply: Callable[[str, list[Message]], str] | None = None
    calls: list[dict[str, Any]] = field(default_factory=list)
    fail_with: LlmError | None = None

    async def verify(self, api_key: str) -> bool:
        return api_key.startswith("fake-valid")

    async def stream_chat(
        self, api_key: str, model: str, system: str, messages: list[Message], max_tokens: int
    ) -> AsyncIterator[Chunk]:
        self.calls.append({"kind": "chat", "system": system, "messages": messages, "model": model})
        if self.fail_with:
            raise self.fail_with
        last = messages[-1]["content"] if messages else ""
        reply = self.chat_reply(system, messages) if self.chat_reply else f"You asked: {last[:200]} [p.1]"
        for i in range(0, len(reply), 16):
            yield reply[i : i + 16]
        yield Usage(len(system + last) // 4, len(reply) // 4)

    async def complete_json(
        self, api_key: str, model: str, system: str, messages: list[Message], schema: dict[str, Any], max_tokens: int
    ) -> tuple[dict[str, Any], Usage]:
        self.calls.append({"kind": "json", "system": system, "messages": messages, "schema": schema})
        if self.fail_with:
            raise self.fail_with
        if self.json_fn is not None:
            data = self.json_fn(schema, messages, system)
        elif self.json_replies:
            data = self.json_replies.pop(0)
        else:
            data = {}
        return data, Usage(len(system) // 4 + 10, len(json.dumps(data)) // 4)


def registry(fake: FakeProvider | None = None) -> dict[str, Provider]:
    providers: dict[str, Provider] = {
        "anthropic": AnthropicProvider(),
        "openai": ChatCompletionsProvider("https://api.openai.com/v1"),
        "mistral": ChatCompletionsProvider("https://api.mistral.ai/v1", json_mode="json_object"),
        "google": GoogleProvider(),
    }
    if fake is not None:
        providers["fake"] = fake
    return providers
