-- Milestone 8: the AI course import is applied by core in one transaction, idempotent on the
-- import ref (spec 7.5). A second apply of the same import returns the stored result.
CREATE TABLE ImportApplication (
    ImportRef CHAR(26) NOT NULL PRIMARY KEY,
    UserId    BIGINT   NOT NULL,
    CourseId  BIGINT   NULL,
    Result    JSON     NOT NULL,
    AppliedAt DATETIME NOT NULL,
    FOREIGN KEY (UserId)   REFERENCES Users(Id)  ON DELETE CASCADE,
    FOREIGN KEY (CourseId) REFERENCES Course(Id) ON DELETE SET NULL
);
