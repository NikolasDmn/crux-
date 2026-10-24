-- Milestone 9: account export and deletion (spec 11, 9.4).

CREATE TABLE AccountExport (
    Id          BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId    CHAR(26)     NOT NULL,
    UserId      BIGINT       NOT NULL,
    Status      ENUM('Queued','Building','Ready','Failed') NOT NULL DEFAULT 'Queued',
    RequestedAt DATETIME     NOT NULL,
    ReadyAt     DATETIME     NULL,
    ExpiresAt   DATETIME     NULL,                     -- the download works for 24 h
    FileRef     CHAR(26)     NULL,                     -- the zip (files, Purpose Export)
    SizeBytes   BIGINT       NULL,
    Error       VARCHAR(500) NULL,
    UNIQUE KEY uq_export_public (PublicId),
    KEY idx_export_status (Status, RequestedAt),
    FOREIGN KEY (UserId) REFERENCES Users(Id) ON DELETE CASCADE
);

-- Not tied to Users by a foreign key: the row outlives the user it deletes (an audit trail).
CREATE TABLE AccountDeletion (
    UserRef         CHAR(26)  NOT NULL PRIMARY KEY,
    UserId          BIGINT    NOT NULL,
    RequestedAt     DATETIME  NOT NULL,
    CourseDecisions JSON      NOT NULL,                -- [{courseRef, action: transfer|delete, toUserRef}]
    Status          ENUM('Pending','Purging','Done','Cancelled') NOT NULL DEFAULT 'Pending',
    Acks            JSON      NULL,                    -- services that sent user.purged
    StartedAt       DATETIME  NULL,
    FinishedAt      DATETIME  NULL,
    KEY idx_deletion_status (Status, RequestedAt)
);
