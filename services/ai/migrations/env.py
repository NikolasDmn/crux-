"""Alembic environment for crux_ai: synchronous PyMySQL, UTC session, one migrator at a time."""

import os

from crux_common.migrations import run_online

run_online(os.environ["CRUX_DATABASE_URL"])
