"""Large imports are read in parts (B8-16): how many, and whether text was left out.

Revision ID: 0002
"""

from alembic import op

revision = "0002"
down_revision = "0001"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.get_bind().exec_driver_sql(
        "ALTER TABLE CourseImport ADD COLUMN Parts SMALLINT NULL AFTER AiRunId, "
        "ADD COLUMN Truncated BOOLEAN NOT NULL DEFAULT FALSE AFTER Parts"
    )


def downgrade() -> None:
    raise NotImplementedError("migrations are append-only (backend-spec 2.2)")
