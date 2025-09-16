use std::collections::HashSet;

use types::{Replica, appxcon::{ProtMsg, Msg}, SyncMsg, SyncState, Round};

use super::Context;

impl Context {
    pub async fn process_rbc_init(self:&mut Context,main_msg: Msg,rbc_origin:Replica){
        let sender = main_msg.origin;
        log::info!("Received RBC Init from node {} in round {}",main_msg.origin,main_msg.round);
        let rbc_state = &mut self.rbc_state;
        rbc_state.node_msgs.insert(sender, main_msg.clone());
        match rbc_state.echos.get_mut(&sender)  {
            None => {
                let mut hash_set = HashSet::default();
                hash_set.insert(self.myid);
                rbc_state.echos.insert(sender, hash_set);
            },
            Some(x) => {
                x.insert(self.myid);
            },
        }
        match rbc_state.readys.get_mut(&sender)  {
            None => {
                let mut hash_set = HashSet::default();
                hash_set.insert(self.myid);
                rbc_state.readys.insert(sender, hash_set);
            },
            Some(x) => {
                x.insert(self.myid);
            },
        }
        log::debug!("Sending echos for RBC from origin {}",main_msg.origin);
        self.broadcast(ProtMsg::ECHO(main_msg.clone(), main_msg.origin, self.myid)).await;
        self.process_echo(main_msg.clone(), rbc_origin,self.myid).await;
    }

    pub async fn process_echo(&mut self, main_msg:Msg,rbc_origin:Replica, echo_sender:Replica){
        let rbc_originator = rbc_origin;
        log::info!("Received ECHO message {:?}",main_msg.clone());
        //if round_state_map.contains_key(&main_msg.round){
        // 1. Add echos to the round state object
        let rbc_state = &mut self.rbc_state;
        // If RBC already terminated, do not consider this RBC
        if rbc_state.terminated_rbcs.contains(&rbc_originator){
            return;
        }
        match rbc_state.echos.get_mut(&rbc_originator) {
            None => {
                let mut echoset = HashSet::default();
                echoset.insert(echo_sender);
                rbc_state.echos.insert(rbc_originator, echoset);
            },
            Some(x) => {
                x.insert(echo_sender);
            }
        }
        let echos = rbc_state.echos.get_mut(&rbc_originator).unwrap();
        // 2. Check if echos reached the threshold, init already received, and round number is matching
        log::debug!("ECHO check: CurrRound: {},ReqRound: {}, echos.len {}, contains key: {}"
        ,self.round,main_msg.value,echos.len(),rbc_state.node_msgs.contains_key(&rbc_originator));
        if echos.len() == self.num_nodes-self.num_faults && 
            rbc_state.node_msgs.contains_key(&rbc_originator){
            // Broadcast readys, otherwise, just wait longer
            self.broadcast(ProtMsg::READY(main_msg.clone(),main_msg.origin, self.myid)).await;
            //msgs_to_be_sent.push(ProtMsg::READY(main_msg.clone(),main_msg.origin, self.myid));
            self.process_ready(main_msg,rbc_originator, self.myid).await;
        }
    }

    pub async fn process_ready(&mut self, msg:Msg, rbc_origin:Replica, ready_sender:Replica){
        let rbc_state = &mut self.rbc_state;
        log::info!("Received READY message from {} for RBC from {}",ready_sender,rbc_origin);
        if rbc_state.terminated_rbcs.contains(&rbc_origin){
            return;
        }
        match rbc_state.readys.get_mut(&rbc_origin) {
            None => {
                let mut readyset = HashSet::default();
                readyset.insert(ready_sender);
                rbc_state.readys.insert(rbc_origin, readyset);
            },
            Some(x) => {
                x.insert(ready_sender);
            }
        }
        let readys = rbc_state.readys.get_mut(&rbc_origin).unwrap();
        // 2. Check if readys reached the threshold, init already received, and round number is matching
        log::debug!("READY check: Current Round {}, reqRound: {}, readys.len {}, contains key: {}"
        ,self.round,msg.value, readys.len(),rbc_state.node_msgs.contains_key(&rbc_origin));
        if  readys.len() == self.num_faults+1 &&
            rbc_state.node_msgs.contains_key(&rbc_origin){
            // Broadcast readys, otherwise, just wait longer
            self.broadcast(ProtMsg::READY(msg.clone(),rbc_origin, self.myid)).await;
            //self.process_ready(main_msg.clone(), self.myid).await;
            //msgs_to_be_sent.push(ProtMsg::READY(main_msg.clone(),main_msg.origin, self.myid));
        }
        else if readys.len() >= self.num_nodes-self.num_faults &&
            rbc_state.node_msgs.contains_key(&rbc_origin){
            // Terminate RBC, RAccept the value
            // Add value to value list, add rbc to rbc list
            log::info!("Terminated RBC of node {} with value {}",rbc_origin,msg.value);
            rbc_state.terminated_rbcs.insert(rbc_origin);
            rbc_state.accepted_vals.push(msg.value);
            if rbc_state.terminated_rbcs.len() >= self.num_nodes - self.num_faults && !self.terminated{
                let max_round = *rbc_state.accepted_vals.iter().max().unwrap();
                if max_round <= self.round as u64{
                    // Terminate immediately, the state is ready for aggregation
                    let final_val = self.aggregate(max_round as Round);
                    let cancel_handler = self.sync_send.send(0, SyncMsg { 
                        sender: self.myid, 
                        state: SyncState::CompletedSharing, 
                        value: final_val 
                    }).await;
                    self.add_cancel_handler(cancel_handler);
                    self.terminated = true;
                }
            }
        }
    }
}