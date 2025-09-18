use super::{Handler, Level};
use anyhow::{anyhow, Result};
use config::Node;
use fnv::FnvHashMap;
use network::{
    plaintcp::{CancelHandler, TcpReceiver, TcpReliableSender},
    Acknowledgement,
};
use std::{
    collections::HashMap,
    net::{SocketAddr, SocketAddrV4},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc::{Receiver, Sender, UnboundedSender};
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver},
};
use tokio::sync::{mpsc, Mutex};
use types::{
    appxcon::{ProtMsg, Replica, WrapperMsg},
    Lev, Round, Val,
};
/**
 * This context contains necessary state variables for executing Delphi
 */
pub struct Delphi {
    /// Networking context
    pub net_send: Arc<Mutex<TcpReliableSender<Replica, WrapperMsg, Acknowledgement>>>,
    pub net_recv: UnboundedReceiver<WrapperMsg>,
    // pub sync_send: Arc<Mutex<TcpReliableSender<Replica, SyncMsg, Acknowledgement>>>,
    // pub sync_recv: UnboundedReceiver<SyncMsg>,

    /// Data context
    pub num_nodes: usize,
    pub myid: usize,
    pub num_faults: usize,
    pub payload: usize,

    /// PKI
    /// Map of secret keys
    pub sec_key_map: HashMap<Replica, Vec<u8>>,

    /// Round number and Approx Consensus related context
    pub round: Round,
    // Starting value v_i
    pub value: Val,
    // Starting separation \rho_0
    pub rho: Val,
    // Desired \epsilon value
    pub epsilon: Val,
    // \Delta value for parameter setting
    pub maxrange: Val,
    // Exponent: The separation increase factor between levels.
    // If separation between checkpoints at level 0 is \rho_0, at level 1, it is exponent*\rho_0 and so on.
    pub exponent: f32,

    // Total number of rounds for Approximate Agreement
    pub total_rounds_bin: Round,
    // Total number of levels. Calculated based on \Delta, \rho_0, and exponent.
    pub total_levels: Lev,

    pub input: Val,
    pub max_input: Val,
    /// State context: Contains the map of levels from 0 to total_levels. Keeps track of Binary Approximate Agreement instances at all levels.
    pub round_state: HashMap<Lev, Level>,
    /// Cancel Handlers
    pub cancel_handlers: HashMap<Round, Vec<CancelHandler<Acknowledgement>>>,

    pub del_inst_id: usize,

    pub consensus_tx: Sender<(usize, Val)>,
    pub consensus_rx: Receiver<(usize, Val)>,

    pub res_sender: Arc<Sender<(usize, Val)>>,
}


impl Delphi {
    /**
     * Protocol begins here.
     */
    pub fn spawn(
        config: Node,
        // epsilon: Val,
        // rho: Val,
        // maxrange: Val,
        // exponent: f32,
        mut val_rx: Receiver<(usize, Val)>,
        result_sender: Arc<Sender<(usize, Val)>>,
    ) ->Result<()>{
        let epsilon = config.delphi.epsilon;
        let rho = config.delphi.delta;
        let maxrange = config.delphi.tri;
        let exponent = config.delphi.expo;

        let prot_payload = &config.prot_payload;
        let v: Vec<&str> = prot_payload.split(',').collect();
        let mut consensus_addrs: FnvHashMap<Replica, SocketAddr> = FnvHashMap::default();
        for (replica, address) in config.net_map_delphi.iter() {
            let address: SocketAddr = address.parse().expect("Unable to parse address");
            consensus_addrs.insert(*replica, SocketAddr::from(address.clone()));
        }
        // let mut syncer_map: FnvHashMap<Replica, SocketAddr> = FnvHashMap::default();
        // syncer_map.insert(0, config.client_addr);
        let my_port = consensus_addrs.get(&config.id).unwrap();
        let my_address = to_socket_address("0.0.0.0", my_port.port());
        // let syncer_listen_port = config.client_port;
        // let syncer_l_address = to_socket_address("0.0.0.0", syncer_listen_port);

        // Setup networking

        // 鍒涘缓WrapperMsg鐨剆enders鍜宺eceivers HashMap
        let mut senders: HashMap<usize, UnboundedSender<WrapperMsg>> = HashMap::new();
        let mut receivers: HashMap<usize, UnboundedReceiver<WrapperMsg>> = HashMap::new();

        // 鍒涘缓SyncMsg鐨剆enders鍜宺eceivers HashMap
        // let mut sync_senders: HashMap<usize, UnboundedSender<SyncMsg>> = HashMap::new();
        // let mut sync_receivers: HashMap<usize, UnboundedReceiver<SyncMsg>> = HashMap::new();

        // 绗竴涓惊鐜細鍒涘缓閫氶亾骞跺瓨鍌?
        let num_instances = config.num_nodes;
        for inst_id in 0..num_instances {
            let (tx, rx) = unbounded_channel::<WrapperMsg>();
            senders.insert(inst_id, tx);
            receivers.insert(inst_id, rx);
            // let (sync_tx, sync_rx) = unbounded_channel::<SyncMsg>();
            // sync_senders.insert(inst_id, sync_tx);
            // sync_receivers.insert(inst_id, sync_rx);
        }

        // 璁剧疆缃戠粶锛氬皢senders鍜宻ync_senders浼犻€掔粰Handler
        TcpReceiver::<Acknowledgement, WrapperMsg, _>::spawn(my_address, Handler::new(senders));
        // TcpReceiver::<Acknowledgement, SyncMsg, _>::spawn(
        //     syncer_l_address,
        //     SyncHandler::new(sync_senders),
        // );
        // let _sleep_time = sleep - SystemTime::now().duration_since(UNIX_EPOCH)
        // .unwrap()
        // .as_millis();
        log::info!("Delphi Consensus addrs {:?}", consensus_addrs);
        let consensus_net = Arc::new(Mutex::new(TcpReliableSender::<
            Replica,
            WrapperMsg,
            Acknowledgement,
        >::with_peers(consensus_addrs.clone())));
        // let sync_net = Arc::new(Mutex::new(TcpReliableSender::<
        //     Replica,
        //     SyncMsg,
        //     Acknowledgement,
        // >::with_peers(syncer_map)));
        if v[0] == "a" {
            tokio::spawn(async move {
                while let Some((inst_id, value)) = val_rx.recv().await {
                    log::info!("Start delphi for inst: {:?} with val: {}", inst_id, value);
                    if let Some(rx_net_to_consensus) = receivers.remove(&inst_id) {
                        // if let Some(rx_net_from_client) = sync_receivers.remove(&inst_id) {
                        let (consensus_tx, consensus_rx) = mpsc::channel(1);
                        let net_send = Arc::clone(&consensus_net);
                        // let sync_send = Arc::clone(&sync_net);
                        let res_sender = Arc::clone(&result_sender);
                        let config_clone = config.clone();
                        tokio::task::spawn(async move {
                            let prot_payload = &config_clone.prot_payload;
                            let v: Vec<&str> = prot_payload.split(',').collect();
                            let _init_value: u64 = v[1].parse::<u64>().unwrap();
                            let exponent: f32 = exponent;
                            let levels = maxrange as f64 / rho as f64;
                            let exponent_log = (exponent as f64).log2();
                            let levels = (levels.log2() / exponent_log).ceil() as Lev;
                            let rounds = ((2
                                * maxrange
                                * (config_clone.num_nodes as i64 + 3)
                                * (levels as i64)) as f64
                                / epsilon as f64)
                                .log2()
                                .ceil() as Round;
                            let max_input: Val = exponent.powf((rounds + 1) as f32).ceil() as Val;
                            let mut levelmap: HashMap<Lev, Level> = HashMap::default();
                            for level in 0..levels {
                                let sep = rho * ((exponent.powf(level as f32).ceil()) as Val);
                                levelmap.insert(
                                    level,
                                    Level::new(
                                        sep,
                                        level,
                                        value,
                                        config_clone.num_faults + 1,
                                        config_clone.num_nodes - config_clone.num_faults,
                                        inst_id,
                                    ),
                                );
                            }
                            let mut c = Delphi {
                                net_send,
                                net_recv: rx_net_to_consensus,
                                // sync_send,
                                // sync_recv: rx_net_from_client,
                                num_nodes: config_clone.num_nodes,
                                sec_key_map: HashMap::default(),
                                myid: config_clone.id,
                                num_faults: config_clone.num_faults,
                                payload: config_clone.payload,
                                round: 0,
                                value,
                                rho,
                                epsilon,
                                maxrange,
                                exponent,
                                total_rounds_bin: rounds,
                                total_levels: levels,
                                input: value,
                                max_input,
                                round_state: levelmap,
                                cancel_handlers: HashMap::default(),
                                del_inst_id: inst_id,
                                res_sender,
                                consensus_tx,
                                consensus_rx,
                            };
                            for (id, sk_data) in config_clone.sk_map.clone() {
                                c.sec_key_map.insert(id, sk_data.clone());
                            }
                            if let Err(e) = c.run().await {
                                log::error!("Consensus error for instance {}: {}", inst_id, e);
                            }
                        });
                        // }
                        // else {
                        //     log::error!("No sync receiver found for inst_id: {}", inst_id);
                        // }
                    } else {
                        log::error!("No receiver found for inst_id: {}", inst_id);
                    }
                }

            });
            Ok(())

        } else {
            panic!("Invalid configuration for protocol");
        }
    }

    pub async fn broadcast(&mut self, protmsg: ProtMsg) {
        let sec_key_map = self.sec_key_map.clone();
        for (replica, sec_key) in sec_key_map.into_iter() {
            if replica != self.myid {
                let wrapper_msg = WrapperMsg::new(
                    protmsg.clone(),
                    self.myid,
                    &sec_key.as_slice(),
                    self.del_inst_id,
                );
                let cancel_handler: CancelHandler<Acknowledgement> =
                    self.net_send.lock().await.send(replica, wrapper_msg).await;
                self.add_cancel_handler(cancel_handler);
            }
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let inst_id = self.del_inst_id;

        // Send the client message that we are alive and kicking

        // let cancel_handler = self
        //     .sync_send
        //     .lock()
        //     .await
        //     .send(
        //         0,
        //         SyncMsg {
        //             sender: self.myid,
        //             state: SyncState::ALIVE,
        //             value: 0,
        //             inst_id,
        //         },
        //     )
        //     .await;
        // self.add_cancel_handler(cancel_handler);

        log::error!(
            "Consensus Start time (inst_id: {}): {:?}",
            inst_id,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );
        self.start_baa(0 as Round).await;
        // let cancel_handler = self.sync_send.lock().await.send(0, SyncMsg {
        //     sender: self.myid,
        //     state: SyncState::STARTED,
        //     value: 0,
        //     inst_id,
        // }).await;
        // self.add_cancel_handler(cancel_handler);

        loop {
            tokio::select! {
                msg = self.net_recv.recv() => {
                    let msg = msg.ok_or_else(|| anyhow!("Networking layer has closed"))?;
                    log::debug!("Got a consensus message from the network (inst_id: {}): {:?}", inst_id, msg);
                    self.process_msg(msg).await;
                },
                cons = self.consensus_rx.recv() => {
                    log::info!("Delphi Consensus Stop time (inst_id: {}): {:?}", inst_id, SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis());
                    self.res_sender.send(cons.unwrap()).await.map_err(anyhow::Error::new)?;
                    // break
                }
                // sync_msg = self.sync_recv.recv() => {
                //     let sync_msg = sync_msg.ok_or_else(|| anyhow!("Networking layer has closed"))?;
                //     match sync_msg.state {
                //         SyncState::START => {
                //             log::error!("Consensus Start time (inst_id: {}): {:?}", inst_id, SystemTime::now()
                //                 .duration_since(UNIX_EPOCH)
                //                 .unwrap()
                //                 .as_millis());
                //             self.start_baa(0 as Round, inst_id).await;
                //             let cancel_handler = self.sync_send.lock().await.send(0, SyncMsg {
                //                 sender: self.myid,
                //                 state: SyncState::STARTED,
                //                 value: 0,
                //                 inst_id,
                //             }).await;
                //             self.add_cancel_handler(cancel_handler);
                //         },
                //         SyncState::STOP => {
                //             log::error!("Consensus Stop time (inst_id: {}): {:?}", inst_id, SystemTime::now()
                //                 .duration_since(UNIX_EPOCH)
                //                 .unwrap()
                //                 .as_millis());
                //             log::info!("Termination signal received by the server (inst_id: {}). Exiting.", inst_id);
                //             break
                //         },
                //         _ => {},
                //     }
                // }

            }
        }
    }
    pub fn add_cancel_handler(&mut self, canc: CancelHandler<Acknowledgement>) {
        self.cancel_handlers
            .entry(self.round)
            .or_default()
            .push(canc);
    }
}

pub fn to_socket_address(ip_str: &str, port: u16) -> SocketAddr {
    let addr = SocketAddrV4::new(ip_str.parse().unwrap(), port);
    addr.into()
}