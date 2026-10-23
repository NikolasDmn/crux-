-- Milestone 2: catalog + sharing additions (OPEN-QUESTIONS.md D2-1..D2-5, S-2 style PublicIds).

-- D2-2: a share is a pending invitation until the invitee accepts. Pending rows grant no access.
ALTER TABLE CourseMember
    ADD COLUMN Status      ENUM('Pending','Accepted') NOT NULL DEFAULT 'Accepted' AFTER Role,
    ADD COLUMN RespondedAt DATETIME NULL AFTER AddedAt;

-- D2-1/D2-3: sharing with an email that has no account yet. Tied to the Invite that was emailed;
-- signing up through it adds the member as Accepted.
CREATE TABLE CourseShareInvite (
    Id          BIGINT AUTO_INCREMENT PRIMARY KEY,
    PublicId    CHAR(26)     NOT NULL,
    CourseId    BIGINT       NOT NULL,
    Email       VARCHAR(255) NOT NULL,
    Role        ENUM('Editor','Viewer') NOT NULL,
    InviteId    BIGINT       NOT NULL,
    InvitedBy   BIGINT       NULL,
    CreatedAt   DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    AcceptedAt  DATETIME     NULL,
    RevokedAt   DATETIME     NULL,
    UNIQUE KEY uq_share_invite_public (PublicId),
    KEY idx_share_invite_course (CourseId),
    KEY idx_share_invite_invite (InviteId),
    FOREIGN KEY (CourseId)  REFERENCES Course(Id) ON DELETE CASCADE,
    FOREIGN KEY (InviteId)  REFERENCES Invite(Id) ON DELETE CASCADE,
    FOREIGN KEY (InvitedBy) REFERENCES Users(Id)  ON DELETE SET NULL
);

-- The API addresses papers and concepts (spec 15 routes, question links in milestone 3); only
-- PublicIds leave the service. Papers are edited, so they get a RowVersion too.
ALTER TABLE Paper
    ADD COLUMN PublicId   CHAR(26) NULL AFTER Id,
    ADD COLUMN RowVersion INT NOT NULL DEFAULT 0,
    ADD UNIQUE KEY uq_paper_public (PublicId);

ALTER TABLE Concept
    ADD COLUMN PublicId CHAR(26) NULL AFTER Id,
    ADD UNIQUE KEY uq_concept_public (PublicId);

ALTER TABLE GradeScale
    ADD COLUMN PublicId CHAR(26) NULL AFTER Id,
    ADD UNIQUE KEY uq_gradescale_public (PublicId);
-- Built-ins get fixed ids (loose PublicId format, like the built-in tutors; S-4).
UPDATE GradeScale SET PublicId = '01GRADESCALE00000000NL1010' WHERE OwnerUserId IS NULL AND Name = 'NL 1-10';
UPDATE GradeScale SET PublicId = '01GRADESCALE0000000PERCENT' WHERE OwnerUserId IS NULL AND Name = 'Percent';
UPDATE GradeScale SET PublicId = '01GRADESCALE000000PASSFAIL' WHERE OwnerUserId IS NULL AND Name = 'Pass/Fail';
