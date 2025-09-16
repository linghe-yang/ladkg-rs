use std::collections::BTreeMap;

use types::{Lev, Point, Val, Round, Replica};
use std::ops::Bound::{Included,Excluded};

use super::Interval;

pub struct Level{
    pub num: Lev,
    pub intervals: BTreeMap<Point,Interval>,
    pub sep: Val,
    pub del_inst_id: usize,
}
/**
 * The Level object contains a list of all intervals (and checkpoints) from i64::MIN to i64::MAX.
 * Checkpoints within this level are spaced by sep.
 * A level is initiated with the node's value v_i. We create three intervals from MIN to v_1-\sep, v_1-\sep to v_1+\sep, v_1 + sep to MAX.
 */
impl Level{
    pub fn new(sep:Val, num:Lev, val: Val, minth: usize, highth:usize, inst_id: usize)-> Self{
        // Find nearest multiple of sep to val.
        let interval_start = ((val/sep)-1)*(sep);
        let interval_end = ((val/sep)+2)*(sep);
        let int_s = Interval::new(i64::MIN, interval_start, num, minth, highth, inst_id);
        let int_2s = Interval::new(interval_start, interval_end, num, minth, highth, inst_id);
        let int_3s = Interval::new(interval_end, i64::MAX, num, minth, highth, inst_id);

        let mut interval_map = BTreeMap::new();
        interval_map.insert(i64::MIN, int_s);
        interval_map.insert(interval_start,int_2s);
        interval_map.insert(interval_end, int_3s);

        Level {
            num: num,
            intervals: interval_map,
            sep: sep,
            del_inst_id: inst_id,
        }
    }

    pub fn terminated_round(&self, round:Round)->bool{
        let mut terminated = true;
        let mut term_vec = Vec::new();
        for (_endpoint,interval) in self.intervals.iter(){
            if !interval.round_terminate(round){
                term_vec.push((interval.start,interval.end));
            }
            terminated = terminated && interval.round_terminate(round);
        }
        log::debug!("In level {}, intervals {:?} have not terminated round {} at inst: {}", self.num,term_vec, round, self.del_inst_id);
        terminated
    }

    /**
     * This method handles ECHO1 messages for multiple checkpoints within the level.
     */
    pub fn add_echo(&mut self, interval_vals:Vec<(Point,Point,Val)>, round:Round, echo_sender:Replica)->(Vec<(Point,Point,Val)>,Vec<(Point,Point,Val)>){
        // Interval vals is a vector of (p1,p2,Val), which indicates an ECHO1 for value Val for all checkpoints between starting point p1 and ending point p2.

        // List of echo1 messages and echo2 messages to send to broadcast.
        let mut echo1msgs:Vec<(Point,Point,Val)> = Vec::new();
        let mut echo2msgs:Vec<(Point,Point,Val)> = Vec::new();
        for (start,end,value) in interval_vals.into_iter(){
            let mut new_interval_map = BTreeMap::new();
            for (_interval_start,interval) in self.intervals.iter(){
                let (echo1,echo2);
                if interval.start < start && interval.end > start{
                    let (int_s,mut int_e) = interval.split(start);
                    if interval.end > end{
                        let (mut sub_int_s,sub_int_e) = int_e.split(end);
                        (echo1,echo2) = sub_int_s.add_echo(round, value, echo_sender);
                        if echo1.is_some(){
                            echo1msgs.push((start,sub_int_s.end,echo1.unwrap()));
                            echo1msgs = Self::compress(echo1msgs,self.num);
                        }
                        if echo2.is_some(){
                            echo2msgs.push((start,sub_int_s.end,echo2.unwrap()));
                            echo2msgs = Self::compress(echo2msgs,self.num);
                        }
                        new_interval_map.insert(int_s.start, int_s);
                        new_interval_map.insert(sub_int_s.start, sub_int_s);
                        new_interval_map.insert(sub_int_e.start, sub_int_e);
                    }
                    else {
                        (echo1,echo2) = int_e.add_echo(round, value, echo_sender);
                        if echo1.is_some(){
                            echo1msgs.push((start,int_e.end,echo1.unwrap()));
                            echo1msgs = Self::compress(echo1msgs,self.num);
                        }
                        if echo2.is_some(){
                            echo2msgs.push((start,int_e.end,echo2.unwrap()));
                            echo2msgs = Self::compress(echo2msgs,self.num);
                        }
                        new_interval_map.insert(int_s.start, int_s);
                        new_interval_map.insert(int_e.start, int_e);
                    }
                }
            }
            for (int_s,interval) in new_interval_map.into_iter(){
                self.intervals.insert(int_s, interval);
            }
            let mut new_interval_map = BTreeMap::new();
            let int_range = self.intervals.range_mut((Included(&start),Excluded(&end)));
            for (_int_start,interval) in int_range{
                if interval.end <= end{
                    let (echo1,echo2) = interval.add_echo(round, value, echo_sender);
                    if echo1.is_some(){
                        echo1msgs.push((interval.start,interval.end,echo1.unwrap()));
                        echo1msgs = Self::compress(echo1msgs,self.num);
                    }
                    if echo2.is_some(){
                        echo2msgs.push((interval.start,interval.end,echo2.unwrap()));
                        echo2msgs = Self::compress(echo2msgs,self.num);
                    }
                }
                else {
                    let (mut s_int,l_int) = interval.split(end);
                    let (echo1,echo2) = s_int.add_echo(round, value, echo_sender);
                    if echo1.is_some(){
                        echo1msgs.push((s_int.start,s_int.end,echo1.unwrap()));
                        echo1msgs = Self::compress(echo1msgs,self.num);
                    }
                    if echo2.is_some(){
                        echo2msgs.push((s_int.start,s_int.end,echo2.unwrap()));
                        echo2msgs = Self::compress(echo2msgs,self.num);
                    }
                    new_interval_map.insert(s_int.start, s_int);
                    new_interval_map.insert(l_int.start, l_int);
                }
            }
            for (int_s,interval) in new_interval_map.into_iter(){
                self.intervals.insert(int_s, interval);
            }
        }
        (echo1msgs,echo2msgs)
    }

    /**
    * This method handles ECHO2 messages for multiple checkpoints within the level.
    */
    pub fn add_echo2(&mut self, interval_vals:Vec<(Point,Point,Val)>, round:Round, echo_sender:Replica){
        // Interval vals is a vector of (p1,p2,Val), which indicates an ECHO2 for value Val for all checkpoints between starting point p1 and ending point p2.
        for (start,end,value) in interval_vals.into_iter(){
            let mut new_interval_map = BTreeMap::new();
            for (_interval_start,interval) in self.intervals.iter(){
                if interval.start < start && interval.end > start{
                    let (int_s,mut int_e) = interval.split(start);
                    if interval.end > end{
                        let (mut sub_int_s,sub_int_e) = int_e.split(end);
                        sub_int_s.add_echo2(round, value, echo_sender);
                        new_interval_map.insert(int_s.start, int_s);
                        new_interval_map.insert(sub_int_s.start, sub_int_s);
                        new_interval_map.insert(sub_int_e.start, sub_int_e);
                    }
                    else {
                        int_e.add_echo2(round, value, echo_sender);
                        new_interval_map.insert(int_s.start, int_s);
                        new_interval_map.insert(int_e.start, int_e);
                    }
                }
            }
            for (int_s,interval) in new_interval_map.into_iter(){
                self.intervals.insert(int_s, interval);
            }
            let mut new_interval_map = BTreeMap::new();
            let int_range = self.intervals.range_mut((Included(&start),Excluded(&end)));
            for (_int_start,interval) in int_range{
                if interval.end <= end{
                    interval.add_echo2(round, value, echo_sender);
                }
                else {
                    let (mut s_int,l_int) = interval.split(end);
                    s_int.add_echo2(round, value, echo_sender);
                    new_interval_map.insert(s_int.start, s_int);
                    new_interval_map.insert(l_int.start, l_int);
                }
            }
            for (int_s,interval) in new_interval_map.into_iter(){
                self.intervals.insert(int_s, interval);
            }
        }
    }
    /**
     * Compress message compresses messages from multiple intervals into a single message (in case such a compression is possible).
     * Say there are two messages (start1,end1,V1) and (end1,end2,V1). This routine creates a single message (start1,end2,V1).
     * This operation saves network bandwidth by reducing redundant information sent.
     */
    pub fn compress(mut interval_vals:Vec<(Point,Point,Val)>, lev:Lev)->Vec<(Point,Point,Val)>{
        if interval_vals.len() > 1{
            let num = interval_vals.len();
            let last_val = interval_vals[num-1];
            let last_but_one = interval_vals.get_mut(num-2).unwrap();
            log::debug!("Compress fn: Last interval: {:?}, last_but_one: {:?} in level {}",last_val,last_but_one,lev);
            if last_but_one.2 == last_val.2 && last_but_one.1 == last_val.0{
                last_but_one.1 = last_val.1;
                interval_vals.pop();
            }
            log::debug!("Compress fn: Vals after compress: {:?}",interval_vals);
            interval_vals
        }
        else {
            interval_vals
        }
    }
    /**
     * Starts new round.
     */
    pub fn start_round(&mut self, round:Round, myid:Replica,val:Val, max_val:Val)->Vec<(Point,Point,Val)>{
        let mut echo_vec:Vec<(Point,Point,Val)> = Vec::new();
        if (round>0 && self.terminated_round(round-1))||round == 0{
            for interval_state in self.intervals.iter_mut() {
                echo_vec.push(interval_state.1.start_round(round, myid, val,max_val));
                echo_vec = Self::compress(echo_vec,self.num);
            }
            return echo_vec;
        }
        else{
            log::error!("Level {} did not terminate round {}",self.num,round);
            return Vec::new();
        }
    }
}