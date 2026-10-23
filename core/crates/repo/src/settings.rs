use chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::MySqlConnection;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, sqlx::FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct SettingsRow {
    pub flash_retention: f64,
    pub flash_new_per_day: i32,
    pub flash_review_limit: i32,
    pub mock_go_easy: bool,
    pub mock_real_layout: bool,
    pub extra: Option<Json<serde_json::Value>>,
    pub updated_at: DateTime<Utc>,
    pub row_version: i32,
}

pub async fn insert_defaults(conn: &mut MySqlConnection, user_id: i64) -> RepoResult<()> {
    sqlx::query("INSERT INTO UserSettings (UserId) VALUES (?)").bind(user_id).execute(conn).await?;
    Ok(())
}

/// `FlashRetention` is DECIMAL(3,2); cast to DOUBLE so it decodes as f64.
pub async fn get(conn: &mut MySqlConnection, user_id: i64) -> RepoResult<Option<SettingsRow>> {
    Ok(sqlx::query_as(
        "SELECT CAST(FlashRetention AS DOUBLE) AS FlashRetention, FlashNewPerDay, FlashReviewLimit, MockGoEasy, \
         MockRealLayout, Extra, UpdatedAt, RowVersion FROM UserSettings WHERE UserId = ?",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn update(conn: &mut MySqlConnection, user_id: i64, expected_version: i32, s: &SettingsRow) -> RepoResult<()> {
    let res = sqlx::query(
        "UPDATE UserSettings SET FlashRetention = ?, FlashNewPerDay = ?, FlashReviewLimit = ?, MockGoEasy = ?, \
         MockRealLayout = ?, Extra = ?, RowVersion = RowVersion + 1 WHERE UserId = ? AND RowVersion = ?",
    )
    .bind(s.flash_retention)
    .bind(s.flash_new_per_day)
    .bind(s.flash_review_limit)
    .bind(s.mock_go_easy)
    .bind(s.mock_real_layout)
    .bind(&s.extra)
    .bind(user_id)
    .bind(expected_version)
    .execute(conn)
    .await?;
    if res.rows_affected() == 0 {
        return Err(RepoError::VersionConflict);
    }
    Ok(())
}
