-- Why a session was revoked, so a refresh token reused seconds after an ordinary rotation (two tabs
-- refreshing at once) gets a grace window, while one revoked by logout, reset or theft detection
-- never does (Nick 2026-10-23: 10 s grace; OPEN-QUESTIONS.md Q-auth-6). NULL = still active.
ALTER TABLE UserSession
    ADD COLUMN RevokeReason ENUM('Rotated','Logout','Reuse','PasswordReset','Admin') NULL AFTER RevokedAt;
