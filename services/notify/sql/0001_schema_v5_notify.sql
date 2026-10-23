-- Exam Board platform schema v5: NOTIFY service database (exam_notify), 2026-10-23
-- Owner: the Notify service. In-app + email, every kind opt-in per user (Nick 2026-10-23):
-- no NotificationPreference row = that kind/channel is OFF.
-- Same server rules as schema-v5-core.sql. *Ref = another service's PublicId.

CREATE TABLE Recipient (                                     -- copy of what Notify needs, synced from Core events
    UserRef        CHAR(26)     NOT NULL PRIMARY KEY,
    Email          VARCHAR(255) NOT NULL,
    EmailVerified  BOOLEAN      NOT NULL DEFAULT FALSE,      -- never email an unverified address
    TimeZone       VARCHAR(64)  NOT NULL,
    Locale         VARCHAR(16)  NOT NULL DEFAULT 'en',
    UnsubscribeTokenHash CHAR(64) NOT NULL,                  -- one-click unsubscribe link in every email
    UpdatedAt      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

CREATE TABLE NotificationPreference (
    UserRef   CHAR(26)    NOT NULL,
    Kind      VARCHAR(48) NOT NULL,                          -- 'exam.eve', 'exam.checkin', 'flash.due', 'import.ready', 'plan.today'
    Channel   ENUM('InApp','Email') NOT NULL,
    Enabled   BOOLEAN     NOT NULL,
    UpdatedAt DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (UserRef, Kind, Channel)
);

CREATE TABLE Notification (
    Id          BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId    CHAR(26)     NOT NULL,
    UserRef     CHAR(26)     NOT NULL,
    Kind        VARCHAR(48)  NOT NULL,
    DedupeKey   VARCHAR(128) NOT NULL,                       -- 'exam.eve:<deadlineRef>'
    DueAt       DATETIME     NOT NULL,
    Payload     JSON         NULL,                           -- template variables + deep link
    CreatedAt   DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ReadAt      DATETIME     NULL,
    CancelledAt DATETIME     NULL,
    UNIQUE KEY uq_notification_public (PublicId),
    UNIQUE KEY uq_notification_dedupe (UserRef, DedupeKey),
    KEY idx_notification_user (UserRef, ReadAt, DueAt)
);

CREATE TABLE NotificationDelivery (                          -- one row per channel actually attempted
    NotificationId    BIGINT   NOT NULL,
    Channel           ENUM('InApp','Email') NOT NULL,
    Status            ENUM('Pending','Sent','Failed','Skipped') NOT NULL DEFAULT 'Pending',
    Attempts          INT      NOT NULL DEFAULT 0,
    ProviderMessageId VARCHAR(128) NULL,
    LastError         VARCHAR(500) NULL,
    NextAttemptAt     DATETIME NULL,
    SentAt            DATETIME NULL,
    PRIMARY KEY (NotificationId, Channel),
    KEY idx_delivery_due (Status, NextAttemptAt),
    FOREIGN KEY (NotificationId) REFERENCES Notification(Id) ON DELETE CASCADE
);

CREATE TABLE EmailSuppression (                              -- bounces, complaints, unsubscribe-all
    Email     VARCHAR(255) NOT NULL PRIMARY KEY,
    Reason    ENUM('Bounce','Complaint','Unsubscribe') NOT NULL,
    CreatedAt DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP
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
