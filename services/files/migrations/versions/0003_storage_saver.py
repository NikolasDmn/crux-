"""Milestone 7 storage saver: thumbnails are FileJob work too.

Revision ID: 0003
"""

from alembic import op

revision = "0003"
down_revision = "0002"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.get_bind().exec_driver_sql("ALTER TABLE FileJob MODIFY COLUMN Kind ENUM('Scan','Extract','Thumbnail') NOT NULL")


def downgrade() -> None:
    raise NotImplementedError("migrations are append-only (backend-spec 2.2)")
