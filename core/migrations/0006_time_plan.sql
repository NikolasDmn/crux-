-- Milestone 5: time, plan, next (OPEN-QUESTIONS.md B5-x).

-- Sessions are listed and created through the API (timer + manual).
ALTER TABLE StudySession
    ADD COLUMN PublicId CHAR(26) NULL AFTER Id,
    ADD UNIQUE KEY uq_session_public (PublicId);

-- What a plan entry is for (/next picks the step kind from it), and a pin: entries the user made,
-- edited or moved are never moved by a rebuild (spec 5.8).
ALTER TABLE StudyPlanEntry
    ADD COLUMN Kind     ENUM('Lecture','Revision','Mock','Other') NOT NULL DEFAULT 'Other' AFTER ComponentId,
    ADD COLUMN PinnedAt DATETIME NULL AFTER CreatedBy,
    ADD KEY idx_plan_user_course (UserId, CourseId, StartAt);

-- deadlines.tick runs every 15 min: remember what it already requested (notify dedupes as well).
ALTER TABLE Deadline
    ADD COLUMN EveRequestedAt     DATETIME NULL,
    ADD COLUMN CheckInRequestedAt DATETIME NULL;

-- Spec 5.11 read model: one row per user and ISO week, rebuilt nightly and on demand.
CREATE TABLE WeekSummary (
    UserId    BIGINT   NOT NULL,
    IsoWeek   CHAR(8)  NOT NULL,                      -- '2026-W43'
    Data      JSON     NOT NULL,
    RebuiltAt DATETIME NOT NULL,
    PRIMARY KEY (UserId, IsoWeek),
    FOREIGN KEY (UserId) REFERENCES Users(Id) ON DELETE CASCADE
);
