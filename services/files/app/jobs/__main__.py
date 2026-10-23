import asyncio

from app.service.users import UserEventsHandler
from app.settings import get_settings
from crux_common.worker import run_worker

if __name__ == "__main__":
    asyncio.run(run_worker(get_settings(), [UserEventsHandler()]))
