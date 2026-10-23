"""SMTP delivery (spec 8: "an SMTP provider (config)"). `smtp://host:port` or `smtps://user:pass@host:port`."""

import asyncio
import smtplib
from email.message import EmailMessage
from typing import Protocol
from urllib.parse import unquote, urlparse


class Mailer(Protocol):
    async def send(self, to: str, subject: str, body: str, headers: dict[str, str]) -> str: ...


class SmtpMailer:
    def __init__(self, url: str, sender: str) -> None:
        self._url = urlparse(url)
        self._sender = sender

    def _send_sync(self, msg: EmailMessage) -> None:
        host, port = self._url.hostname or "localhost", self._url.port
        if self._url.scheme == "smtps":
            client: smtplib.SMTP = smtplib.SMTP_SSL(host, port or 465, timeout=10)
        else:
            client = smtplib.SMTP(host, port or 25, timeout=10)
        with client:
            if self._url.scheme == "smtp+starttls":
                client.starttls()
            if self._url.username:
                client.login(unquote(self._url.username), unquote(self._url.password or ""))
            client.send_message(msg)

    async def send(self, to: str, subject: str, body: str, headers: dict[str, str]) -> str:
        msg = EmailMessage()
        msg["From"] = self._sender
        msg["To"] = to
        msg["Subject"] = subject
        for k, v in headers.items():
            msg[k] = v
        msg.set_content(body)
        await asyncio.to_thread(self._send_sync, msg)
        return str(msg.get("Message-ID", ""))
