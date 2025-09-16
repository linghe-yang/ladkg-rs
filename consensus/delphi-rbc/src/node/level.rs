use std::collections::BTreeMap;

use types::{Lev, Point, Val, Round, Replica};
use std::ops::Bound::{Included,Excluded};

use super::Interval;

pub struct Level{
    pub num: Lev,
    pub intervals: BTreeMap<Point,Interval>,
    pub sep: Val
}

impl Level{
    pub fn new(sep:Val, num:Lev, val: Val, minth: usize, highth:usize)-> Self{
        // Find nearest multiple of sep to val.
        let interval_start = ((val/sep)-1)*(sep);
        let interval_end = ((val/sep)+2)*(sep);
        let int_s = Interval::new(i64::MIN, interval_start, num, minth, highth);
        let int_2s = Interval::new(interval_start, interval_end, num, minth, highth);
        let int_3s = Interval::new(interval_end, i64::MAX, num, minth, highth);

        let mut interval_map = BTreeMap::new();
        interval_map.insert(i64::MIN, int_s);
        interval_map.insert(interval_start,int_2s);
        interval_map.insert(interval_end, int_3s);

        Level { 
            num: num, 
            intervals: interval_map,
            sep: sep 
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
        log::info!("In level {}, intervals {:?} have not terminated round {}", self.num,term_vec, round);
        terminated
    }

    pub fn add_echo(&mut self, interval_vals:Vec<(Point,Point,Val)>, round:Round, echo_sender:Replica)->(Vec<(Point,Point,Val)>,Vec<(Point,Point,Val)>){
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

    pub fn add_echo2(&mut self, interval_vals:Vec<(Point,Point,Val)>, round:Round, echo_sender:Replica){
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

    pub fn compress(mut interval_vals:Vec<(Point,Point,Val)>, lev:Lev)->Vec<(Point,Point,Val)>{
        if interval_vals.len() > 1{
            let num = interval_vals.len();
            let last_val = interval_vals[num-1];
            let mut last_but_one = interval_vals.get_mut(num-2).unwrap();
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

    pub fn early_terminate(&self, round:Round,max_value:Val)->bool{
        // Check if the level is characterized by only 3 intervals
        log::info!("Early termination request for level {}, round {}, intervals: {:?}",self.num,round,self.intervals.keys());
        if self.intervals.len() == 3{
            let mut index = 0;
            let mut condition = true;
            for (_int_start,interval) in self.intervals.iter() {
                if index == 0 || index == 2{
                    condition = condition && interval.round_terminate(round) && (interval.term_value(round) == 0) && interval.one_value_termination(round);
                }
                else {
                    condition = condition && interval.round_terminate(round) && (interval.term_value(round) == max_value) && interval.one_value_termination(round);
                }
                index = index+1;
            }
            condition
        }
        else {
            false
        }
    }
}