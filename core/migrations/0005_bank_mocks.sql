-- Milestone 3: PublicIds the API needs (spec 3.1: only PublicIds leave a service).
-- - QuestionVersion: events and the AI context name exact versions (spec 3.3 mock.submitted,
--   internal GET /questions/{versionRef}).
-- - MCOption: answers name the chosen option (`choiceOptionId`, spec 5.4).
-- - Slip, Trap: PATCH /slips/{id}, DELETE /courses/{id}/traps/{trapId}.
ALTER TABLE QuestionVersion ADD COLUMN PublicId CHAR(26) NULL AFTER Id, ADD UNIQUE KEY uq_version_public (PublicId);
ALTER TABLE MCOption        ADD COLUMN PublicId CHAR(26) NULL AFTER Id, ADD UNIQUE KEY uq_option_public (PublicId);
ALTER TABLE Slip            ADD COLUMN PublicId CHAR(26) NULL AFTER Id, ADD UNIQUE KEY uq_slip_public (PublicId);
ALTER TABLE Trap            ADD COLUMN PublicId CHAR(26) NULL AFTER Id, ADD UNIQUE KEY uq_trap_public (PublicId);

-- Spec 5.3 counts a guess as a miss ("wrong, blank or a guess"). The read model keeps the last
-- answer's confidence so "misses only" can find guessed-right questions too.
ALTER TABLE UserQuestionStat ADD COLUMN LastConfidence ENUM('Sure','Unsure','Guess') NULL AFTER LastOutcome;

-- WeakSpot rules apply check by check (crux_domain::weakspot::apply); the two counters the rules
-- need live on the row so a user editing Severity doesn't break a replay from the start.
ALTER TABLE WeakSpot
    ADD COLUMN MissesAtLevel TINYINT NOT NULL DEFAULT 0 AFTER ColdPassesNeeded,
    ADD COLUMN ColdPassRun   TINYINT NOT NULL DEFAULT 0 AFTER MissesAtLevel;

-- Deadlines are addressed by the API (mock builder `deadlineId`, milestone 5 routes).
ALTER TABLE Deadline ADD COLUMN PublicId CHAR(26) NULL AFTER Id, ADD UNIQUE KEY uq_deadline_public (PublicId);
