//! What core consumes from the other services: files' storage-saver events (spec 6.2) and the
//! `user.purged` acks of an account deletion (spec 9.4).

use async_trait::async_trait;
use crux_clients::bus::Delivery;
use crux_domain::topics;
use crux_repo::MySqlConnection;

use crate::consumer::Handler;

pub struct FileEvents;

#[async_trait]
impl Handler for FileEvents {
    fn topics(&self) -> Vec<&'static str> {
        vec![topics::FILE_PURGE_SCHEDULED, topics::FILE_PURGED, topics::FILE_RESTORED, topics::USER_PURGED]
    }

    async fn handle(&self, conn: &mut MySqlConnection, d: &Delivery) -> anyhow::Result<()> {
        if d.topic == topics::USER_PURGED {
            return crux_service::account::on_user_purged(conn, &d.envelope.payload).await;
        }
        crux_service::saver::on_file_event(conn, &d.topic, &d.envelope.payload).await
    }
}
