import asyncio

from crux_common.worker import supervise


async def test_a_crashing_loop_is_restarted_until_it_finishes() -> None:
    runs = []

    async def flaky() -> None:
        runs.append(1)
        if len(runs) < 3:
            raise ConnectionError("store not up yet")

    await asyncio.wait_for(supervise("flaky", flaky, asyncio.Event(), delay=0.01), timeout=2)
    assert len(runs) == 3


async def test_stop_ends_a_crash_loop() -> None:
    stop = asyncio.Event()

    async def always_fails() -> None:
        stop.set()
        raise RuntimeError("boom")

    await asyncio.wait_for(supervise("broken", always_fails, stop, delay=10), timeout=2)
