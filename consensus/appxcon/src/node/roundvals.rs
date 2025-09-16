use std::collections::{HashSet, HashMap};

use types::appxcon::{Replica, Msg};
/**
 * The State of each round of Approximate Agreement. 
 * It contains information about Reliable Broadcasts of all n nodes, Witnesses sent and accepted, and Accepted values through RBC. 
 */
#[derive(Debug,Clone)]
pub struct RoundState{
    pub node_msgs: HashMap<Replica,Msg>,
    pub echos: HashMap<Replica,HashSet<Replica>>,
    pub readys: HashMap<Replica,HashSet<Replica>>,
    pub accepted_vals: Vec<u64>,
    pub witnesses: HashMap<Replica,Vec<Replica>>,
    pub terminated_rbcs: HashSet<Replica>,
    pub accepted_witnesses: HashSet<Replica>,
    pub witness_sent:bool
}

impl RoundState{
    pub fn new()-> RoundState{
        RoundState{
            node_msgs: HashMap::default(),
            echos: HashMap::default(),
            readys:HashMap::default(),
            witnesses:HashMap::default(),
            accepted_vals: Vec::new(),
            terminated_rbcs:HashSet::default(),
            accepted_witnesses:HashSet::default(),
            witness_sent:false
        }
    }
    pub fn insert_node(&mut self, msg:Msg){
        self.node_msgs.insert(msg.origin, msg.clone());
    }
}