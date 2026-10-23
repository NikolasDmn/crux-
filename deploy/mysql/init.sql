-- One MySQL server, one database per service (backend-spec 1). Each service gets a user that can
-- only touch its own database, so "files never writes core tables" (spec 9.3) is enforced by grants.
-- Dev passwords; production passwords come from .env via deploy/mysql/init-prod.sh (later).

CREATE DATABASE IF NOT EXISTS crux_core   CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;
CREATE DATABASE IF NOT EXISTS crux_files  CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;
CREATE DATABASE IF NOT EXISTS crux_ai     CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;
CREATE DATABASE IF NOT EXISTS crux_notify CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE USER IF NOT EXISTS 'crux_core'@'%'   IDENTIFIED BY 'crux-core';
CREATE USER IF NOT EXISTS 'crux_files'@'%'  IDENTIFIED BY 'crux-files';
CREATE USER IF NOT EXISTS 'crux_ai'@'%'     IDENTIFIED BY 'crux-ai';
CREATE USER IF NOT EXISTS 'crux_notify'@'%' IDENTIFIED BY 'crux-notify';

GRANT ALL PRIVILEGES ON crux_core.*   TO 'crux_core'@'%';
GRANT ALL PRIVILEGES ON crux_files.*  TO 'crux_files'@'%';
GRANT ALL PRIVILEGES ON crux_ai.*     TO 'crux_ai'@'%';
GRANT ALL PRIVILEGES ON crux_notify.* TO 'crux_notify'@'%';

-- Integration tests create throwaway databases named crux_test_<service>_<random>.
CREATE USER IF NOT EXISTS 'crux_test'@'%' IDENTIFIED BY 'crux-test';
GRANT ALL PRIVILEGES ON `crux\_test\_%`.* TO 'crux_test'@'%';
FLUSH PRIVILEGES;
