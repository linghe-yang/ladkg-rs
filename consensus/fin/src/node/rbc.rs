use std::collections::HashSet;

use types::{Replica, appxcon::{ProtMsg, Msg}};

use super::Context;
/**
 * This file implements Bracha's Reliable Broadcast (RBC) protocol. 
 */
impl Context {
    /**
     * Start RBC block. 
     */
    pub async fn start_rbc(&mut self,msg:Msg){
        // Broadcast message
        log::info!("Started RBC by broadcasting message {:?}",msg);
        self.broadcast(ProtMsg::RBCInit(msg.clone(), self.myid)).await;
        self.process_rbc_init(msg, self.myid).await;
    }

    #[async_recursion::async_recursion]
    pub async fn process_rbc_init(self:&mut Context,main_msg: Msg,rbc_origin:Replica){
        let sender = main_msg.origin;
        log::info!("Received RBC Init from node {} in round {}",main_msg.origin,main_msg.round);
        let rbc_state;
        if main_msg.rnd_estm{
            rbc_state = &mut self.mvba_state;
        }
        else {
            rbc_state = &mut self.rbc_state;
        }
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

    #[async_recursion::async_recursion]
    pub async fn process_echo(&mut self, main_msg:Msg,rbc_origin:Replica, echo_sender:Replica){
        let rbc_originator = rbc_origin;
        log::info!("Received ECHO message {:?}",main_msg.clone());
        //if round_state_map.contains_key(&main_msg.round){
        // 1. Add echos to the round state object
        let rbc_state;
        if main_msg.rnd_estm{
            rbc_state = &mut self.mvba_state;
        }
        else {
            rbc_state = &mut self.rbc_state;
        }
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
        // log::debug!("ECHO check: CurrRound: {},ReqRound: {}, echos.len {}, contains key: {}"
        // ,self.round,main_msg.value,echos.len(),rbc_state.node_msgs.contains_key(&rbc_originator));
        if echos.len() == self.num_nodes-self.num_faults && 
            rbc_state.node_msgs.contains_key(&rbc_originator){
            // Broadcast readys, otherwise, just wait longer
            self.broadcast(ProtMsg::READY(main_msg.clone(),main_msg.origin, self.myid)).await;
            //msgs_to_be_sent.push(ProtMsg::READY(main_msg.clone(),main_msg.origin, self.myid));
            self.process_ready(main_msg,rbc_originator, self.myid).await;
        }
    }

    #[async_recursion::async_recursion]
    pub async fn process_ready(&mut self, msg:Msg, rbc_origin:Replica, ready_sender:Replica){
        let rbc_state;
        if msg.rnd_estm{
            rbc_state = &mut self.mvba_state;
        }
        else {
            rbc_state = &mut self.rbc_state;
        }
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
        // log::debug!("READY check: Current Round {}, reqRound: {}, readys.len {}, contains key: {}"
        // ,self.round,msg.value, readys.len(),rbc_state.node_msgs.contains_key(&rbc_origin));
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
            // Is the message MVBA or RBC?
            if !msg.rnd_estm {
                // If RBC phase terminated, start MVBA phase with RBC phase's output
                if rbc_state.terminated_rbcs.len() == self.num_nodes-self.num_faults{
                    let mut term_vec = Vec::new();
                    for rbc_term in rbc_state.terminated_rbcs.clone().into_iter(){
                        term_vec.push(rbc_term);
                    }
                    let msg = Msg{ 
                        value: self.input_val as u64, 
                        origin: self.myid,
                        round: 0 as u64, 
                        rnd_estm: true, 
                        message: term_vec
                    };
                    self.start_rbc(msg).await;
                }
                // Update witnesses
                for (index,values) in self.mvba_state.node_msgs.iter(){
                    if self.mvba_state.terminated_rbcs.contains(index) && !self.witnesses.contains(index){
                        let mut witness = true;
                        for node_index in values.message.clone().into_iter(){
                            witness = witness && self.rbc_state.terminated_rbcs.contains(&node_index);
                        }
                        if witness{
                            self.witnesses.insert(*index);
                        }
                    }
                }
            }
            else {
                // Add a node to witness list when you accept the n-f RBCs broadcasted by it. 
                let mut avail = true;
                for replica in msg.message.clone().into_iter(){
                    if self.rbc_state.terminated_rbcs.contains(&replica){
                        avail = avail && true;
                    }
                    else {
                        avail = false;
                        break;
                    }
                }
                if avail{
                    self.witnesses.insert(msg.origin);
                }
                // Elect a leader accepting n-f witnesses
                if self.witnesses.len() == self.num_nodes-self.num_faults{
                    // Start leader election
                    self.elect_leader(self.leader_round).await;
                }
            }
        }
    }
}