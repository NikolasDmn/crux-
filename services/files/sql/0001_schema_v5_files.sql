-- Exam Board platform schema v5: FILES service database (exam_files), 2026-10-23
-- Owner: the Files service (Python). Upload, virus scan, PDF text extraction, thumbnails,
-- question-image crops, the storage saver and per-user storage quotas.
-- Same server rules as schema-v5-core.sql (utf8mb4, UTC, strict mode). *Ref = another service's PublicId.

-- One row per stored object. The row outlives the object: after the storage saver purges an
-- original, the row (with Sha256) and its extracted text stay, so AI tutoring, search and
-- citations keep working and a re-upload of the same file reattaches to the same row.
CREATE TABLE StoredFile (
    Id               BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId         CHAR(26)     NOT NULL,
    OwnerUserRef     CHAR(26)     NOT NULL,             -- Core Users.PublicId
    Purpose          ENUM('Original','QuestionImage','Thumbnail','Export','AnswerImage') NOT NULL,
    DerivedFromFileId BIGINT      NULL,                 -- crop/thumbnail: the original it was cut from
    SourcePage       INT          NULL,                 -- crop/thumbnail: 1-based page in the original
    CropBox          JSON         NULL,                 -- crop: {"x":..,"y":..,"w":..,"h":..} in PDF points
    StorageKey       VARCHAR(512) NULL,                 -- NULL once purged
    FileName         VARCHAR(255) NOT NULL,
    MimeType         VARCHAR(127) NOT NULL,             -- sniffed server-side; only application/pdf accepted for originals (for now)
    SizeBytes        BIGINT       NOT NULL,
    Sha256           CHAR(64)     NOT NULL,
    PageCount        INT          NULL,
    Status           ENUM('Uploading','Active','PendingPurge','Purged','Deleted') NOT NULL DEFAULT 'Uploading',
    ScanStatus       ENUM('Pending','Clean','Infected','Failed') NOT NULL DEFAULT 'Pending',
    ExtractStatus    ENUM('NotNeeded','Pending','Done','Failed') NOT NULL DEFAULT 'Pending',
    ExtractorVersion VARCHAR(16)  NULL,
    PurgeRequestedAt DATETIME     NULL,
    PurgeAt          DATETIME     NULL,                 -- end of the 7-day undo window
    PurgedAt         DATETIME     NULL,
    UploadedAt       DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    RowVersion       INT          NOT NULL DEFAULT 0,
    -- one Original row per owner per fingerprint (MySQL has no partial index: generated column)
    OriginalSha      CHAR(64) AS (IF(Purpose = 'Original', Sha256, NULL)) STORED,
    UNIQUE KEY uq_file_public (PublicId),
    UNIQUE KEY uq_file_key (StorageKey),
    UNIQUE KEY uq_file_original (OwnerUserRef, OriginalSha),
    KEY idx_file_owner (OwnerUserRef, Purpose, Status),
    KEY idx_file_purge (Status, PurgeAt),
    KEY idx_file_derived (DerivedFromFileId),
    FOREIGN KEY (DerivedFromFileId) REFERENCES StoredFile(Id) ON DELETE RESTRICT,
    CHECK (SizeBytes >= 0),
    CHECK ((Status = 'PendingPurge') = (PurgeAt IS NOT NULL)),
    CHECK ((Status IN ('Purged','Deleted')) = (StorageKey IS NULL)),
    CHECK (Purpose = 'Original' OR Status <> 'PendingPurge'),        -- the storage saver only purges originals
    CHECK (Purpose NOT IN ('QuestionImage','Thumbnail') OR SourcePage IS NOT NULL)
);
-- App rules: purging an original never touches its QuestionImage crops (they are separate objects);
-- thumbnails are created only when the user opted in; Infected => object deleted, Status 'Deleted'.

-- Extracted text, per page or section. Survives purge of the original.
CREATE TABLE FileChunk (
    Id               BIGINT AUTO_INCREMENT PRIMARY KEY,
    FileId           BIGINT       NOT NULL,
    PageNo           INT          NULL,
    ChunkNo          INT          NOT NULL,
    Heading          VARCHAR(255) NULL,
    Text             MEDIUMTEXT   NOT NULL,
    ContentSha       CHAR(64)     NOT NULL,
    ExtractMethod    ENUM('Text','OCR','Vision') NOT NULL,
    ExtractorVersion VARCHAR(16)  NOT NULL,
    EmbeddingModel   VARCHAR(64)  NULL,
    EmbeddingRef     VARCHAR(128) NULL,
    UNIQUE KEY uq_chunk (FileId, ChunkNo),
    KEY idx_chunk_page (FileId, PageNo),
    FULLTEXT KEY ft_chunk_text (Heading, Text),
    FOREIGN KEY (FileId) REFERENCES StoredFile(Id) ON DELETE CASCADE
);

-- Quota per user. QuotaBytes is copied from the user's Core tier on 'user.tier_changed'.
-- Reserve on upload start, settle on complete (same pattern as the AI budget):
--   UPDATE UserStorage SET ReservedBytes = ReservedBytes + :size
--    WHERE UserRef = ? AND UsedBytes + ReservedBytes + :size <= QuotaBytes
-- Purged and Deleted objects don't count; PendingPurge still counts until the purge runs.
CREATE TABLE UserStorage (
    UserRef       CHAR(26) NOT NULL PRIMARY KEY,
    QuotaBytes    BIGINT   NOT NULL,
    UsedBytes     BIGINT   NOT NULL DEFAULT 0,
    ReservedBytes BIGINT   NOT NULL DEFAULT 0,
    AutoPurgeOriginals BOOLEAN NOT NULL DEFAULT FALSE,  -- "delete originals once processed" (off by default)
    KeepThumbnails     BOOLEAN NOT NULL DEFAULT FALSE,  -- opt-in page thumbnails (~30 KB/page)
    UpdatedAt     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    RowVersion    INT      NOT NULL DEFAULT 0,
    CHECK (QuotaBytes >= 0 AND UsedBytes >= 0 AND ReservedBytes >= 0)
);

CREATE TABLE Outbox (
    Id           BIGINT AUTO_INCREMENT PRIMARY KEY,
    EventId      CHAR(26)    NOT NULL,                   -- ULID, global across services
    Topic        VARCHAR(64) NOT NULL,                   -- 'mock.submitted', 'material.purge_requested'
    SchemaVersion SMALLINT   NOT NULL DEFAULT 1,
    AggregateRef CHAR(26)    NULL,                       -- PublicId of the row it is about (ordering key)
    Payload      JSON        NOT NULL,
    CreatedAt    DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PublishedAt  DATETIME(3) NULL,
    Attempts     INT         NOT NULL DEFAULT 0,
    UNIQUE KEY uq_outbox_event (EventId),
    KEY idx_outbox_pending (PublishedAt, Id)
);

CREATE TABLE ConsumedEvent (
    EventId    CHAR(26)    NOT NULL PRIMARY KEY,
    Topic      VARCHAR(64) NOT NULL,
    ConsumedAt DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
);
