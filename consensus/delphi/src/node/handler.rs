use std::collections::HashMap;
use async_trait::async_trait;
use futures_util::SinkExt;
use network::{Acknowledgement};
use tokio::sync::mpsc::UnboundedSender;
use types::appxcon::WrapperMsg;

#[derive(Debug, Clone)]
pub struct Handler {
    consensus_senders: HashMap<usize, UnboundedSender<WrapperMsg>>,
}

impl Handler {
    pub fn new(consensus_senders: HashMap<usize, UnboundedSender<WrapperMsg>>) -> Self {
        Self { consensus_senders }
    }
}

#[async_trait]
impl network::Handler<Acknowledgement, WrapperMsg> for Handler {
    async fn dispatch(&self, msg: WrapperMsg, writer: &mut network::Writer<Acknowledgement>) {
        if let Some(tx) = self.consensus_senders.get(&msg.inst_id) {
            if let Err(e) = tx.send(msg.clone()) {
                log::error!("Failed to send message to consensus channel (inst_id: {}): {}", msg.inst_id, e);
            }
        } 

        writer
            .send(Acknowledgement::Pong)
            .await
            .expect("Failed to send an acknowledgement");
    }
}