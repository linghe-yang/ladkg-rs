use std::collections::{HashSet};

use types::{appxcon::{Replica}, Point, Lev, Val};

#[derive(Debug,Clone)]
pub struct RoundStateBin{
    // Map of Replica, and binary state of two values, their echos list and echo2 list, list of values for which echo1s were sent and echo2s list
    pub state: Vec<(Val,HashSet<Replica>,HashSet<Replica>,bool,bool)>,
    pub echo1vals: HashSet<Val>,
    pub echo2vals: Vec<Val>,
    pub term_val:Option<Val>,
    pub start: Point,
    pub end: Point,
    pub level:Lev
}

impl RoundStateBin{
    pub fn new_with_echo(msg: Val,echo_sender:Replica, start:Point,end: Point,level:Lev)-> RoundStateBin{
        let mut rnd_state = RoundStateBin{
            state:Vec::new(),
            echo1vals: HashSet::new(),
            echo2vals: Vec::new(),
            term_val:None,
            start:start,
            end:end,
            level:level
        };
        let parsed_bigint = Self::to_target_type(msg.clone());
        //let mut arr_state:Vec<(u64,HashSet<Replica>,HashSet<Replica>,bool,bool)> = Vec::new();
        let mut echo1_set = HashSet::new();
        echo1_set.insert(echo_sender);
        let echo2_set:HashSet<Replica>=HashSet::new();
        rnd_state.state.push((parsed_bigint,echo1_set,echo2_set,false,false));
        rnd_state
    }

    pub fn new_with_echo2(msg: Val,echo_sender:Replica, start:Point,end: Point,level:Lev)-> RoundStateBin{
        let mut rnd_state = RoundStateBin{
            state:Vec::new(),
            echo1vals: HashSet::new(),
            echo2vals: Vec::new(),
            term_val:None,
            start:start,
            end:end,
            level:level
        };
        let parsed_bigint = Self::to_target_type(msg.clone());
        let mut echo2_set = HashSet::new();
        echo2_set.insert(echo_sender);
        let echo1_set:HashSet<Replica>=HashSet::new();
        rnd_state.state.push((parsed_bigint,echo1_set,echo2_set,false,false));
        rnd_state
    }

    pub fn add_echo(&mut self, msg: Val, echo_sender:Replica, minthreshold:usize, highthreshold:usize)-> (Option<Val>,Option<Val>){
        let mut echo1_msg:Option<Val> = None;
        let mut echo2_msg:Option<Val> = None;
        // If the instance has already terminated, do not process messages from this node
        if self.term_val.is_some(){
            return (echo1_msg,echo2_msg);
        }
        let parsed_bigint = Self::to_target_type(msg.clone());
        //if self.state.contains_key(&rep){
        //let arr_tup = self.state.get_mut(&rep).unwrap();
        let arr_vec = &mut self.state;
        // The echo sent by echo_sender was for this value in the bivalent initial value state
        if arr_vec[0].0 == parsed_bigint{
            arr_vec[0].1.insert(echo_sender);
            // check for t+1 votes: if it has t+1 votes, send out another echo1 message
            // check whether an echo has been sent out for this value in this instance
            if arr_vec[0].1.len() >= minthreshold && !arr_vec[0].3{
                log::debug!("Got t+1 ECHO messages for BAA inst {}->{} in Level {}, sending ECHO",self.start,self.end,self.level);
                //arr_vec[0].1.insert(myid);
                echo1_msg = Some(msg.clone());
                arr_vec[0].3 = true;
            }
            // check for 2t+1 votes: if it has 2t+1 votes, send out echo2 message
            else if arr_vec[0].1.len() >= highthreshold && !arr_vec[0].4{
                log::debug!("Got 2t+1 ECHO messages for BAA inst {}->{} in Level {}, sending ECHO2",self.start,self.end,self.level);
                echo2_msg = Some(msg.clone());
                self.echo1vals.insert(parsed_bigint);
                // If you send out ECHO2 messages for two values, you should terminate immediately and not wait for 2t+1 ECHO2 messages
                if self.echo1vals.len() == 2{
                    // terminate protocol for instance &rep
                    let vec_arr:Vec<Val> = self.echo1vals.clone().into_iter().map(|x| x).collect();
                    let next_round_val = (vec_arr[0].clone()+vec_arr[1].clone())/2;
                    self.term_val = Some(next_round_val);
                }
                arr_vec[0].4 = true;
            }
        }
        else{
            if arr_vec.len() == 1{
                // insert new array vector
                let mut echo_set:HashSet<Replica>= HashSet::default();
                echo_set.insert(echo_sender);
                arr_vec.push((parsed_bigint,echo_set,HashSet::default(),false,false));
            }
            else {
                arr_vec[1].1.insert(echo_sender);
                if arr_vec[1].1.len() >= minthreshold && !arr_vec[1].3{
                    log::debug!("Second value {} got t+1 votes in start: {}, end:{},level:{}",parsed_bigint.clone(),self.start,self.end,self.level);
                    //arr_vec[1].1.insert(myid);
                    echo1_msg = Some(msg.clone());
                    arr_vec[1].3 = true;
                }
                else if arr_vec[1].1.len() >= highthreshold && !arr_vec[1].4{
                    echo2_msg = Some(msg.clone());
                    self.echo1vals.insert(parsed_bigint);
                    if self.echo1vals.len() == 2{
                        // terminate protocol for instance &rep
                        let vec_arr:Vec<Val> = self.echo1vals.clone().into_iter().map(|x| x).collect();
                        let next_round_val = (vec_arr[0].clone()+vec_arr[1].clone())/2;
                        self.term_val = Some(next_round_val);
                    }
                    arr_vec[1].4 = true;
                }
            }
        }
        (echo1_msg,echo2_msg)
    }

    pub fn add_echo2(&mut self,msg: Val, echo2_sender:Replica,highthreshold:usize){
        let parsed_bigint = Self::to_target_type(msg.clone());
        // this vector can only contain two elements, if the echo corresponds to the first element, the first if block is executed
        let arr_vec = &mut self.state;
        if arr_vec[0].0 == parsed_bigint{
            arr_vec[0].2.insert(echo2_sender);
            // check for 2t+1 votes: if it has 2t+1 votes, then terminate
            if arr_vec[0].2.len() >= highthreshold{
                self.echo2vals.push(parsed_bigint);
                self.term_val = Some(parsed_bigint);
            }
        }
        else{
            if arr_vec.len() == 1{
                // insert new array vector
                let mut echo2_set:HashSet<Replica>= HashSet::default();
                echo2_set.insert(echo2_sender);
                arr_vec.push((parsed_bigint,HashSet::default(),echo2_set,false,false));
            }
            else{
                arr_vec[1].2.insert(echo2_sender);
                if arr_vec[1].2.len() >= highthreshold{
                    log::debug!("Value {:?} received n-f echo2s for instance start->{}, end->{}, level:{}",arr_vec[1].0.clone(),self.start,self.end,self.level);
                    self.echo2vals.push(parsed_bigint);
                    self.term_val = Some(arr_vec[1].0.clone());
                }
            }
        }
    }

    pub fn terminated(&self)-> bool{
        self.term_val.is_some()
    }

    pub fn to_target_type(msg:Val)->Val{
        msg
        // let mut msg_bytes = [0u8;8];
        // msg_bytes.clone_from_slice(msg.as_slice());
        // u64::from_be_bytes(msg_bytes)
    }

    pub fn num_values_one_two(&self)->bool{
        if self.state.len() == 2{
            if self.state.get(0).unwrap().3 == true && self.state.get(1).unwrap().3 == false{
                true
            }
            else if self.state.get(0).unwrap().3 == false && self.state.get(1).unwrap().3 == true{
                true
            }
            else if self.state.get(0).unwrap().3 == true && self.state.get(1).unwrap().3 == true{
                false
            }
            else {
                false
            }
        }
        else{
            true
        }
    }
}