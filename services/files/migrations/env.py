"""Alembic environment for crux_files: synchronous PyMySQL, UTC session (schema header)."""

import os

from alembic import context
from sqlalchemy import create_engine, text

url = os.environ["CRUX_DATABASE_URL"].replace("+aiomysql", "+pymysql")
engine = create_engine(url)

with engine.connect() as connection:
    connection.execute(text("SET time_zone = '+00:00'"))
    context.configure(connection=connection, target_metadata=None, transaction_per_migration=True)
    with context.begin_transaction():
        context.run_migrations()
