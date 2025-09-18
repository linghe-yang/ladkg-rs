use std::{collections::{HashSet, HashMap}, net::{SocketAddr}, time::{SystemTime, UNIX_EPOCH, Duration}};
use std::net::SocketAddrV4;
use anyhow::{Result, anyhow};
use avsss::r_ring::R;
use fnv::FnvHashMap;
use network::{plaintcp::{TcpReliableSender, CancelHandler}, Acknowledgement};
use network::plaintcp::TcpReceiver;
use tokio::sync::{oneshot, mpsc::{UnboundedReceiver}};
use tokio::sync::mpsc::unbounded_channel;
use types::{Replica, SyncMsg, SyncState};
use crate::sync_handler::SyncHandler;

pub struct Syncer{
    pub num_nodes: usize,
    pub start_time: u128,
    pub net_map: FnvHashMap<Replica,String>,
    pub alive: HashSet<Replica>,
    pub timings:HashMap<Replica,u128>,
    pub th_pks: HashMap<Replica,R>,
    pub cli_addr: SocketAddr,
    pub rx_net: UnboundedReceiver<SyncMsg>,
    pub net_send: TcpReliableSender<Replica,SyncMsg,Acknowledgement>,
    exit_rx: oneshot::Receiver<()>,
    /// Cancel Handlers
    pub cancel_handlers: Vec<CancelHandler<Acknowledgement>>,
}

impl Syncer{
    pub fn spawn(
        net_map: FnvHashMap<Replica,String>,
        cli_addr:SocketAddr,
    )-> Result<oneshot::Sender<()>>{


        let (exit_tx, exit_rx) = oneshot::channel();
        let (tx_net_to_server, rx_net_to_server) = unbounded_channel();
        let cli_addr_sock = cli_addr.port();
        let new_sock_address = SocketAddrV4::new("0.0.0.0".parse()?, cli_addr_sock);
        TcpReceiver::<Acknowledgement, SyncMsg, _>::spawn(
            SocketAddr::V4(new_sock_address),
            SyncHandler::new(tx_net_to_server),
        );
        let mut server_addrs :FnvHashMap<Replica,SocketAddr>= FnvHashMap::default();
        for (replica,address) in net_map.iter(){
            let address:SocketAddr = address.parse().expect("Unable to parse address");
            server_addrs.insert(*replica, SocketAddr::from(address.clone()));
        }
        let net_send = TcpReliableSender::<Replica,SyncMsg,Acknowledgement>::with_peers(server_addrs);
        tokio::spawn(async move{
            let mut syncer = Syncer{
                net_map:net_map.clone(),
                start_time:0,
                num_nodes:net_map.len(),
                alive:HashSet::default(),
                th_pks:HashMap::default(),
                timings:HashMap::default(),
                cli_addr,
                rx_net:rx_net_to_server,
                net_send,
                exit_rx,
                cancel_handlers:Vec::new(),
            };
            if let Err(e) = syncer.run().await {
                log::error!("Consensus error: {}", e);
            }
        });
        Ok(exit_tx)
    }
    pub async fn broadcast(&mut self, sync_msg:SyncMsg){
        for replica in 0..self.num_nodes {
            let cancel_handler:CancelHandler<Acknowledgement> = self.net_send.send(replica, sync_msg.clone()).await;
            self.add_cancel_handler(cancel_handler);    
        }
    }
    pub async fn run(&mut self)-> Result<()>{
        loop {
            tokio::select! {
                // Receive exit handlers
                exit_val = &mut self.exit_rx => {
                    exit_val.map_err(anyhow::Error::new)?;
                    log::info!("Termination signal received by the server. Exiting.");
                    break
                },
                msg = self.rx_net.recv() => {
                    // Received a protocol message
                    // Received a protocol message
                    log::debug!("Got a message from the server: {:?}", msg);
                    let msg = msg.ok_or_else(||
                        anyhow!("Networking layer has closed")
                    )?;
                    match msg.state{
                        SyncState::ALIVE=>{
                            log::info!("Got ALIVE message from node {}",msg.sender);
                            self.alive.insert(msg.sender);
                            if self.alive.len() == self.num_nodes{
                                // sleep before sending message
                                std::thread::sleep(Duration::from_secs(3));
                                self.broadcast(SyncMsg { 
                                    sender: self.num_nodes, 
                                    state: SyncState::StartVSS,
                                }).await;
                                log::info!("StartVSS message broadcast to all nodes");
                                self.start_time = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis();
                            }
                        },
                        SyncState::COMPLETED(pub_key)=>{
                            log::info!("Got COMPLETED message from node {}",msg.sender);
                            self.timings.insert(msg.sender, SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis());
                            self.th_pks.insert(msg.sender,*pub_key.clone());
                            if self.timings.len() == self.num_nodes{
                                // All nodes terminated protocol
                                let mut vec_times = Vec::new();
                                for (_rep,time) in self.timings.iter(){
                                    vec_times.push(time.clone()-self.start_time);
                                }
                                vec_times.sort();
                                let r0 = self.th_pks.get(&msg.sender).unwrap();
                                let mut flag_pk_consistent = true;
                                for (_,pk) in self.th_pks.iter() {
                                    if r0 != pk {
                                        flag_pk_consistent = false;
                                        log::info!("Inconsistent pk detected");
                                        break;
                                    }

                                }
                                if flag_pk_consistent {
                                    log::info!("All n nodes completed the protocol {:?} with threshold pk: {:?}",vec_times,r0);
                                }


                                self.broadcast(SyncMsg { sender: self.num_nodes, state: SyncState::STOP}).await;
                                log::info!("STOP message broadcast to all nodes");
                            }
                        }
                        _=>{}
                    }
                },
            }
        }
        Ok(())
    }
    pub fn add_cancel_handler(&mut self, canc: CancelHandler<Acknowledgement>){
        self.cancel_handlers
            .push(canc);
    }
}

