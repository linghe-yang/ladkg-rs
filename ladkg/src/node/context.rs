use crate::node::handler::Handler;
use anyhow::anyhow;
use avsss::components::{PrivateShare, PublicShare, R_SIGMA, SharingStore, X_LEN, Y_LEN, decrypt, is_merkle_valid, share, supple_share, try_decrypt, BETA, X_SIGMA, Y_SIGMA};
use avsss::r_ring::{N, R};
use avsss::shamir::{shamir_reconstruct};
use avsss::util::generate_r_matrix;
use avsss::{PublicKey, SecretKey, Store, calculate_u, euclidean_norm, vector_add_mod_p};
use config::Node;
use crypto::Algorithm;
use crypto::dilithum_sig::SecretKey as DilithiumSecretKey;
use crypto::dilithum_sig::{PublicKey as DilithiumPublicKey, Signature};
use fnv::FnvHashMap;
use hashrand::node::HashRand;
use hrconfig::Node as HashRandConfig;
use hrcrypto::Algorithm as HashRandAlgorithm;
use log::{error, info};
use nalgebra::DVector;
use network::Acknowledgement;
use network::plaintcp::{CancelHandler, TcpReceiver, TcpReliableSender};
use rand::SeedableRng;
use rand::rngs::{OsRng, StdRng};
use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use avsss::shamir_r::shamir_reconstruct_r;
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, channel, unbounded_channel};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::sleep;
use types::dkg::msg::{VSSMsg, VSSWrapperMsg};
use types::dkg::trans::Transcript;
use types::{Replica, Round, SyncMsg, SyncState};
use crate::node::sync_handler::SyncHandler;

pub struct Context {
    pub net_send: TcpReliableSender<Replica, VSSWrapperMsg, Acknowledgement>,
    pub net_recv: UnboundedReceiver<VSSWrapperMsg>,
    pub sync_send:TcpReliableSender<Replica,SyncMsg,Acknowledgement>,
    pub sync_recv: UnboundedReceiver<SyncMsg>,

    pub myid: usize,
    pub sec_key_map: HashMap<Replica, Vec<u8>>,
    pub num_nodes: usize,
    pub num_faults: usize,
    pub trans_waiting_time: u64,
    pub round: Round,

    pub pks: Vec<(i32, PublicKey)>,
    pub sk: SecretKey,

    pub sig_pks: Vec<(i32, DilithiumPublicKey)>,
    pub sig_sk: DilithiumSecretKey,

    pub store: SharingStore,
    pub th_sk: DVector<R>,
    pub th_pk_shares: HashMap<Replica, R>,
    pub th_pk: R,

    pub pub_shares: HashMap<Replica, PublicShare>,
    pub unvalidated_shares: HashMap<Replica, PrivateShare>,
    pub validated_my_share: HashMap<Replica, DVector<R>>,

    pub transcripts: Arc<RwLock<HashMap<Replica, Transcript>>>,
    pub trans_tx: Sender<Transcript>,
    pub acs_res_rx: Receiver<Vec<Transcript>>,

    pub certificate: Arc<RwLock<HashMap<Replica, Signature>>>,

    pub decided_trans: Vec<Transcript>,
    pub coin_req: Sender<u32>,
    pub coin_recv: Receiver<(u32,u128)>,

    pub salt: HashMap<u32, u128>,

    pub cancel_handlers: HashMap<Round, Vec<CancelHandler<Acknowledgement>>>,
    exit_rx: oneshot::Receiver<()>,

    counting_down: bool
}

impl Context {
    pub fn spawn(config: Node) -> anyhow::Result<(oneshot::Sender<()>, oneshot::Sender<()>, oneshot::Sender<()>)> {
        let exit_tx_acs;
        let exit_tx_hr;

        let prot_payload = &config.prot_payload;
        let v: Vec<&str> = prot_payload.split(',').collect();
        let mut consensus_addrs: FnvHashMap<Replica, SocketAddr> = FnvHashMap::default();
        for (replica, address) in config.net_map_dkg.iter() {
            let address: SocketAddr = address.parse().expect("Unable to parse address");
            consensus_addrs.insert(*replica, SocketAddr::from(address.clone()));
        }
        let my_port = consensus_addrs.get(&config.id).unwrap();
        let my_address = to_socket_address("0.0.0.0", my_port.port());


        let mut syncer_map:FnvHashMap<Replica,SocketAddr> = FnvHashMap::default();
        syncer_map.insert(0, config.client_addr);
        let syncer_listen_port = config.client_port;
        let syncer_l_address = to_socket_address("0.0.0.0", syncer_listen_port);
        let (tx_net_to_client,rx_net_from_client) = unbounded_channel();
        TcpReceiver::<Acknowledgement,SyncMsg,_>::spawn(
            syncer_l_address,
            SyncHandler::new(tx_net_to_client)
        );
        let sync_net = TcpReliableSender::<Replica,SyncMsg,Acknowledgement>::with_peers(syncer_map);

        let (tx_net_to_consensus, rx_net_to_consensus) = unbounded_channel();
        TcpReceiver::<Acknowledgement, VSSWrapperMsg, _>::spawn(
            my_address,
            Handler::new(tx_net_to_consensus),
        );

        let consensus_net =
            TcpReliableSender::<Replica, VSSWrapperMsg, Acknowledgement>::with_peers(
                consensus_addrs.clone(),
            );
        info!("DKG Consensus addrs {:?}", consensus_addrs);

        let (coin_construct, coin_const_recv) = mpsc::channel(10000);
        let (coin_send, mut coin_recv) = channel::<(u32,u128)>(10000);

        let (trans_tx, trans_rx) = channel(10);
        let (acs_res_tx, acs_res_rx) = channel(10);

        let acs_config = config.clone();
        let hr_config = gen_hashrand_config(&config);


        let (coin_to_acs_tx, coin_to_acs_rx) = channel(10000);
        let (coin_to_dkg_tx, coin_to_dkg_rx) = channel(10000);
        tokio::spawn(async move {
            while let Some(beacon) = coin_recv.recv().await {
                if beacon.0 == 10 {
                    coin_to_acs_tx.send(beacon).await.expect("fail to send beacon");
                }else if beacon.0 == 20 {
                    coin_to_dkg_tx.send(beacon).await.expect("fail to send beacon");
                }
            }
        });

        exit_tx_acs =
            acs::node::Context::spawn(acs_config, coin_construct.clone(), coin_to_acs_rx, trans_rx, acs_res_tx)?;

        exit_tx_hr = HashRand::spawn(
            hr_config,
            0,
            config.drb.batch,
            config.drb.frequency,
            coin_const_recv,
            coin_send,
        )?;


        if v[0] == "a" {
            let (exit_tx, exit_rx) = oneshot::channel();
            let coin_req = coin_construct.clone();

            tokio::spawn(async move {
                let mut c = Context {
                    net_send: consensus_net,
                    net_recv: rx_net_to_consensus,
                    sync_send: sync_net,
                    sync_recv: rx_net_from_client,
                    num_nodes: config.num_nodes,
                    trans_waiting_time: config.dkg.trans_waiting_time,
                    sec_key_map: HashMap::default(),
                    myid: config.id,
                    num_faults: config.num_faults,
                    round: 0,

                    pks: config.dkg.pks,
                    sk: config.dkg.sk,
                    sig_pks: config.dkg.sig_pks,
                    sig_sk: config.dkg.sig_sk,
                    store: SharingStore::default(),
                    th_sk: DVector::from_fn(1, |_,_| R::default()),
                    th_pk_shares: HashMap::new(),
                    th_pk: R::default(),

                    pub_shares: HashMap::new(),
                    unvalidated_shares: HashMap::new(),
                    validated_my_share: HashMap::new(),
                    certificate: Arc::new(RwLock::new(HashMap::default())),

                    transcripts: Arc::new(RwLock::new(HashMap::default())),
                    decided_trans: Vec::new(),
                    trans_tx,
                    acs_res_rx,

                    coin_req,
                    coin_recv: coin_to_dkg_rx,
                    salt: HashMap::new(),

                    exit_rx,
                    cancel_handlers: HashMap::default(),
                    counting_down: false
                };
                for (id, sk_data) in config.sk_map.clone() {
                    c.sec_key_map.insert(id, sk_data.clone());
                }
                //c.invoke_coin.insert(100, Duration::from_millis(sleep_time.try_into().unwrap()));
                if let Err(e) = c.run().await {
                    log::error!("DKG Consensus error:{}", e);
                }
                log::debug!("Started n-parallel RBC");
                // Initialize storage
            });



            Ok((exit_tx,exit_tx_acs,exit_tx_hr))
        } else {
            panic!("Invalid configuration for protocol");
        }


    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        let cancel_handler = self.sync_send.send(
            0,
            SyncMsg { sender: self.myid, state: SyncState::ALIVE}).await;
        self.add_cancel_handler(cancel_handler);

        loop {
            tokio::select! {
                // Handle exit signal
                result = &mut self.exit_rx => {
                    result.map_err(|_| anyhow!("Exit channel closed"))?;
                    info!("Termination signal received. Exiting.");
                    break;
                }
                // Handle network messages
                Some(msg) = self.net_recv.recv() => {
                    self.handle_network_message(msg).await?;
                }

                sync_msg = self.sync_recv.recv() =>{
                    let sync_msg = sync_msg.ok_or_else(||
                        anyhow!("Networking layer has closed")
                    )?;
                    match sync_msg.state {
                        SyncState::StartVSS =>{
                            info!("DKG Start time: {:?}", SystemTime::now()
                                .duration_since(UNIX_EPOCH)?
                                .as_millis());
                            self.sharing_phase().await?;
                        },
                        SyncState::STOP =>{
                            log::error!("DKG Stop time: {:?}", SystemTime::now()
                                .duration_since(UNIX_EPOCH)?
                                .as_millis());
                            info!("Termination signal received by the server. Exiting.");
                            break
                        },
                        _=>{}
                    }
                }

                Some(res) = self.acs_res_rx.recv() => {
                    self.handle_acs_result(res).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn sharing_phase(&mut self) -> Result<(), anyhow::Error>{
        let mut rng = StdRng::from_rng(OsRng)?;
        let secret =
            DVector::from_iterator(X_LEN, (0..X_LEN).map(|_| R::random_gaussian(&mut rng, 1.0)));

        let (prs, pus, st) = share(secret, self.num_nodes, self.num_faults, &self.pks);

        self.store = st;
        for share in prs {
            let target = (share.id - 1) as Replica;
            let msg = VSSMsg::VSSPrivateShare(share, self.myid);
            self.p2p_send(msg, target).await;
        }
        self.pub_shares.entry(self.myid).or_insert(pus.clone());

        let pub_msg = VSSMsg::VSSPublicShare(pus, self.myid);
        self.broadcast(pub_msg).await;

        self.coin_req.send(20).await?;
        Ok(())
    }

    /// Handles incoming network messages
    async fn handle_network_message(&mut self, msg: VSSWrapperMsg) -> Result<(), anyhow::Error> {
        let msg = msg.protmsg;
        match msg {
            VSSMsg::VSSPrivateShare(my_share, sender) => {
                if !self.pub_shares.contains_key(&sender) {
                    self.unvalidated_shares.entry(sender).or_insert(my_share);
                } else {
                    self.verify_my_share(sender, my_share).await;
                }
            }
            VSSMsg::VSSPublicShare(pub_share, sender) => {
                if !self.verify_pub_share(&pub_share) {return Ok(())}
                self.pub_shares.entry(sender).or_insert(pub_share.clone());
                if self.unvalidated_shares.contains_key(&sender) {
                    let my_share = self.unvalidated_shares.get(&sender).unwrap().clone();
                    self.verify_my_share(sender, my_share).await;
                    self.unvalidated_shares.remove(&sender);
                }
            }
            VSSMsg::VSSReply(sig, sender) => {
                if self.transcripts.read().await.contains_key(&self.myid) {
                    return Ok(());
                }
                let pus = self.pub_shares.get(&self.myid).unwrap();
                if let Some(pk) = self
                    .sig_pks
                    .iter()
                    .find(|(id, _)| *id == (sender + 1) as i32)
                {
                    if sig.verify(&pus.merkle_root, &pk.1).is_ok() {
                        let mut guard = self.certificate.write().await;
                        guard.entry(sender).or_insert(sig);
                        if guard.len() >= (2 * self.num_faults + 1) && !self.counting_down {
                            self.gen_transcript();
                        }
                    }
                }
            }
            VSSMsg::DKGPubKey(pub_key, sender) => {
                if self.th_pk != R::default() {return Ok(())}
                self.th_pk_shares.entry(sender).or_insert(*pub_key);
                if self.th_pk_shares.len() >= (self.num_nodes - self.num_faults) {
                    let shares: Vec<(i32, R)> = self.th_pk_shares.clone()
                        .into_iter()
                        .map(|(k, v)| ((k+1) as i32, v))
                        .collect();
                    if let Some(b) = shamir_reconstruct_r(&shares, self.num_faults) {
                        self.th_pk = b;
                        let cancel_handler = self.sync_send.send(
                            0,
                            SyncMsg { sender: self.myid, state: SyncState::COMPLETED(Box::new(b))}).await;
                        self.add_cancel_handler(cancel_handler);
                        info!("th_pubkey shares constructed: {:?}", b);
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_acs_result(&mut self, res: Vec<Transcript>) -> Result<(), anyhow::Error> {
        let mut decided_trans = Vec::new();
        for trans in res.iter() {
            if trans.verify(&self.sig_pks, &self.pks, self.num_nodes) {
                decided_trans.push(trans.clone());
            }
        }

        for trans in decided_trans.iter() {
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.validated_my_share.entry(trans.id)
            {
                let my_supple = trans
                    .supple_shares
                    .iter()
                    .find(|s| s.id == (self.myid + 1) as i32)
                    .unwrap();
                let (_, pk) = self
                    .pks
                    .iter()
                    .find(|(id, _)| *id == (self.myid + 1) as i32)
                    .unwrap();

                if let Ok((x, _)) = try_decrypt(pk, &self.sk, my_supple) {
                    e.insert(x);
                } else {
                    panic!("Supplementary share decrypt failed");
                }
            }
        }
        let th_sk = self.sum_secret_key(&decided_trans).unwrap();
        self.th_sk = th_sk.clone();

        info!("acs finished");
        if let Some((_,coin)) = self.coin_recv.recv().await {
            let mut rng = create_thread_safe_rng(coin);
            let a = R::random_gaussian(&mut rng, 1f64);
            let beta = R::from(BETA);
            let temp = th_sk[0].add_mod_p(&a.mul_mod_p(&th_sk[1]));
            let b_i = beta.sub_mod_p(&temp);
            self.th_pk_shares.entry(self.myid).or_insert(b_i);
            let msg = VSSMsg::DKGPubKey(Box::new(b_i), self.myid);
            self.broadcast(msg).await;
            info!("my pk broadcast");
        }

        Ok(())
    }

    pub fn verify_pub_share(&self, pub_share: &PublicShare) -> bool{
        let u = shamir_reconstruct(&pub_share.u_vec, self.num_faults);
        if u.is_none() {
            return false;
        }
        let u = u.unwrap();
        let c = 1f64;
        let p1 = (X_LEN * N) as f64 * c * R_SIGMA * c * X_SIGMA;
        let p2 = (N as f64).sqrt() * c * Y_SIGMA;
        let b_prime = (Y_LEN as f64).sqrt() * (p1 + p2);
        let v_norm = euclidean_norm(&u);
        if v_norm > b_prime {
            error!("u false");
            return false;
        }
        true
    }

    pub async fn verify_my_share(&mut self, sender: Replica, my_share: PrivateShare) {
        let pub_share = self.pub_shares.get(&sender).unwrap();
        if !is_merkle_valid(
            pub_share.merkle_root,
            &my_share.v,
            &my_share.w,
            &my_share.merkle_proof,
        ) {
            return;
        }
        if let Ok((x, y)) = decrypt(&self.sk, &my_share) {
            let r = generate_r_matrix(pub_share.merkle_root, X_LEN, Y_LEN, R_SIGMA);
            let u_i = calculate_u(&r, &x, &y);
            if let Some((_, pub_u_i)) = pub_share
                .u_vec
                .iter()
                .find(|(i, _)| *i == (self.myid + 1) as i32)
            {
                if u_i != *pub_u_i {
                    return;
                } else {
                    self.validated_my_share.entry(sender).or_insert(x);
                }
            } else {
                return;
            }
        } else {
            return;
        }

        let pub_share = self.pub_shares.get(&sender).unwrap();
        info!("my share from {} is valid", sender);
        let sig = Signature::new(&pub_share.merkle_root, &self.sig_sk);
        let msg = VSSMsg::VSSReply(sig, self.myid);
        self.p2p_send(msg, sender).await;
    }

    pub fn gen_transcript(&self) {
        let num_nodes = self.num_nodes;
        let mut store = self.store.clone();
        let myid = self.myid;
        let pks = self.pks.clone();
        let pub_share = self.pub_shares.get(&myid).unwrap().clone();
        let trans_tx = self.trans_tx.clone();
        let delta = self.trans_waiting_time;

        let certificate = Arc::clone(&self.certificate);
        let transcripts = Arc::clone(&self.transcripts);

        tokio::spawn(async move {
            sleep(Duration::from_millis(delta)).await;
            let guard = certificate.read().await.clone();
            let cert: Vec<(Replica, Signature)> = guard.into_iter().collect();
            let supple_indices = find_missing_replicas(&cert, num_nodes);
            let missing_set: HashSet<i32> = supple_indices
                .into_iter()
                .filter_map(|r| i32::try_from(r + 1).ok()) // 瀹夊叏鍦板皢 usize 杞崲涓?i32
                .collect();
            let supple_ciphers: Vec<(i32, DVector<R>, DVector<R>, Store)> = store
                .ciphers
                .iter()
                .filter(|(i, _, _, _)| missing_set.contains(i))
                .cloned() // 鍋囪闇€瑕佸厠闅嗘暟鎹?
                .collect();

            store.ciphers = supple_ciphers;
            let supple_shares = supple_share(store, &pks);
            let trans = Transcript {
                id: myid,
                cert,
                pub_share,
                supple_shares,
            };

            transcripts.write().await.entry(myid).or_insert(trans.clone());
            info!("my transcript has formed");
            trans_tx.send(trans).await.unwrap();
        });
    }

    pub fn sum_secret_key(&self, decided_trans: &Vec<Transcript>) -> Option<DVector<R>> {
        let mut result: Option<DVector<R>> = None;
        for transcript in decided_trans {
            if let Some(vector) = self.validated_my_share.get(&transcript.id) {
                match result {
                    None => {
                        result = Some(vector.clone());
                    }
                    Some(ref mut current) => {
                        *current = vector_add_mod_p(current, vector);
                    }
                }
            } else {
                panic!("no share for aggregate");
            }
        }
        result
    }

    pub async fn broadcast(&mut self, protmsg: VSSMsg) {
        let sec_key_map = self.sec_key_map.clone();
        for (replica, sec_key) in sec_key_map.into_iter() {
            let wrapper_msg = VSSWrapperMsg::new(protmsg.clone(), self.myid, &sec_key.as_slice());
            let cancel_handler: CancelHandler<Acknowledgement> =
                self.net_send.send(replica, wrapper_msg).await;
            self.add_cancel_handler(cancel_handler);
        }
    }

    pub async fn p2p_send(&mut self, protmsg: VSSMsg, target: Replica) {
        let sec_key_map = self.sec_key_map.clone();
        let sec_key = sec_key_map.iter().find(|(r, _s)| **r == target).unwrap();
        let wrapper_msg = VSSWrapperMsg::new(protmsg.clone(), self.myid, sec_key.1.as_slice());
        let cancel_handler: CancelHandler<Acknowledgement> =
            self.net_send.send(target, wrapper_msg).await;
        self.add_cancel_handler(cancel_handler);
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

fn find_missing_replicas(vec: &Vec<(Replica, Signature)>, n: usize) -> Vec<Replica> {
    let replicas: HashSet<usize> = vec.iter().map(|(replica, _)| *replica).collect();

    (0..n).filter(|i| !replicas.contains(i)).collect()
}

fn gen_hashrand_config(config: &Node) -> HashRandConfig {
    HashRandConfig {
        net_map: config.net_map_drb.clone(),
        delta: config.delay,
        id: config.id,
        num_nodes: config.num_nodes,
        num_faults: config.num_faults,
        block_size: config.block_size,
        client_port: config.client_port,
        client_addr: config.client_addr,
        payload: config.payload,
        prot_payload: "cc,123".to_string(),
        crypto_alg: trans_hashrand_algorithm(&config.crypto_alg),
        pk_map: config.pk_map.clone(),
        secret_key_bytes: config.secret_key_bytes.clone(),
        sk_map: config.sk_map.clone(),
        my_cert: config.my_cert.clone(),
        my_cert_key: config.my_cert_key.clone(),
        root_cert: config.root_cert.clone(),
    }
}

fn trans_hashrand_algorithm(alg: &Algorithm) -> HashRandAlgorithm {
    match alg {
        Algorithm::RSA => HashRandAlgorithm::RSA,
        Algorithm::ED25519 => HashRandAlgorithm::ED25519,
        Algorithm::NOPKI => HashRandAlgorithm::NOPKI,
        Algorithm::SECP256K1 => HashRandAlgorithm::SECP256K1,
    }
}

pub fn create_thread_safe_rng(seed: u128) -> StdRng {
    let seed_bytes: [u8; 16] = seed.to_le_bytes();
    let mut full_seed: [u8; 32] = [0; 32];
    full_seed[..16].copy_from_slice(&seed_bytes);
    StdRng::from_seed(full_seed)
}