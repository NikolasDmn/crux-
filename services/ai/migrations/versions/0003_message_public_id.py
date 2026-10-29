"""Chat messages get a PublicId (audit L5: the API showed the integer row id, and with it how many
messages Crux holds). Existing rows are given one here.

Revision ID: 0003
"""

from alembic import op

from crux_common.ids import new_id

revision = "0003"
down_revision = "0002"
branch_labels = None
depends_on = None


def upgrade() -> None:
    db = op.get_bind()
    db.exec_driver_sql("ALTER TABLE AiMessage ADD COLUMN PublicId CHAR(26) NULL AFTER Id")
    for (row_id,) in db.exec_driver_sql("SELECT Id FROM AiMessage WHERE PublicId IS NULL").fetchall():
        db.exec_driver_sql("UPDATE AiMessage SET PublicId = %s WHERE Id = %s", (new_id(), row_id))
    db.exec_driver_sql(
        "ALTER TABLE AiMessage MODIFY PublicId CHAR(26) NOT NULL, ADD UNIQUE KEY uq_message_public (PublicId)"
    )


def downgrade() -> None:
    raise NotImplementedError("migrations are append-only (backend-spec 2.2)")
