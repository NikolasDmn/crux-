"""Question images can be uploaded directly, not only cropped from a PDF page: only thumbnails
still need a source page.

Revision ID: 0004
"""

from alembic import op

revision = "0004"
down_revision = "0003"
branch_labels = None
depends_on = None


def upgrade() -> None:
    db = op.get_bind()
    # MySQL named the check itself; find it by what it checks.
    name = db.exec_driver_sql(
        "SELECT CONSTRAINT_NAME FROM information_schema.CHECK_CONSTRAINTS "
        "WHERE CONSTRAINT_SCHEMA = DATABASE() AND CHECK_CLAUSE LIKE '%%SourcePage%%'"
    ).scalar()
    if name:
        db.exec_driver_sql(f"ALTER TABLE StoredFile DROP CHECK `{name}`")
    db.exec_driver_sql(
        "ALTER TABLE StoredFile ADD CONSTRAINT chk_thumbnail_page "
        "CHECK (Purpose <> 'Thumbnail' OR SourcePage IS NOT NULL)"
    )


def downgrade() -> None:
    raise NotImplementedError("migrations are append-only (backend-spec 2.2)")
