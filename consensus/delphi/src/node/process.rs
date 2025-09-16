use std::{sync::Arc};

use crypto::hash::{verf_mac};
use types::{appxcon::{WrapperMsg, ProtMsg}};
use crate::node::{context::Delphi};


impl Delphi{
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
    
    /**
     * Receives a message, deserializes it, and passes it to the appropriate handling function. 
     */
    pub(crate) async fn process_msg(&mut self, wrapper_msg: WrapperMsg){
        log::debug!("Received protocol msg: {:?}",wrapper_msg);
        let msg = Arc::new(wrapper_msg.clone());
        if self.check_proposal(msg){
            match wrapper_msg.clone().protmsg {
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