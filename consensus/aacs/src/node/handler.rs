
use async_trait::async_trait;
use futures_util::SinkExt;
use network::{Acknowledgement};
use tokio::sync::mpsc::UnboundedSender;
use types::dkg::msg::ACSWrapperMsg;

#[derive(Debug, Clone)]
pub struct Handler {
    consensus_tx: UnboundedSender<ACSWrapperMsg>,
}

impl Handler {
    pub fn new(consensus_tx: UnboundedSender<ACSWrapperMsg>) -> Self {
        Self { consensus_tx }
    }
}

#[async_trait]
impl network::Handler<Acknowledgement, ACSWrapperMsg>
for Handler
{
    async fn dispatch(
        &self,
        msg: ACSWrapperMsg,
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