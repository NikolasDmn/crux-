"""Smartness levels per feature (plans-and-billing.md, stage 1): a JSON map feature -> level.
NULL means every feature uses its default.

Revision ID: 0004
"""

from alembic import op

revision = "0004"
down_revision = "0003"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.get_bind().exec_driver_sql("ALTER TABLE UserAiSettings ADD COLUMN Levels JSON NULL AFTER DefaultModel")


def downgrade() -> None:
    raise NotImplementedError("migrations are append-only (backend-spec 2.2)")
