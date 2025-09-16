use std::collections::HashMap;
use async_trait::async_trait;
use futures_util::SinkExt;
use network::{Acknowledgement};
use tokio::sync::mpsc::UnboundedSender;
use types::SyncMsg;

#[derive(Debug, Clone)]
pub struct SyncHandler {
    consensus_senders: HashMap<usize, UnboundedSender<SyncMsg>>,
}

impl SyncHandler {
    pub fn new(consensus_senders: HashMap<usize, UnboundedSender<SyncMsg>>) -> Self {
        Self { consensus_senders }
    }
}

#[async_trait::async_trait]
impl network::Handler<Acknowledgement, SyncMsg> for SyncHandler {
    async fn dispatch(&self, msg: SyncMsg, writer: &mut network::Writer<Acknowledgement>) {
        if let Some(tx) = self.consensus_senders.get(&msg.inst_id) {
            if let Err(e) = tx.send(msg.clone()) {
                log::error!("Failed to send sync message to consensus channel (inst_id: {}): {}", msg.inst_id, e);
            } else {
                log::debug!("Forwarded sync message to consensus channel (inst_id: {})", msg.inst_id);
            }
        } else {
            log::error!("No sender found for inst_id: {}", msg.inst_id);
        }

        writer
            .send(Acknowledgement::Pong)
            .await
            .expect("Failed to send an acknowledgement");
    }
}