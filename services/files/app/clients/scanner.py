"""Virus scanning (spec 6.1 step 3). ClamAV over clamd's INSTREAM protocol; a fake for dev (D2-7)."""

import asyncio
import struct
from collections.abc import AsyncIterator
from typing import Protocol

#: The standard antivirus test file (not a virus). The fake scanner flags it, so the "infected" path
#: is exercised in dev and tests without ClamAV.
EICAR = b"X5O!P%@AP[4\\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*"


class Scanner(Protocol):
    async def scan(self, chunks: AsyncIterator[bytes]) -> str:
        """Returns 'Clean' or 'Infected'; raises when the scan itself fails."""
        ...


class FakeScanner:
    async def scan(self, chunks: AsyncIterator[bytes]) -> str:
        tail = b""
        async for chunk in chunks:
            window = tail + chunk
            if EICAR in window:
                return "Infected"
            tail = window[-len(EICAR) :]
        return "Clean"


class ClamdScanner:
    """clamd INSTREAM: 'zINSTREAM\\0', then <len:u32 big-endian><data> chunks, then a zero length."""

    def __init__(self, host: str, port: int, timeout: float = 60.0) -> None:
        self.host, self.port, self.timeout = host, port, timeout

    async def scan(self, chunks: AsyncIterator[bytes]) -> str:
        reader, writer = await asyncio.wait_for(asyncio.open_connection(self.host, self.port), self.timeout)
        try:
            writer.write(b"zINSTREAM\0")
            async for chunk in chunks:
                for i in range(0, len(chunk), 64 * 1024):
                    part = chunk[i : i + 64 * 1024]
                    writer.write(struct.pack(">I", len(part)) + part)
                    await writer.drain()
            writer.write(struct.pack(">I", 0))
            await writer.drain()
            reply = (await asyncio.wait_for(reader.read(4096), self.timeout)).rstrip(b"\0").decode(errors="replace")
        finally:
            writer.close()
        # "stream: OK" | "stream: Eicar-Signature FOUND" | "... ERROR"
        if reply.endswith("OK"):
            return "Clean"
        if reply.endswith("FOUND"):
            return "Infected"
        raise RuntimeError(f"clamd: {reply}")
