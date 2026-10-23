"""Milestone 2 uploads: who uploaded, which course, tier label, and a durable job table.

- D2-4: course uploads count against the course owner (OwnerUserRef); the uploader is kept.
- CourseRef: lets core check a material's file belongs to its course, and the quota view group by course.
- FileJob: scan/extract work with retries (spec 10: "retries 3x with exponential backoff").

Revision ID: 0002
"""

from alembic import op

revision = "0002"
down_revision = "0001"
branch_labels = None
depends_on = None

STATEMENTS = [
    """ALTER TABLE StoredFile
        ADD COLUMN UploadedByUserRef CHAR(26) NULL AFTER OwnerUserRef,
        ADD COLUMN CourseRef CHAR(26) NULL AFTER UploadedByUserRef,
        ADD KEY idx_file_course (CourseRef)""",
    "ALTER TABLE UserStorage ADD COLUMN TierCode VARCHAR(32) NULL AFTER QuotaBytes",
    """CREATE TABLE FileJob (
        Id            BIGINT AUTO_INCREMENT PRIMARY KEY,
        FileId        BIGINT       NOT NULL,
        Kind          ENUM('Scan','Extract') NOT NULL,
        Status        ENUM('Pending','Running','Done','Failed') NOT NULL DEFAULT 'Pending',
        Attempts      INT          NOT NULL DEFAULT 0,
        NextAttemptAt DATETIME(3)  NOT NULL,
        StartedAt     DATETIME(3)  NULL,
        LastError     VARCHAR(500) NULL,
        CreatedAt     DATETIME(3)  NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
        UNIQUE KEY uq_job (FileId, Kind),
        KEY idx_job_due (Status, NextAttemptAt),
        FOREIGN KEY (FileId) REFERENCES StoredFile(Id) ON DELETE CASCADE
    )""",
]


def upgrade() -> None:
    for statement in STATEMENTS:
        op.get_bind().exec_driver_sql(statement)


def downgrade() -> None:
    raise NotImplementedError("migrations are append-only (backend-spec 2.2)")
