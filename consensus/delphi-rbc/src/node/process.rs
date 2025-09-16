use std::{sync::Arc};

use crypto::hash::{verf_mac};
use types::{appxcon::{WrapperMsg, ProtMsg}};
use crate::node::{
    context::Context
};

/*
    Approximate Consensus proceeds in rounds. Every round has a state of its own.
    Every round is composed of three stages: a) n-parallel reliable broadcast, b) Witness technique,
    and c) Value reduction. The three stages form a round for Approximate Agreement. 

    The RoundState object is designed to handle all three stages. For the reliable broadcast stage, all n nodes
    initiate a reliable broadcast to broadcast their current round values. This stage of the protocol ends 
    when n-f reliable broadcasts are terminated. 

    In the witness technique stage, every node broadcasts the first n-f nodes whose values are reliably accepted 
    by the current node. We call node $i$ a witness to node $j$ if j reliably accepted the first n-f messages 
    reliably accepted by node $i$. Every node stays in this stage until it accepts n-f witnesses. 

    After accepting n-f witnesses, the node updates its value for the next round and repeats the process for 
    a future round. 
*/
impl Context{
    pub fn check_proposal(&self,wrapper_msg: Arc<WrapperMsg>) -> bool {
        // validate MAC
        let byte_val = bincode::serialize(&wrapper_msg.protmsg).expect("Failed to serialize object");
        let sec_key = match self.sec_key_map.get(&wrapper_msg.clone().sender) {
            Some(val) => {val},
            None => {panic!("Secret key not available, this shouldn't happen")},
        };
        if !verf_mac(&byte_val,&sec_key.as_slice(),&wrapper_msg.mac){
            log::warn!("MAC Verification failed.");
            return false;
        }
        true
    }
    
    pub(crate) async fn process_msg(&mut self, wrapper_msg: WrapperMsg){
        log::debug!("Received protocol msg: {:?}",wrapper_msg);
        let msg = Arc::new(wrapper_msg.clone());
        if self.check_proposal(msg){
            match wrapper_msg.clone().protmsg {
                ProtMsg::RBCInit(main_msg,rep)=> {
                    // RBC initialized
                    log::debug!("Received RBC init : {:?}",main_msg);
                    self.process_rbc_init(main_msg.clone(),rep).await;
                },
                ProtMsg::ECHO(main_msg, _orig, sender) =>{
                    // ECHO for main_msg: RBC originated by orig, echo sent by sender
                    self.process_echo(main_msg.clone(),_orig, sender).await;
                },
                ProtMsg::READY(main_msg, _orig, sender) =>{
                    // READY for main_msg: RBC originated by orig, echo sent by sender
                    self.process_ready(main_msg.clone(),_orig, sender).await;
                },
                ProtMsg::BinaryAAEcho(msgs, echo_sender, round) =>{
                    log::debug!("Received Binary AA Echo1 from node {}",echo_sender);
                    self.process_baa_echo(msgs, echo_sender, round).await;
                },
                ProtMsg::BinaryAAEcho2(msgs, echo2_sender, round) =>{
                    log::debug!("Received Binary AA Echo2 from node {}",echo2_sender);
                    self.process_baa_echo2(msgs, echo2_sender, round).await;
                },
                _=>{}
            }
        }
        else {
            log::warn!("MAC Verification failed for message {:?}",wrapper_msg.protmsg);
        }
    }
}