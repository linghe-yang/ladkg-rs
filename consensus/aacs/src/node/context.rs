use crate::node::handler::Handler;
use config::Node;
use delphi::node::Delphi;
use fnv::FnvHashMap;
use log::info;
use network::Acknowledgement;
use network::plaintcp::{CancelHandler, TcpReceiver, TcpReliableSender};
use rand::SeedableRng;
use rand::prelude::SliceRandom;
use rand::rngs::StdRng;
use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, SocketAddrV4};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, unbounded_channel, channel};
use types::dkg::msg::{ACSMsg, ACSWrapperMsg};
use types::{Replica, Round, Val};
use types::dkg::trans::Transcript;

pub struct Context {
    pub sec_key_map: HashMap<Replica, Vec<u8>>,
    pub myid: usize,
    pub net_send: TcpReliableSender<Replica, ACSWrapperMsg, Acknowledgement>,
    pub net_recv: UnboundedReceiver<ACSWrapperMsg>,
    pub round: Round,
    pub num_nodes: usize,
    pub num_faults: usize,

    pub high_val: Val,
    pub low_val: Val,
    pub kappa: usize,

    pub payload: usize,
    pub leaders: HashSet<Replica>,
    // pub round_state: HashMap<u64,RoundState>,
    pub trans_recv: Receiver<Transcript>,
    pub trans_buffer: HashMap<Replica, Transcript>,
    pub indices_broadcasted: bool,
    pub indices_buffer: HashMap<Replica, HashSet<Replica>>,
    pub indices_decided: HashSet<Replica>,


    pub coin_req_send: Sender<u32>,
    pub coin_recv: Receiver<(u32, u128)>,

    pub delphi_val_tx: Sender<(usize, Val)>,
    pub delphi_res_rx: Receiver<(usize, Val)>,

    pub delphi_joined: HashSet<Replica>,
    pub delphi_terminated: HashSet<Replica>,

    pub final_set: HashSet<Replica>,
    pub res_send: Sender<Vec<Transcript>>,

    pub terminated: bool,

    pub cancel_handlers: HashMap<Round, Vec<CancelHandler<Acknowledgement>>>,

}

impl Context {
    pub fn spawn(
        config: Node,
        coin_req_send: Sender<u32>,
        coin_recv: Receiver<(u32, u128)>,

        trans_recv: Receiver<Transcript>,
        res_send: Sender<Vec<Transcript>>,
    ) -> anyhow::Result<()> {
        let prot_payload = &config.prot_payload;
        let v: Vec<&str> = prot_payload.split(',').collect();
        let mut consensus_addrs: FnvHashMap<Replica, SocketAddr> = FnvHashMap::default();
        for (replica, address) in config.net_map_rbc.iter() {
            let address: SocketAddr = address.parse().expect("Unable to parse address");
            consensus_addrs.insert(*replica, SocketAddr::from(address.clone()));
        }

        let my_port = consensus_addrs.get(&config.id).unwrap();
        let my_address = to_socket_address("0.0.0.0", my_port.port());

        let (tx_net_to_consensus, rx_net_to_consensus) = unbounded_channel();
        TcpReceiver::<Acknowledgement, ACSWrapperMsg, _>::spawn(
            my_address,
            Handler::new(tx_net_to_consensus),
        );



        let consensus_net = TcpReliableSender::<Replica, ACSWrapperMsg, Acknowledgement>::with_peers(
            consensus_addrs.clone(),
        );
        info!("ACS Consensus addrs {:?}", consensus_addrs);

        let (val_tx, val_rx) = channel(100);
        let (res_tx, res_rx) = channel(100);
        Delphi::spawn(config.clone(), val_rx, Arc::new(res_tx))?;


        if v[0] == "a" {
            tokio::spawn(async move {
                let mut c = Context {
                    net_send: consensus_net,
                    net_recv: rx_net_to_consensus,
                    num_nodes: config.num_nodes,
                    sec_key_map: HashMap::default(),
                    myid: config.id,
                    num_faults: config.num_faults,

                    high_val: config.delphi.high_val,
                    low_val: config.delphi.low_val,
                    kappa: config.acs.kappa,


                    payload: config.payload,
                    round: 0,
                    leaders: HashSet::new(),
                    trans_recv,
                    trans_buffer: HashMap::new(),
                    indices_broadcasted: false,
                    indices_buffer: HashMap::new(),
                    indices_decided: HashSet::new(),
                    coin_req_send,
                    coin_recv,

                    delphi_val_tx: val_tx,
                    delphi_res_rx: res_rx,

                    delphi_joined: HashSet::new(),
                    delphi_terminated: HashSet::new(),

                    final_set: HashSet::new(),
                    res_send,

                    terminated: false,
                    cancel_handlers: HashMap::default(),
                };
                for (id, sk_data) in config.sk_map.clone() {
                    c.sec_key_map.insert(id, sk_data.clone());
                }
                if let Err(e) = c.run().await {
                    log::error!("ACS Consensus error:{}", e);
                }
                log::debug!("Started n-parallel RBC");
            });
            Ok(())
        } else {
            panic!("Invalid configuration for protocol");
        }
    }
    /// Main consensus protocol loop
    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        if let Err(e) = self.coin_req_send.send(0 as u32).await {
            log::warn!(
                "Failed to start hashrand because of error {}",
                e
            );
        }
        self.coin_req_send.send(10).await?;
        while let Some((seq_num,coin)) = self.coin_recv.recv().await {
            if seq_num == 10 {
                self.leaders = select_set(self.num_nodes,self.kappa,coin);
                info!("Node {}: Leaders selected by common coin: {:?}",self.myid, self.leaders);
                break
            }
        }

        loop {
            tokio::select! {
                // Handle incoming transcripts
                Some(transcript) = self.trans_recv.recv() => {
                    info!("Node {}: Transcript received from VSS", self.myid);
                    self.broadcast(ACSMsg::RBCTrans(transcript.clone(), self.myid)).await;
                }
                // Handle network messages
                Some(msg) = self.net_recv.recv() => {
                    self.handle_network_message(msg).await?;
                }
                // Handle Delphi results
                Some((inst_id, val)) = self.delphi_res_rx.recv() => {
                    self.handle_delphi_result(inst_id, val).await?;
                }

            }
        }
    }

    /// Handles incoming network messages
    async fn handle_network_message(&mut self, msg: ACSWrapperMsg) -> Result<(), anyhow::Error> {
        let msg = msg.protmsg;
        match msg {
            ACSMsg::RBCTrans(trans, sender) => {
                info!("Node {}: Received Transcript from {}", self.myid, sender);
                self.trans_buffer.insert(sender, trans);
                // Leader-specific logic
                if self.leaders.contains(&self.myid) && !self.indices_broadcasted {
                    if let Some(set) = find_t_transcripts(&self.trans_buffer, self.num_nodes - self.num_faults) {
                        let indices: HashSet<Replica> = set.into_iter().map(|(r, _)| r).collect();
                        self.broadcast(ACSMsg::RBCIndexSet(indices, self.myid)).await;
                        self.indices_broadcasted = true;
                    }
                }

                // Check for Delphi joins
                for (sender, indices) in self.indices_buffer.iter() {
                    if !self.delphi_joined.contains(sender) && self.all_trans_received(indices) {
                        self.delphi_val_tx.send((*sender, self.high_val)).await?;
                        self.delphi_joined.insert(*sender);
                    }
                }

                // Check if final set is complete
                self.check_final_set().await;
            }
            ACSMsg::RBCIndexSet(indices, sender) => {
                if !self.leaders.contains(&sender) || indices.len() != (self.num_nodes - self.num_faults) || self.delphi_joined.contains(&sender) {
                    return Ok(());
                }
                info!("Node {}: Received Indices set from {}", self.myid, sender);
                self.indices_buffer.insert(sender, indices.clone());
                if self.all_trans_received(&indices) {
                    self.delphi_val_tx.send((sender, self.high_val)).await?;
                    self.delphi_joined.insert(sender);
                }

                self.check_termination_condition().await?;
            }
        }
        Ok(())
    }

    /// Handles Delphi result processing
    async fn handle_delphi_result(&mut self, inst_id: Replica, val: Val) -> Result<(), anyhow::Error> {
        self.delphi_terminated.insert(inst_id);
        if val > (self.high_val + self.low_val) / 2 {
            self.indices_decided.insert(inst_id);
            for leader in self.leaders.iter() {
                if !self.delphi_joined.contains(leader) {
                    self.delphi_val_tx.send((*leader, self.low_val)).await?;
                    self.delphi_joined.insert(*leader);
                }
            }
        }

        self.check_termination_condition().await?;
        Ok(())
    }

    /// Checks if termination condition is met and updates final set
    async fn check_termination_condition(&mut self) -> Result<(), anyhow::Error> {
        if self.delphi_terminated.len() < self.kappa {
            return Ok(());
        }

        if self.indices_decided.iter().all(|r| self.indices_buffer.contains_key(r)) {
            let final_set = self.indices_decided.iter()
                .flat_map(|r| self.indices_buffer.get(r).unwrap().iter().copied())
                .collect::<HashSet<Replica>>();
            self.final_set = final_set;
            info!("Node {}: All Delphi instances terminated, final set decided.", self.myid);
            self.check_final_set().await;
        }
        Ok(())
    }

    /// Checks if all transcripts in final set are received and logs result
    async fn check_final_set(&mut self) {
        if !self.terminated && !self.final_set.is_empty() && self.final_set.iter().all(|r| self.trans_buffer.contains_key(r)) {
            let result = self.final_set.iter()
                .map(|r| self.trans_buffer.get(r).unwrap().clone())
                .collect::<Vec<Transcript>>();
            info!("Node {}: All Transcript in final set received, ACS result formed", self.myid);
            self.res_send.send(result).await.unwrap();
            self.terminated = true;
        }
    }
    pub async fn broadcast(&mut self, protmsg: ACSMsg) {
        let sec_key_map = self.sec_key_map.clone();
        for (replica, sec_key) in sec_key_map.into_iter() {
            let wrapper_msg = ACSWrapperMsg::new(protmsg.clone(), self.myid, &sec_key.as_slice());
            let cancel_handler: CancelHandler<Acknowledgement> =
                self.net_send.send(replica, wrapper_msg).await;
            self.add_cancel_handler(cancel_handler);
        }
    }

    pub fn add_cancel_handler(&mut self, canc: CancelHandler<Acknowledgement>) {
        self.cancel_handlers
            .entry(self.round)
            .or_default()
            .push(canc);
    }

    fn all_trans_received(&self, s_i: &HashSet<Replica>) -> bool {
        s_i.iter()
            .all(|replica| self.trans_buffer.contains_key(replica))
    }

}

pub fn to_socket_address(ip_str: &str, port: u16) -> SocketAddr {
    let addr = SocketAddrV4::new(ip_str.parse().unwrap(), port);
    addr.into()
}

pub fn select_set(n: usize, kappa: usize, seed: u128) -> HashSet<usize> {
    if kappa > n {
        panic!("kappa cannot be greater than n");
    }

    // Convert u128 seed to u64 by taking the lower 64 bits
    let seed_u64 = seed as u64;

    // Initialize a seeded RNG for determinism
    let mut rng = StdRng::seed_from_u64(seed_u64);

    // Create a vector of numbers from 0 to n-1
    let mut nums: Vec<usize> = (0..n).collect();

    // Shuffle the vector using the seeded RNG (Fisher-Yates shuffle ensures fairness)
    nums.shuffle(&mut rng);

    // Take the first `kappa` elements and collect into a HashSet
    nums.into_iter().take(kappa).collect::<HashSet<_>>()
}

pub fn find_t_transcripts(
    map: &HashMap<Replica, Transcript>,
    t: usize,
) -> Option<Vec<(Replica, Transcript)>> {
    if map.len() == t {
        let result = map
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect::<Vec<(Replica, Transcript)>>();
        Some(result)
    } else {
        None
    }
}
