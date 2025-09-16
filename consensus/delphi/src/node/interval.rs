use std::collections::{BTreeMap};

use types::{Round, Point, Lev, Replica, Val};

use super::RoundStateBin;

/**
 * The Interval object encapsulates the state of one or more checkpoints.
 * An interval can contain multiple checkpoints. However, it must contain at least one checkpoint.
 * In our protocol, an interval can represent multiple checkpoints if they have identical state.
 * Instead of maintaining k different checkpoints and k cloned states, this interval abstraction saves memory by keeping track of checkpoints with identical state.
 */
#[derive(Debug,Clone)]
pub struct Interval{
    // RoundStateBin is the state of a round of Binary Approximate Agreement.
    pub state: BTreeMap<Round,RoundStateBin>,
    pub start: Point,
    pub end: Point,
    pub level: Lev,
    // t+1
    pub minthreshold: usize,
    // n-t
    pub highthreshold: usize,

    pub del_inst_id: usize
}

impl Interval{
    pub fn new(start:Point, end:Point, lev:Lev, minth: usize, highth:usize, inst_id: usize)-> Self{
        return Interval {
            state: BTreeMap::new(),
            start: start,
            end: end,
            level: lev,
            minthreshold: minth,
            highthreshold: highth,
            del_inst_id: inst_id
        }
    }

    pub fn round_terminate(&self, round:Round)->bool{
        if self.state.contains_key(&round){
            return self.state.get(&round).unwrap().terminated();
        }
        else {
            return false;
        }
    }

    pub fn term_value(&self, round:Round)->Val{
        self.state.get(&round).unwrap().term_val.unwrap()
    }
    /**
     * An interval can be split into smaller intervals. We clone the state and assign different start and end points.
     * This happens when the states of checkpoints within this interval diverge (possibly by receiving different ECHO1 and ECHO2 messages).
     */
    pub fn split(&self, index: Point)->(Interval,Interval){
        log::debug!("Interval {}->{} being split at {} in level {} at inst: {}", self.start,self.end,index,self.level, self.del_inst_id);
        let mut int_st = self.clone();
        int_st.end = index;
        let mut int_en = self.clone();
        int_en.start = index;
        (int_st,int_en)
    }

    // Creates a new RoundStateBin object
    fn new_round_with_echo(&mut self, round:Round,msg:Val,echo_sender:Replica){
        if !self.state.contains_key(&round){
            let round_state = RoundStateBin::new_with_echo(msg, echo_sender, self.start, self.end, self.level);
            self.state.insert(round, round_state);
        }
    }

    fn new_round_with_echo2(&mut self, round:Round,msg:Val,echo2_sender:Replica){
        if !self.state.contains_key(&round){
            let round_state = RoundStateBin::new_with_echo2(msg, echo2_sender, self.start, self.end, self.level);
            self.state.insert(round, round_state);
        }
    }

    // This method handles an ECHO1 message. The processing logic of ECHO1s is within the RoundStateBin object.
    pub fn add_echo(&mut self,round:Round,msg:Val,echo_sender:Replica)-> (Option<Val>,Option<Val>){
        if !self.state.contains_key(&round){
            self.new_round_with_echo(round, msg, echo_sender);
            return (None,None);
        }
        else{
            let rnd_state = self.state.get_mut(&round).unwrap();
            return rnd_state.add_echo(msg, echo_sender, self.minthreshold, self.highthreshold);
        }
    }

    // This method handles an ECHO2 message. The processing logic of ECHO2s is within the RoundStateBin object.
    pub fn add_echo2(&mut self,round:Round,msg:Val,echo2_sender:Replica)-> bool{
        if !self.state.contains_key(&round){
            self.new_round_with_echo2(round, msg, echo2_sender);
            return false;
        }
        else{
            let rnd_state = self.state.get_mut(&round).unwrap();
            rnd_state.add_echo2(msg, echo2_sender, self.highthreshold);
            return rnd_state.terminated();
        }
    }

    /*
        Starts a new round of Binary Approximate Agreement.
     */
    pub fn start_round(&mut self,round:Round,myid:Replica, val:Val,max_val:Val)->(Point,Point,Val){
        if round> 0 && self.state.contains_key(&(round-1)) && self.state.get(&(round-1)).unwrap().terminated(){
            let val = self.state.get(&(round-1)).unwrap().term_val.unwrap();
            let rnd_state = RoundStateBin::new_with_echo(val, myid, self.start, self.end, self.level);
            self.state.insert(round, rnd_state);
            return (self.start,self.end,val);
        }
        else if round == 0{
            let rnd_state;
            let ret_val:Val;
            if val>=self.start && val <= self.end{
                rnd_state = RoundStateBin::new_with_echo(max_val, myid, self.start, self.end, self.level);
                ret_val = max_val;
            }
            else {
                rnd_state = RoundStateBin::new_with_echo(0, myid, self.start, self.end, self.level);
                ret_val = 0;
            }
            self.state.insert(round, rnd_state);
            return (self.start,self.end,ret_val);
        }
        else {
            log::error!("Old Round {} did not end for interval {}->{} at level {}",round-1,self.start,self.end,self.level);
            return (0,0,0);
        }
    }
}