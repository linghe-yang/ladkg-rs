use std::{sync::Arc};

use crypto::hash::verf_mac;
use types::{appxcon::{ProtMsg, WrapperMsg}};

use super::{Context};

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
    /**
     * Message deserialization happens here. Message is deserialized and passed to the appropriate handling function. 
     */
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
                ProtMsg::LeaderCoin(round, index, signature, sender) =>{
                    self.handle_incoming_leader_coin(round, index, signature, sender).await;
                },
                ProtMsg::FinBinAAEcho(val, echo_sender, leader_round,baa_round) =>{
                    self.process_baa_echo(val, echo_sender, leader_round,baa_round).await;
                },
                ProtMsg::FinBinAAEcho2(val, echo_sender, leader_round,baa_round) =>{
                    self.process_baa_echo2(val, echo_sender, leader_round,baa_round).await;
                },
                ProtMsg::FinBinAAEcho3(val, echo_sender, leader_round,baa_round) =>{
                    self.process_baa_echo3(val, echo_sender, leader_round,baa_round).await;
                },
                ProtMsg::BBACoin(leader_round,baa_round, signature, sender) =>{
                    self.process_partial_sig(signature, sender, leader_round,baa_round).await;
                },
                // ProtMsg::BinaryAAEcho(msgs, echo_sender, round) =>{
                //     log::debug!("Received Binary AA Echo1 from node {}",echo_sender);
                //     self.process_baa_echo(msgs, echo_sender, round).await;
                // },
                // ProtMsg::BinaryAAEcho2(msgs, echo2_sender, round) =>{
                //     log::debug!("Received Binary AA Echo2 from node {}",echo2_sender);
                //     self.process_baa_echo2(msgs, echo2_sender, round).await;
                // },
                _=>{}
            }
        }
        else {
            log::warn!("MAC Verification failed for message {:?}",wrapper_msg.protmsg);
        }
    }

    // async fn empty_queue_and_proceed(&mut self, round:Round){
    //     let glow_bls_state = self.state.get_mut(&round).unwrap();
    //     let msg_queue = glow_bls_state.message_queue();
    //     let mut broadcast_msgs = Vec::new();
    //     while !msg_queue.is_empty(){
    //         broadcast_msgs.push(msg_queue.pop().unwrap());
    //         //self.broadcast(msg.body, round).await;
    //     }
    //     if glow_bls_state.wants_to_proceed(){
    //         glow_bls_state.proceed().unwrap();
    //     }
    //     // Send outgoing messages
    //     let msg_queue = glow_bls_state.message_queue();
    //     while !msg_queue.is_empty(){
    //         broadcast_msgs.push(msg_queue.pop().unwrap());
    //         //let msg = msg_queue.pop().unwrap();
    //     }
    //     if glow_bls_state.is_finished(){
    //         let result = glow_bls_state.pick_output().unwrap().unwrap();
    //         log::info!("Result obtained, the following is the signature: {:?}",result.1.to_bytes(false));
    //         let cancel_handler = self.sync_send.send(0, SyncMsg { sender: self.myid as usize, state: SyncState::BeaconRecon(round, self.myid as usize, round as usize, result.1.to_bytes(false)), value:0}).await;
    //         self.add_cancel_handler(cancel_handler);
    //         self.curr_round = round+1;
    //         self.start_round(round+1).await;
    //     }
    //     for msg in broadcast_msgs.into_iter(){
    //         self.broadcast(msg.body, round).await;
    //     }
    // }
}