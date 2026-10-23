-- Invites are addressed by the API (DELETE /invites/{id}, /admin/invites), and only PublicIds
-- leave the service (spec 3.1). schema-v5-core.sql has no PublicId on Invite; flagged in
-- OPEN-QUESTIONS.md. Nullable for rows that existed before this migration (none in practice).
ALTER TABLE Invite
    ADD COLUMN PublicId CHAR(26) NULL AFTER Id,
    ADD UNIQUE KEY uq_invite_public (PublicId);
