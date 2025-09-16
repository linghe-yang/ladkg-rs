use std::{net::{SocketAddr, SocketAddrV4}, collections::{HashMap, HashSet, BTreeMap}, time::{UNIX_EPOCH, SystemTime}};
use std::{fs::File, io::{Read}};

use anyhow::{Result, anyhow};
use config::Node;
use crypto_blstrs::threshold_sig::{BlstrsSecretKey, BlstrsPublicKey, Partial, PartialBlstrsSignature};
use fnv::FnvHashMap;
use network::{plaintcp::{TcpReliableSender, TcpReceiver, CancelHandler}, Acknowledgement};
//use tbls::{poly::Poly, curve::bls12377::{G1, Scalar}, sig::Share};
use tokio::sync::{mpsc::{UnboundedReceiver, unbounded_channel}, oneshot};
use types::{Replica, SyncMsg, Round, SyncState, appxcon::{ProtMsg, WrapperMsg,Msg}, Val};

use super::{Handler, SyncHandler, RBCState, RoundStateBin};

pub struct Context{
    pub net_send: TcpReliableSender<Replica,WrapperMsg,Acknowledgement>,
    pub net_recv: UnboundedReceiver<WrapperMsg>,
    pub sync_send:TcpReliableSender<Replica,SyncMsg,Acknowledgement>,
    pub sync_recv: UnboundedReceiver<SyncMsg>,
    /// Data context
    pub num_nodes: usize,
    pub myid: Replica,
    pub num_faults: usize,
    /// PKI
    /// Replica map
    pub sec_key_map:HashMap<usize, Vec<u8>>,
    pub input_val: Val,

    /// Reliable Broadcast State
    pub rbc_state: RBCState,
    pub mvba_state: RBCState,

    /// Common Coin state
    /// Leader election state: Round wise mapping, each round collects partial signatures from nodes for log(n) random bits for leader election.
    pub leader_election_state: HashMap<Round,(HashMap<usize,Vec<PartialBlstrsSignature>>,BTreeMap<usize,u8>,Option<Replica>)>,
    pub witnesses: HashSet<Replica>,

    /// Binary Approximate Agreement State
    pub round_state: HashMap<Round,HashMap<Round,RoundStateBin>>,

    /// Cancel Handlers
    pub cancel_handlers: HashMap<Round,Vec<CancelHandler<Acknowledgement>>>,
    pub leader_round: Round,
    pub baa_round: Round,
    
    /// Threshold setup parameters
    pub tpubkey_share: HashMap<usize,Partial<BlstrsPublicKey>>,
    pub secret_key: Partial<BlstrsSecretKey>,
    pub sign_msg: String,

    pub terminated: bool,
    /// Exit protocol
    exit_rx: oneshot::Receiver<()>,
}

impl Context {
    pub fn spawn(
        config:Node,
        secret_loc:&str,
        pkey_vec: Vec<String>,
        input_val:Val,
        seed:String
    )->anyhow::Result<oneshot::Sender<()>>{
        let mut consensus_addrs :FnvHashMap<Replica,SocketAddr>= FnvHashMap::default();
        for (replica,address) in config.net_map.iter(){
            let address:SocketAddr = address.parse().expect("Unable to parse address");
            consensus_addrs.insert(*replica, SocketAddr::from(address.clone()));
        }
        let my_port = consensus_addrs.get(&config.id).unwrap();
        let my_address = to_socket_address("0.0.0.0", my_port.port());
        let mut syncer_map:FnvHashMap<Replica,SocketAddr> = FnvHashMap::default();
        syncer_map.insert(0, config.client_addr);
        // No clients needed

        // Setup networking
        let (tx_net_to_consensus, rx_net_to_consensus) = unbounded_channel();
        TcpReceiver::<Acknowledgement, WrapperMsg, _>::spawn(
            my_address,
            Handler::new(tx_net_to_consensus),
        );
        let syncer_listen_port = config.client_port;
        let syncer_l_address = to_socket_address("0.0.0.0", syncer_listen_port);
        // The server must listen to the client's messages on some port that is not being used to listen to other servers
        let (tx_net_to_client,rx_net_from_client) = unbounded_channel();
        TcpReceiver::<Acknowledgement,SyncMsg,_>::spawn(
            syncer_l_address, 
            SyncHandler::new(tx_net_to_client)
        );
        let consensus_net = TcpReliableSender::<Replica,WrapperMsg,Acknowledgement>::with_peers(
            consensus_addrs.clone()
        );
        
        let sync_net = TcpReliableSender::<Replica,SyncMsg,Acknowledgement>::with_peers(syncer_map);
        let (exit_tx, exit_rx) = oneshot::channel();

        // Secret key
        //let secret_key:PathBuf = PathBuf::from(secret_loc.clone());
        log::info!("Secret key file path {} {:?}",secret_loc.clone(),pkey_vec.clone());
        let publickey_vec = pkey_vec.into_iter().map(|pkey_loc| {
            let mut thresh_pubkey_buffer = Vec::new();
            File::open(pkey_loc.as_str())
                        .expect("Unable to open polypub file")
                        .read_to_end(&mut thresh_pubkey_buffer)
                        .expect("Unable to read polydata");
            let thresh_pubkey:BlstrsPublicKey = bincode::deserialize(thresh_pubkey_buffer.as_slice()).expect("Unable to deserialize pubkey data");
            thresh_pubkey
        }).collect::<Vec<_>>();
        let mut pkey_map = HashMap::default();
        let mut i:usize = 1;
        for pkey in publickey_vec.into_iter(){
            pkey_map.insert(i, Partial{
                idx: i as usize,
                data:pkey
            });
            i+=1;
        }
        let mut secret_key_buffer = Vec::new();
        File::open(secret_loc)
                    .expect("Unable to open threshold secret key")
                    .read_to_end(&mut secret_key_buffer)
                    .expect("Unable to read threshold secret key");
        //let pubkey_poly:BlstrsPublicKey = bincode::deserialize(pubkey_poly_buffer.as_slice()).expect("Unable to deserialize pubkey poly data");
        let secret_key:BlstrsSecretKey = bincode::deserialize(secret_key_buffer.as_slice()).expect("Unable to deserialize threshold secret key");
        let secret_share:Partial<BlstrsSecretKey> = Partial { 
            idx: config.id+1, 
            data: secret_key
        };
        tokio::spawn(async move {
            // The modulus of the secret is set for probability of coin success = 1- 5*10^{-9}
            let mut c = Context {
                net_send:consensus_net,
                net_recv:rx_net_to_consensus,
                sync_send: sync_net,
                sync_recv: rx_net_from_client,
                num_nodes: config.num_nodes.try_into().unwrap(),
                sec_key_map: HashMap::default(),
                myid: config.id.try_into().unwrap(),
                num_faults: config.num_faults.try_into().unwrap(),
                cancel_handlers:HashMap::default(),
                leader_round: 0,
                baa_round:0,
                exit_rx:exit_rx,
                //secret:secret,
                sign_msg: seed,

                input_val:input_val,

                rbc_state: RBCState::new(),
                mvba_state: RBCState::new(),
                witnesses: HashSet::default(),

                leader_election_state: HashMap::new(),

                round_state:HashMap::default(),
                terminated:false,
                tpubkey_share:pkey_map,
                secret_key:secret_share
            };
            for (id, sk_data) in config.sk_map.clone() {
                c.sec_key_map.insert(id.try_into().unwrap(), sk_data.clone());
            }
            if let Err(e) = c.run().await {
                log::error!("Consensus error: {}", e);
            }
        });
        Ok(exit_tx)
    }

    pub async fn broadcast(&mut self, protmsg:ProtMsg){
        let sec_key_map = self.sec_key_map.clone();
        for (replica,sec_key) in sec_key_map.into_iter() {
            if replica != self.myid{
                let wrapper_msg = WrapperMsg::new(protmsg.clone(), self.myid, &sec_key.as_slice());
                let cancel_handler:CancelHandler<Acknowledgement> = self.net_send.send(replica, wrapper_msg).await;
                self.add_cancel_handler(cancel_handler);
            }
        }
    }

    pub fn add_cancel_handler(&mut self, canc: CancelHandler<Acknowledgement>){
        self.cancel_handlers
            .entry(self.leader_round+self.baa_round)
            .or_default()
            .push(canc);
    }

    pub async fn send(&mut self,replica:Replica, wrapper_msg:WrapperMsg){
        let cancel_handler:CancelHandler<Acknowledgement> = self.net_send.send(replica, wrapper_msg).await;
        self.add_cancel_handler(cancel_handler);
    }

    pub async fn run(&mut self)-> Result<()>{
        let cancel_handler = self.sync_send.send(0,SyncMsg { sender: self.myid as usize, state: SyncState::ALIVE,value:0}).await;
        self.add_cancel_handler(cancel_handler);
        loop {
            tokio::select! {
                // Receive exit handlers
                exit_val = &mut self.exit_rx => {
                    exit_val.map_err(anyhow::Error::new)?;
                    log::info!("Termination signal received by the server. Exiting.");
                    break
                },
                msg = self.net_recv.recv() => {
                    // Received a protocol message
                    log::debug!("Got a consensus message from the network: {:?}", msg);
                    let msg = msg.ok_or_else(||
                        anyhow!("Networking layer has closed")
                    )?;
                    self.process_msg(msg).await;
                },
                sync_msg = self.sync_recv.recv() =>{
                    let sync_msg = sync_msg.ok_or_else(||
                        anyhow!("Networking layer has closed")
                    )?;
                    match sync_msg.state {
                        SyncState::START =>{
                            log::error!("Consensus Start time: {:?}", SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis());
                            // With round number
                            let msg = Msg{ 
                                value: self.input_val as u64, 
                                origin: self.myid, 
                                round: 0 as u64, 
                                rnd_estm: false, 
                                message: Vec::new()
                            };
                            self.start_rbc(msg).await;
                            let cancel_handler = self.sync_send.send(0, SyncMsg { sender: self.myid as usize, state: SyncState::STARTED, value:0}).await;
                            self.add_cancel_handler(cancel_handler);
                        },
                        SyncState::STOP =>{
                            log::error!("Consensus Stop time: {:?}", SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis());
                            log::info!("Termination signal received by the server. Exiting.");
                            break
                        },
                        _=>{}
                    }
                },
            };
        }
        Ok(())
    }
}

pub fn to_socket_address(
    ip_str: &str,
    port: u16,
) -> SocketAddr {
    let addr = SocketAddrV4::new(ip_str.parse().unwrap(), port);
    addr.into()
}