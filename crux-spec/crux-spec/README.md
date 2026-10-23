# Crux: build package

Specs for building Crux, a study and exam-prep platform. Written 2026-10-23.

Read in this order:

1. `backend-spec.md`: architecture (4 services), protocols, domain rules, AI, storage saver, route tables, milestones.
2. `schema/`: the four MySQL 8 databases, which are authoritative for tables and columns. Each file becomes that service's first migration.
   - `schema-v5-core.sql` → `crux_core` (Rust core service)
   - `schema-v5-files.sql` → `crux_files` (Python files service)
   - `schema-v5-ai.sql` → `crux_ai` (Python AI service)
   - `schema-v5-notify.sql` → `crux_notify` (Python notify service)
3. `frontend-spec.md`: React SPA, screens, routes and rules. Section 3 (Design) is intentionally empty and will be filled later.
4. `background/schema-v4-review.md`: why the safeguards exist, and the rules MySQL can't enforce (still valid for v5).
5. `brand/`: logo mark (Crux mark A) for light and dark backgrounds. The dot takes the course colour.

All four schema files load clean on MySQL 8.0.46 (utf8mb4, server time zone +00:00).
