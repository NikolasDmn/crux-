"""schema-v5-notify.sql, verbatim (backend-spec 2.2: the schema file is migration 0001).

Revision ID: 0001
"""

from pathlib import Path

from alembic import op

from crux_common.migrations import read_statements

revision = "0001"
down_revision = None
branch_labels = None
depends_on = None

SQL = Path(__file__).resolve().parents[2] / "sql" / "0001_schema_v5_notify.sql"


def upgrade() -> None:
    for statement in read_statements(SQL):
        op.get_bind().exec_driver_sql(statement)


def downgrade() -> None:
    raise NotImplementedError("migrations are append-only (backend-spec 2.2)")
