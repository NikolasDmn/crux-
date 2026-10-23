-- Exam Board platform schema v5: AI service database (exam_ai), 2026-10-23
-- Owner: the AI service (Python). Everything AI is opt-in; with no UserAiSettings row (or Enabled =
-- FALSE) the service refuses every call for that user. Users bring their own provider keys.
-- Same server rules as schema-v5-core.sql. *Ref = another service's PublicId (no FK possible).

-- ---------- settings, keys, consent ----------
CREATE TABLE UserAiSettings (
    UserRef              CHAR(26) NOT NULL PRIMARY KEY,
    Enabled              BOOLEAN  NOT NULL DEFAULT FALSE,
    AllowMaterialChat    BOOLEAN  NOT NULL DEFAULT TRUE,
    AllowGrading         BOOLEAN  NOT NULL DEFAULT TRUE,
    AllowCourseImport    BOOLEAN  NOT NULL DEFAULT TRUE,
    AllowPastExamImport  BOOLEAN  NOT NULL DEFAULT TRUE,     -- past exams -> bank questions (Nick: opt-in, AI only)
    AllowGeneration      BOOLEAN  NOT NULL DEFAULT TRUE,
    NightlyGeneration    BOOLEAN  NOT NULL DEFAULT FALSE,    -- nightly top-up of untested questions
    ShareProgress        BOOLEAN  NOT NULL DEFAULT TRUE,
    ShareMistakes        BOOLEAN  NOT NULL DEFAULT TRUE,
    ShareAnswers         BOOLEAN  NOT NULL DEFAULT TRUE,
    AutoApproveGenerated BOOLEAN  NOT NULL DEFAULT TRUE,
    DefaultProviderKeyId BIGINT   NULL,
    DefaultModel         VARCHAR(64) NULL,
    DefaultTutorTemplateId BIGINT NULL,
    MonthlyCapMicroUsd   BIGINT   NULL,                      -- user's own spending cap on their key; NULL = none
    ConsentPolicyVersion VARCHAR(16) NULL,
    ConsentedAt          DATETIME NULL,
    UpdatedAt            DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    RowVersion           INT      NOT NULL DEFAULT 0,
    CHECK (Enabled = FALSE OR ConsentedAt IS NOT NULL)
);

-- Bring-your-own keys, one or more per provider. Envelope-encrypted; plaintext only in memory for a call.
CREATE TABLE AiProviderKey (
    Id             BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId       CHAR(26)     NOT NULL,
    UserRef        CHAR(26)     NOT NULL,
    Provider       VARCHAR(32)  NOT NULL,                    -- 'anthropic', 'openai', 'google', 'mistral' (registry in code)
    Label          VARCHAR(64)  NOT NULL DEFAULT 'default',
    Ciphertext     VARBINARY(1024) NOT NULL,
    KmsKeyId       VARCHAR(64)  NOT NULL,
    Last4          CHAR(4)      NOT NULL,
    Status         ENUM('Unverified','Valid','Invalid','Revoked') NOT NULL DEFAULT 'Unverified',
    LastVerifiedAt DATETIME     NULL,
    CreatedAt      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    RevokedAt      DATETIME     NULL,
    UNIQUE KEY uq_key_public (PublicId),
    UNIQUE KEY uq_key_label (UserRef, Provider, Label),
    CHECK ((Status = 'Revoked') = (RevokedAt IS NOT NULL))
);

-- Tutor templates: personality + teaching style (Nick 2026-10-23). Built-ins have OwnerUserRef NULL.
-- Templates are versioned like questions: editing inserts Version + 1, runs record the version used.
CREATE TABLE AiTutorTemplate (
    Id            BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId      CHAR(26)     NOT NULL,
    TemplateKey   VARCHAR(48)  NOT NULL,                     -- stable across versions: 'strict-coach'
    Version       INT          NOT NULL DEFAULT 1,
    OwnerUserRef  CHAR(26)     NULL,
    Name          VARCHAR(64)  NOT NULL,
    Description   VARCHAR(255) NOT NULL,
    Personality   ENUM('Direct','Warm','Socratic','Playful','Formal') NOT NULL,
    TeachingStyle ENUM('HintFirst','ExplainFirst','QuizMe','WorkedExamples') NOT NULL,
    Rules         JSON         NOT NULL,                     -- {"approachCheckYesNoHint":true,"oneQuestionAtATime":true,"maxWords":250}
    PromptText    TEXT         NOT NULL,                     -- the template's system-prompt block
    IsActive      BOOLEAN      NOT NULL DEFAULT TRUE,
    CreatedAt     DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uq_template_public (PublicId),
    UNIQUE KEY uq_template_version (OwnerUserRef, TemplateKey, Version),
    CHECK (Version >= 1)
);

ALTER TABLE UserAiSettings
    ADD FOREIGN KEY (DefaultProviderKeyId)   REFERENCES AiProviderKey(Id)   ON DELETE SET NULL,
    ADD FOREIGN KEY (DefaultTutorTemplateId) REFERENCES AiTutorTemplate(Id) ON DELETE SET NULL;

-- Per-course AI choices (some courses forbid AI; a different tutor per course).
CREATE TABLE AiCourseSetting (
    UserRef           CHAR(26) NOT NULL,
    CourseRef         CHAR(26) NOT NULL,
    AiEnabled         BOOLEAN  NULL,                         -- NULL = follow the user setting
    TutorTemplateId   BIGINT   NULL,
    NightlyGeneration BOOLEAN  NULL,
    GenerationPolicy  JSON     NULL,                         -- {"basin":{"7":3,"3":2,"0":3},"minTarget":2,"maxPerRun":50}
    PRIMARY KEY (UserRef, CourseRef),
    FOREIGN KEY (TutorTemplateId) REFERENCES AiTutorTemplate(Id) ON DELETE SET NULL
);

CREATE TABLE AiConsentLog (
    Id            BIGINT AUTO_INCREMENT PRIMARY KEY,
    UserRef       CHAR(26)    NOT NULL,
    ChangedAt     DATETIME(3) NOT NULL,
    PolicyVersion VARCHAR(16) NOT NULL,
    Delta         JSON        NOT NULL,
    KEY idx_consent_user (UserRef, ChangedAt)
);

-- ---------- runs, cost, context ----------
CREATE TABLE AiRun (
    Id                BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId          CHAR(26)     NOT NULL,
    UserRef           CHAR(26)     NOT NULL,
    CourseRef         CHAR(26)     NULL,
    ParentRunId       BIGINT       NULL,
    Feature           ENUM('MaterialChat','Grading','CourseImport','PastExamImport','Generation','Extraction','Other') NOT NULL,
    ProviderKeyId     BIGINT       NULL,                     -- the user's key that paid for it
    Provider          VARCHAR(32)  NOT NULL,
    Model             VARCHAR(64)  NOT NULL,
    PromptVersion     VARCHAR(32)  NOT NULL,
    TutorTemplateId   BIGINT       NULL,                     -- exact template version used
    IdempotencyKey    VARCHAR(64)  NULL,
    ExternalRequestId VARCHAR(128) NULL,
    Status            ENUM('Queued','Running','Succeeded','Failed','Cancelled','Blocked') NOT NULL DEFAULT 'Queued',
    InputTokens       INT          NULL,
    OutputTokens      INT          NULL,
    CacheReadTokens   INT          NULL,
    CacheWriteTokens  INT          NULL,
    CostMicroUsd      BIGINT       NULL,                     -- estimate from the provider price table (the user is billed by the provider)
    TranscriptRef     VARCHAR(512) NULL,
    RetainUntil       DATETIME     NULL,
    Error             TEXT         NULL,
    CreatedAt         DATETIME(3)  NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    StartedAt         DATETIME(3)  NULL,
    FinishedAt        DATETIME(3)  NULL,
    UNIQUE KEY uq_run_public (PublicId),
    UNIQUE KEY uq_run_idem (UserRef, IdempotencyKey),
    KEY idx_run_user_time (UserRef, CreatedAt),
    KEY idx_run_status (Status, CreatedAt),
    KEY idx_run_parent (ParentRunId),
    FOREIGN KEY (ParentRunId)     REFERENCES AiRun(Id)           ON DELETE CASCADE,
    FOREIGN KEY (ProviderKeyId)   REFERENCES AiProviderKey(Id)   ON DELETE SET NULL,
    FOREIGN KEY (TutorTemplateId) REFERENCES AiTutorTemplate(Id) ON DELETE SET NULL
);

CREATE TABLE AiUsageMonthly (                                -- enforces MonthlyCapMicroUsd (reserve, then settle)
    UserRef          CHAR(26) NOT NULL,
    Month            CHAR(7)  NOT NULL,
    SpentMicroUsd    BIGINT   NOT NULL DEFAULT 0,
    ReservedMicroUsd BIGINT   NOT NULL DEFAULT 0,
    Runs             INT      NOT NULL DEFAULT 0,
    PRIMARY KEY (UserRef, Month),
    CHECK (SpentMicroUsd >= 0 AND ReservedMicroUsd >= 0)
);

CREATE TABLE AiRunContext (                                  -- every record that went into the prompt
    AiRunId    BIGINT      NOT NULL,
    EntityType VARCHAR(32) NOT NULL,                         -- 'core.WeakSpot', 'files.FileChunk', ...
    EntityRef  VARCHAR(40) NOT NULL,                         -- PublicId, or '<fileRef>:<chunkNo>' for chunks
    PRIMARY KEY (AiRunId, EntityType, EntityRef),
    FOREIGN KEY (AiRunId) REFERENCES AiRun(Id) ON DELETE CASCADE
);

-- ---------- side-panel chat ----------
CREATE TABLE AiConversation (
    Id              BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId        CHAR(26)     NOT NULL,
    UserRef         CHAR(26)     NOT NULL,
    CourseRef       CHAR(26)     NOT NULL,
    Scope           ENUM('Material','Component','Question','Mock','Course') NOT NULL,
    MaterialRef     CHAR(26)     NULL,
    ComponentRef    CHAR(26)     NULL,
    QuestionRef     CHAR(26)     NULL,
    MockRef         CHAR(26)     NULL,
    TutorTemplateId BIGINT       NULL,                       -- NULL = course/user default at send time
    Title           VARCHAR(255) NULL,
    CreatedAt       DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ArchivedAt      DATETIME     NULL,
    UNIQUE KEY uq_conv_public (PublicId),
    KEY idx_conv_user (UserRef, CourseRef, CreatedAt),
    KEY idx_conv_material (UserRef, MaterialRef),
    FOREIGN KEY (TutorTemplateId) REFERENCES AiTutorTemplate(Id) ON DELETE SET NULL,
    CHECK ((Scope <> 'Material'  OR MaterialRef  IS NOT NULL)
       AND (Scope <> 'Component' OR ComponentRef IS NOT NULL)
       AND (Scope <> 'Question'  OR QuestionRef  IS NOT NULL)
       AND (Scope <> 'Mock'      OR MockRef      IS NOT NULL))
);

CREATE TABLE AiMessage (
    Id             BIGINT AUTO_INCREMENT PRIMARY KEY,
    ConversationId BIGINT      NOT NULL,
    Role           ENUM('User','Assistant') NOT NULL,
    Content        MEDIUMTEXT  NOT NULL,
    Anchor         JSON        NULL,                         -- {"fileRef":..,"page":12,"quote":"..."}
    AiRunId        BIGINT      NULL,
    Feedback       ENUM('Up','Down') NULL,
    CreatedAt      DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    KEY idx_msg_conv (ConversationId, CreatedAt),
    FOREIGN KEY (ConversationId) REFERENCES AiConversation(Id) ON DELETE CASCADE,
    FOREIGN KEY (AiRunId)        REFERENCES AiRun(Id)          ON DELETE SET NULL,
    CHECK (Role = 'Assistant' OR Feedback IS NULL)
);

CREATE TABLE AiCitation (
    MessageId BIGINT       NOT NULL,
    FileRef   CHAR(26)     NOT NULL,
    ChunkNo   INT          NOT NULL,
    PageNo    INT          NULL,
    Quote     VARCHAR(500) NULL,
    PRIMARY KEY (MessageId, FileRef, ChunkNo),
    FOREIGN KEY (MessageId) REFERENCES AiMessage(Id) ON DELETE CASCADE
);

-- ---------- grading suggestions ----------
-- Core owns the grade. This is only the suggestion; Core copies an accepted one into MockQuestion
-- (GradedBy 'AI', AcceptedAiGradeRef) and emits 'mock.ai_grade_decided', which sets Status here.
CREATE TABLE AiGrade (
    Id               BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId         CHAR(26)     NOT NULL,
    UserRef          CHAR(26)     NOT NULL,
    MockRef          CHAR(26)     NOT NULL,
    QuestionOrder    INT          NOT NULL,
    QuestionVersionRef CHAR(26)   NULL,
    AiRunId          BIGINT       NOT NULL,
    SuggestedOutcome ENUM('Right','Partial','Wrong','Blank') NOT NULL,
    SuggestedGrade   DECIMAL(6,2) NOT NULL,
    MaxGrade         DECIMAL(6,2) NOT NULL,
    Feedback         TEXT         NOT NULL,
    SlipsJson        JSON         NULL,
    Status           ENUM('Pending','Accepted','Overridden','Rejected','Superseded') NOT NULL DEFAULT 'Pending',
    DecidedAt        DATETIME     NULL,
    CreatedAt        DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uq_grade_public (PublicId),
    KEY idx_grade_mock (MockRef, QuestionOrder, Status),
    FOREIGN KEY (AiRunId) REFERENCES AiRun(Id) ON DELETE CASCADE,
    CHECK (SuggestedGrade >= 0 AND SuggestedGrade <= MaxGrade)
);

-- ---------- course import (Core applies proposals; this service never writes the catalog) ----------
CREATE TABLE CourseImport (
    Id              BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId        CHAR(26) NOT NULL,
    UserRef         CHAR(26) NOT NULL,
    CourseRef       CHAR(26) NULL,                           -- NULL until Core created the course
    Status          ENUM('Uploading','Queued','Extracting','Proposed','Applying','Applied','Failed','Cancelled') NOT NULL DEFAULT 'Uploading',
    Instructions    TEXT     NULL,
    ImportPastExams BOOLEAN  NOT NULL DEFAULT FALSE,         -- turn past exams into bank questions (opt-in)
    AiRunId         BIGINT   NULL,
    CreatedAt       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    AppliedAt       DATETIME NULL,
    RowVersion      INT      NOT NULL DEFAULT 0,
    UNIQUE KEY uq_import_public (PublicId),
    KEY idx_import_user (UserRef, CreatedAt),
    FOREIGN KEY (AiRunId) REFERENCES AiRun(Id) ON DELETE SET NULL
);

CREATE TABLE CourseImportFile (
    ImportId     BIGINT   NOT NULL,
    FileRef      CHAR(26) NOT NULL,
    DetectedKind ENUM('Slides','Textbook','Exercise','PastExam','Notes','Syllabus','Schedule','Other') NULL,
    Status       ENUM('Pending','Extracted','Skipped','Failed') NOT NULL DEFAULT 'Pending',
    Note         VARCHAR(255) NULL,
    PRIMARY KEY (ImportId, FileRef),
    FOREIGN KEY (ImportId) REFERENCES CourseImport(Id) ON DELETE CASCADE
);

CREATE TABLE ImportProposal (
    Id                   BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId             CHAR(26)    NOT NULL,
    ImportId             BIGINT      NOT NULL,
    Kind                 ENUM('Course','Component','Material','Concept','Paper','Question','Flashcard',
                              'Deadline','DeadlineComponent','PlanEntry','ExamAids') NOT NULL,
    TempKey              VARCHAR(64) NOT NULL,
    ParentTempKey        VARCHAR(64) NULL,
    Payload              JSON        NOT NULL,
    PayloadSchemaVersion SMALLINT    NOT NULL DEFAULT 1,
    UserEdits            JSON        NULL,
    SourceFileRef        CHAR(26)    NULL,
    SourcePage           INT         NULL,
    Confidence           ENUM('High','Medium','Low') NOT NULL DEFAULT 'Medium',
    Rationale            VARCHAR(500) NULL,
    DuplicateOfRef       CHAR(26)    NULL,                   -- an existing Core row this matches
    Status               ENUM('Pending','Accepted','Edited','Rejected','Applied','Failed') NOT NULL DEFAULT 'Pending',
    CreatedEntityRef     CHAR(26)    NULL,                   -- Core PublicId after apply
    RowVersion           INT         NOT NULL DEFAULT 0,
    UNIQUE KEY uq_proposal_public (PublicId),
    UNIQUE KEY uq_proposal_key (ImportId, Kind, TempKey),
    KEY idx_proposal_status (ImportId, Status),
    FOREIGN KEY (ImportId) REFERENCES CourseImport(Id) ON DELETE CASCADE,
    CHECK (JSON_TYPE(Payload) = 'OBJECT'),
    CHECK (Status <> 'Applied' OR CreatedEntityRef IS NOT NULL)
);

-- ---------- question generation (button or nightly) ----------
CREATE TABLE GenerationJob (
    Id           BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId     CHAR(26) NOT NULL,
    UserRef      CHAR(26) NOT NULL,
    CourseRef    CHAR(26) NOT NULL,
    TriggeredBy  ENUM('Button','Nightly') NOT NULL,
    Request      JSON     NOT NULL,                          -- {"componentRef":..,"n":10,"seedQuestionRef":..}
    Status       ENUM('Queued','Running','Done','Failed','Blocked') NOT NULL DEFAULT 'Queued',
    AiRunId      BIGINT   NULL,
    CreatedCount INT      NOT NULL DEFAULT 0,
    RejectedCount INT     NOT NULL DEFAULT 0,               -- failed validation (dup, bad key)
    CreatedAt    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FinishedAt   DATETIME NULL,
    UNIQUE KEY uq_gen_public (PublicId),
    KEY idx_gen_user (UserRef, CreatedAt),
    FOREIGN KEY (AiRunId) REFERENCES AiRun(Id) ON DELETE SET NULL
);

-- ---------- built-in tutor templates ----------
INSERT INTO AiTutorTemplate (PublicId, TemplateKey, Name, Description, Personality, TeachingStyle, Rules, PromptText) VALUES
('01JBUILTIN0000000000000001', 'strict-coach', 'Strict coach',
 'Direct, no pep talk. Hints before answers; asks you to work it out.',
 'Direct', 'HintFirst',
 JSON_OBJECT('approachCheckYesNoHint', TRUE, 'oneQuestionAtATime', TRUE, 'hintsBeforeSolution', 2, 'maxWords', 250),
 'You are a direct exam coach. Never solve a problem the student is working on unless they are stuck after two hints or ask for the solution. If they ask whether their approach is right, answer only yes, or no plus one hint, then ask what they get. No pep talk.'),
('01JBUILTIN0000000000000002', 'friendly-explainer', 'Friendly explainer',
 'Warm and patient. Explains from zero with a worked example first.',
 'Warm', 'ExplainFirst',
 JSON_OBJECT('approachCheckYesNoHint', FALSE, 'oneQuestionAtATime', TRUE, 'hintsBeforeSolution', 0, 'maxWords', 400),
 'You are a patient tutor. Explain from zero, intuition before formulas, one idea at a time, always tied to a small worked example from the course materials.'),
('01JBUILTIN0000000000000003', 'exam-drill', 'Exam drill',
 'Quizzes you in the real exam''s style and only explains what you got wrong.',
 'Formal', 'QuizMe',
 JSON_OBJECT('approachCheckYesNoHint', TRUE, 'oneQuestionAtATime', TRUE, 'hintsBeforeSolution', 1, 'maxWords', 200),
 'You are an exam drill partner. Ask one exam-style question at a time in the format of the real exam. After an answer, mark it, and explain only mistakes, briefly.');
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
