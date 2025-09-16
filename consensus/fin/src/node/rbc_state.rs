use std::collections::{HashSet, HashMap};

use types::appxcon::{Replica, Msg};

#[derive(Debug,Clone)]
pub struct RBCState{
    pub node_msgs: HashMap<Replica,Msg>,
    pub echos: HashMap<Replica,HashSet<Replica>>,
    pub readys: HashMap<Replica,HashSet<Replica>>,
    pub accepted_vals: Vec<u64>,
    pub terminated_rbcs: HashSet<Replica>,
}

impl RBCState{
    pub fn new()-> RBCState{
        RBCState{
            node_msgs: HashMap::default(),
            echos: HashMap::default(),
            readys:HashMap::default(),
            accepted_vals: Vec::new(),
            terminated_rbcs:HashSet::default(),
        }
    }
    pub fn insert_node(&mut self, msg:Msg){
        self.node_msgs.insert(msg.origin, msg.clone());
    }
}