import asyncio

from app.service.users import UserEventsHandler
from app.settings import get_settings
from crux_common.worker import run_worker

if __name__ == "__main__":
    settings = get_settings()
    asyncio.run(run_worker(settings, [UserEventsHandler(settings.unsubscribe_secret)]))
