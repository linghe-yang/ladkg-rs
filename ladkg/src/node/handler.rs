
use async_trait::async_trait;
use futures_util::SinkExt;
use network::{Acknowledgement};
use tokio::sync::mpsc::UnboundedSender;
use types::dkg::msg::VSSWrapperMsg;

#[derive(Debug, Clone)]
pub struct Handler {
    consensus_tx: UnboundedSender<VSSWrapperMsg>,
}

impl Handler {
    pub fn new(consensus_tx: UnboundedSender<VSSWrapperMsg>) -> Self {
        Self { consensus_tx }
    }
}

#[async_trait]
impl network::Handler<Acknowledgement, VSSWrapperMsg>
for Handler
{
    async fn dispatch(
        &self,
        msg: VSSWrapperMsg,
        writer: &mut network::Writer<Acknowledgement>,
    ) {
        // Forward the message
        self.consensus_tx
            .send(msg)
            .expect("Failed to send message to the consensus channel");

        // Acknowledge
        writer
            .send(Acknowledgement::Pong)
            .await
            .expect("Failed to send an acknowledgement");
    }
}