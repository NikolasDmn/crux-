//! Consumes files' storage-saver events (spec 6.2) into core's materials.

use async_trait::async_trait;
use crux_clients::bus::Delivery;
use crux_domain::topics;
use crux_repo::MySqlConnection;

use crate::consumer::Handler;

pub struct FileEvents;

#[async_trait]
impl Handler for FileEvents {
    fn topics(&self) -> Vec<&'static str> {
        vec![topics::FILE_PURGE_SCHEDULED, topics::FILE_PURGED, topics::FILE_RESTORED]
    }

    async fn handle(&self, conn: &mut MySqlConnection, d: &Delivery) -> anyhow::Result<()> {
        crux_service::saver::on_file_event(conn, &d.topic, &d.envelope.payload).await
    }
}
