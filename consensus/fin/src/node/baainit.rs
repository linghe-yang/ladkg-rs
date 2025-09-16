use std::collections::HashMap;

use crypto_blstrs::{crypto::threshold_sig::{SecretKey, PublicKey}, threshold_sig::{PartialBlstrsSignature}};
use types::{Replica, appxcon::{ProtMsg}, Round, Val, SyncState, SyncMsg};

use crate::node::{Context, RoundStateBin};

/**
 * We use Abraham, Ben-David, and Yandamuri's Binary Byzantine Agreement protocol as the BBA protocol in FIN. 
 * FIN uses a RABA protocol with a higher round complexity. We replace this RABA protocol with Gather and Abraham, Ben-David, and Yandamuri's BBA protocol to achieve BBA within 5 rounds in the best case.
 * Overall, this protocol has a lesser round complexity than FIN and essentially terminates faster because of Gather's higher probability of encountering a stable proposal.  
 * Refer to both protocols for a detailed protocol description. 
 */
impl Context{
    #[async_recursion::async_recursion]
    pub async fn process_baa_echo(self: &mut Context, msg:Val, echo_sender:Replica, leader_round:Round,baa_round:Round){
        if self.leader_round>leader_round {
            return;
        }
        if self.baa_round>baa_round{
            return;
        }
        log::info!("Received ECHO1 message from node {} with content {:?} for leader round {}, baa round {}",echo_sender,msg,leader_round,baa_round);
        let val = msg.clone();
        let mut terminate = None;
        // To avoid mutable borrow
        let mut msgs_to_send = Vec::new();
        if self.round_state.contains_key(&leader_round){
            let baa_rnd_state = self.round_state.get_mut(&leader_round).unwrap();
            if baa_rnd_state.contains_key(&baa_round){
                let round_state = baa_rnd_state.get_mut(&baa_round).unwrap();
                let (echo1,echo2,echo3) = round_state.add_echo(val,  echo_sender,self.num_faults+1,self.num_nodes-self.num_faults);
                if echo1.is_some(){
                    msgs_to_send.push(ProtMsg::FinBinAAEcho(echo1.unwrap(), self.myid,leader_round, baa_round));
                    let (_e1,e2,e3) = round_state.add_echo(echo1.unwrap(), self.myid, self.num_faults+1,self.num_nodes-self.num_faults);
                    if e2.is_some(){
                        log::info!("Sending echo2 message {} for lround {},bround {}",e2.unwrap(),leader_round,baa_round);
                        msgs_to_send.push(ProtMsg::FinBinAAEcho2(e2.unwrap(), self.myid, leader_round,baa_round));    
                    }
                    if e3.is_some(){
                        log::info!("Sending echo3 message {} for lround {},bround {}",e3.unwrap(),leader_round,baa_round);
                        msgs_to_send.push(ProtMsg::FinBinAAEcho3(e3.unwrap(), self.myid, leader_round,baa_round));
                    }
                }
                if echo2.is_some(){
                    msgs_to_send.push(ProtMsg::FinBinAAEcho2(echo2.unwrap(), self.myid, leader_round,baa_round));
                    let echo3 = round_state.add_echo2(echo2.unwrap(), self.myid, self.num_nodes-self.num_faults);
                    if echo3.is_some(){
                        msgs_to_send.push(ProtMsg::FinBinAAEcho3(echo3.unwrap(), self.myid, leader_round,baa_round));
                    }
                }
                if echo3.is_some(){
                    msgs_to_send.push(ProtMsg::FinBinAAEcho3(echo3.unwrap(), self.myid, leader_round,baa_round));
                    let term = round_state.add_echo3(echo3.unwrap(), self.myid, self.num_nodes-self.num_faults);
                    if term && !round_state.contains_sig(self.myid){
                        // Create partial signature and broadcast
                        let mut beacon_msg = self.sign_msg.clone();
                        beacon_msg.push_str("baa");
                        beacon_msg.push_str(leader_round.to_string().as_str());
                        beacon_msg.push_str(baa_round.to_string().as_str());
                        let dst = "Test";
                        let psig = self.secret_key.sign(&beacon_msg, &dst);
                        round_state.add_partial_sig(self.myid, psig.clone());
                        terminate = round_state.aggregate_psigs(self.num_faults+1);
                        let sig_data = bincode::serialize(&psig).expect("Serialization error");
                        let coin_flip_msg = ProtMsg::BBACoin(leader_round,baa_round, sig_data, self.myid);
                        msgs_to_send.push(coin_flip_msg);
                    }
                }
            }
            else {
                let round_state = RoundStateBin::new_with_echo(msg, echo_sender);
                baa_rnd_state.insert(baa_round, round_state);                
            }
        }
        else {
            let round_state = RoundStateBin::new_with_echo(msg, echo_sender);
            let mut baa_rnd_state = HashMap::default();
            baa_rnd_state.insert(baa_round, round_state);
            self.round_state.insert(leader_round, baa_rnd_state);

        }
        for msg in msgs_to_send{
            self.broadcast(msg).await;
        }
        if terminate.is_some(){
            self.start_baa(leader_round,baa_round+1, terminate.unwrap().1, terminate.unwrap().0).await;
        }
    }

    pub async fn process_baa_echo2(self: &mut Context, msg: Val, echo2_sender:Replica, leader_round:Round,baa_round:Round){
        if self.leader_round>leader_round {
            return;
        }
        if self.baa_round>baa_round{
            return;
        }
        let mut terminate = None;
        let mut msgs_to_send = Vec::new();
        log::info!("Received ECHO2 message from node {} with content {:?} for lround {}, bround {}",echo2_sender,msg,leader_round,baa_round);
        if self.round_state.contains_key(&leader_round){
            let baa_rnd_state = self.round_state.get_mut(&leader_round).unwrap();
            if baa_rnd_state.contains_key(&baa_round){
                let round_state = baa_rnd_state.get_mut(&baa_round).unwrap();
                let echo3 = round_state.add_echo2(msg,echo2_sender,self.num_nodes-self.num_faults);
                if echo3.is_some(){
                    let term = round_state.add_echo3(echo3.unwrap(), self.myid, self.num_nodes-self.num_faults);
                    msgs_to_send.push(ProtMsg::FinBinAAEcho3(echo3.unwrap(), self.myid, leader_round,baa_round));
                    log::info!("Sending echo3 message {} for lround {}, bround {}",echo3.unwrap(),leader_round,baa_round);
                    if term{
                        // Create partial signature and broadcast
                        let mut beacon_msg = self.sign_msg.clone();
                        beacon_msg.push_str("baa");
                        beacon_msg.push_str(leader_round.to_string().as_str());
                        beacon_msg.push_str(baa_round.to_string().as_str());
                        let dst = "Test";
                        let psig = self.secret_key.sign(&beacon_msg, &dst);
                        round_state.add_partial_sig(self.myid, psig.clone());
                        terminate = round_state.aggregate_psigs(self.num_faults+1);
                        let sig_data = bincode::serialize(&psig).expect("Serialization error");
                        let coin_flip_msg = ProtMsg::BBACoin(leader_round,baa_round, sig_data, self.myid);
                        log::info!("Sending partial signature for lround {}, bround {}",leader_round,baa_round);
                        msgs_to_send.push(coin_flip_msg);
                    }
                }
            }
            else {
                let round_state = RoundStateBin::new_with_echo2(msg, echo2_sender);
                baa_rnd_state.insert(baa_round, round_state);                
            }
        }
        else {
            let round_state = RoundStateBin::new_with_echo2(msg, echo2_sender);
            let mut baa_rnd_state = HashMap::default();
            baa_rnd_state.insert(baa_round, round_state);
            self.round_state.insert(leader_round, baa_rnd_state);
        }
        for msg in msgs_to_send{
            self.broadcast(msg).await;
        }
        if terminate.is_some(){
            self.start_baa(leader_round,baa_round+1, terminate.unwrap().1, terminate.unwrap().0).await;
        }
    }

    pub async fn process_baa_echo3(self: &mut Context, msg: Val, echo3_sender:Replica, leader_round:Round,baa_round:Round){
        if self.leader_round>leader_round {
            return;
        }
        if self.baa_round>baa_round{
            return;
        }
        let mut terminate = None;
        log::info!("Received ECHO3 message from node {} with content {:?} for lround {}, bround {}",echo3_sender,msg,leader_round,baa_round);
        if self.round_state.contains_key(&leader_round){
            let baa_rnd_state = self.round_state.get_mut(&leader_round).unwrap();
            if baa_rnd_state.contains_key(&baa_round){
                let round_state = baa_rnd_state.get_mut(&baa_round).unwrap();
                // term variable signifies whether coin is ready for broadcasting
                let term = round_state.add_echo3(msg,echo3_sender,self.num_nodes-self.num_faults);
                if term{
                    let mut beacon_msg = self.sign_msg.clone();
                    beacon_msg.push_str("baa");
                    beacon_msg.push_str(leader_round.to_string().as_str());
                    beacon_msg.push_str(baa_round.to_string().as_str());
                    let dst = "Test";
                    let psig = self.secret_key.sign(&beacon_msg, &dst);
                    round_state.add_partial_sig(self.myid, psig.clone());
                    terminate = round_state.aggregate_psigs(self.num_faults+1);
                    let sig_data = bincode::serialize(&psig).expect("Serialization error");
                    let coin_flip_msg = ProtMsg::BBACoin(leader_round,baa_round, sig_data, self.myid);
                    log::info!("Broadcasting partial signature for lround {}, bround {}",leader_round,baa_round);
                    self.broadcast(coin_flip_msg).await;
                }
            }
            else {
                let round_state = RoundStateBin::new_with_echo3(msg, echo3_sender);
                baa_rnd_state.insert(baa_round, round_state);                
            }
        }
        else {
            let round_state = RoundStateBin::new_with_echo3(msg, echo3_sender);
            let mut baa_rnd_state = HashMap::default();
            baa_rnd_state.insert(baa_round, round_state);
            self.round_state.insert(leader_round, baa_rnd_state);
        }
        if terminate.is_some(){
            self.start_baa(leader_round,baa_round+1, terminate.unwrap().1, terminate.unwrap().0).await;
        }
    }

    pub async fn process_partial_sig(self:&mut Context, psig: Vec<u8>,sig_sender:Replica,leader_round:Round,baa_round:Round){
        if self.leader_round>leader_round {
            return;
        }
        if self.baa_round>baa_round{
            return;
        }
        log::info!("Received partial signature message from node {} with lround {}, bround: {}",sig_sender,leader_round,baa_round);
        let psig_deser:PartialBlstrsSignature = bincode::deserialize(psig.as_slice()).expect("Deserialization error");        
        let mut terminate = None;
        if self.round_state.contains_key(&leader_round){
            let baa_rnd_state = self.round_state.get_mut(&leader_round).unwrap();
            if baa_rnd_state.contains_key(&baa_round){
                let rnd_state = baa_rnd_state.get_mut(&baa_round).unwrap();
                if rnd_state.partial_sig_vec.len() < self.num_faults +1{
                    let pkey = self.tpubkey_share.get(&(sig_sender+1)).unwrap();
                    let mut beacon_msg = self.sign_msg.clone();
                    beacon_msg.push_str("baa");
                    beacon_msg.push_str(leader_round.to_string().as_str());
                    beacon_msg.push_str(baa_round.to_string().as_str());
                    let dst = "Test";
                    if pkey.verify(&psig_deser, &beacon_msg, &dst){
                        log::info!("Signature verification successful, adding sig to map");
                        rnd_state.add_partial_sig(sig_sender, psig_deser);
                        terminate = rnd_state.aggregate_psigs(self.num_faults+1);
                    }
                    else {
                        log::error!("Signature verification unsuccessful");
                        return;
                    }
                }
            }
            else {
                let rnd_state = RoundStateBin::new_with_psig( psig_deser,sig_sender);
                baa_rnd_state.insert(baa_round, rnd_state);
            }
        }
        else {
            let rnd_state = RoundStateBin::new_with_psig( psig_deser,sig_sender);
            let mut baa_rnd_state = HashMap::default();
            baa_rnd_state.insert(baa_round, rnd_state);
            self.round_state.insert(leader_round, baa_rnd_state);
        }
        if terminate.is_some(){
            self.start_baa(leader_round,baa_round+1, terminate.unwrap().1, terminate.unwrap().0).await;
        }
    }

    #[async_recursion::async_recursion]
    pub async fn start_baa(self: &mut Context,leader_round:Round, baa_round:Round, term_val: Val, terminate: bool){
        if self.terminated{
            return;
        }
        if !terminate{
            log::info!("Received request to start new round lround {} bround {}",leader_round,baa_round);
            // Restart next round with updated value
            self.process_baa_echo(term_val, self.myid, leader_round,baa_round).await;
            // Broadcast Binary AA echo and start round round.
            self.broadcast(ProtMsg::FinBinAAEcho(term_val, self.myid, leader_round,baa_round)).await;
            self.baa_round = baa_round;
        }
        else {
            // Find target proposal that was elected
            if term_val == 2 && !self.terminated{
                if self.leader_election_state.contains_key(&(leader_round)){
                    let leader_node = self.leader_election_state.get(&(leader_round)).unwrap().2.unwrap();
                    let broadcasted_indices = self.mvba_state.node_msgs.get(&leader_node).unwrap().message.clone();
                    // Take median of indices
                    let mut broadcasted_values = Vec::new();
                    for replica in broadcasted_indices.into_iter(){
                        broadcasted_values.push(self.rbc_state.node_msgs.get(&replica).unwrap().value.clone());
                    }
                    broadcasted_values.sort();
                    let output = *broadcasted_values.get(self.num_faults+1).unwrap();
                    // Send it to sync handler
                    log::info!("Terminated BA with value {}",output);
                    let cancel_handler = self.sync_send.send(0, SyncMsg { 
                        sender: self.myid, 
                        state: SyncState::CompletedSharing, 
                        value: output as Val
                    }).await;
                    self.add_cancel_handler(cancel_handler);
                    self.terminated = true;
                }    
            }
            else {
                log::info!("Reelecting leader for lround {}",leader_round+1);
                // Reelect node and restart Binary AA
                self.elect_leader(leader_round+1).await;
                self.leader_round = leader_round+1;
            }
        }
    }
}