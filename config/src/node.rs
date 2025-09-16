use serde::{
    Serialize,
    Deserialize
};
use types::{Replica, Val};
use crypto::Algorithm;
use fnv::FnvHashMap as HashMap;
use super::{
    ParseError,
    is_valid_replica
};
use std::fs::File;
use std::io::prelude::*;
use std::net::{SocketAddr, SocketAddrV4};
use avsss::{PublicKey, SecretKey};
use crypto::dilithum_sig::PublicKey as DilithiumPublicKey;
use crypto::dilithum_sig::SecretKey as DilithiumSecretKey;
use serde_json::from_reader;
use toml::from_str;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Node {
    /// Node network config
    pub net_map_delphi: HashMap<Replica, String>,
    pub net_map_rbc: HashMap<Replica, String>,
    pub net_map_dkg: HashMap<Replica, String>,
    pub net_map_drb: HashMap<Replica, String>,

    /// Protocol details
    pub delay: u64,
    pub delphi: DelphiParams,
    pub acs: ACSParams,
    pub dkg: DKGParams,
    pub drb: DRBParams,

    pub id: Replica,
    pub num_nodes: usize,
    pub num_faults: usize,
    pub block_size:usize,
    pub client_port: u16,
    pub client_addr: SocketAddr,
    pub payload: usize,

    pub prot_payload: String,
    /// Crypto primitives
    pub crypto_alg: Algorithm,
    pub pk_map: HashMap<Replica, Vec<u8>>,
    pub secret_key_bytes: Vec<u8>,
    /// For authenticated channels
    pub sk_map: HashMap<Replica,Vec<u8>>,

    /// OpenSSL Certificate Details
    pub my_cert: Vec<u8>,
    pub my_cert_key: Vec<u8>,
    pub root_cert: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DelphiParams{
    pub delta: Val,
    pub epsilon: Val,
    pub tri: Val,
    pub expo: f32,
    pub high_val: Val,
    pub low_val: Val,
}

impl Default for DelphiParams {
    fn default() -> Self {
        DelphiParams {
            delta: 10,
            epsilon: 1,
            tri: 100000,
            expo: 2.0,
            high_val: 10000,
            low_val: 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ACSParams{
    pub kappa: usize,
}
impl Default for ACSParams {
    fn default() -> Self {
        ACSParams {
            kappa: 2,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DKGParams{
    pub pks: Vec<(i32,PublicKey)> ,
    pub sk: SecretKey,
    pub sig_pks: Vec<(i32, DilithiumPublicKey)>,
    pub sig_sk: DilithiumSecretKey,

    pub trans_waiting_time: u64
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DRBParams{
    pub batch: usize,
    pub frequency: u32
}

impl Default for DRBParams {
    fn default() -> Self {
        DRBParams{
            batch: 50,
            frequency: 25
        }
    }
}

impl Node {
    pub fn validate(&self) -> Result<(), ParseError> {
        if self.net_map_delphi.len() != self.num_nodes {
            return Err(ParseError::InvalidMapLen(self.num_nodes, self.net_map_delphi.len()));
        }
        if 2*self.num_faults >= self.num_nodes {
            return Err(ParseError::IncorrectFaults(self.num_faults, self.num_nodes));
        }
        // for repl in &self.net_map {
        //     if !is_valid_replica(*repl.0, self.num_nodes) {
        //         return Err(ParseError::InvalidMapEntry(*repl.0));
        //     }
        // }
        match self.crypto_alg {
            Algorithm::ED25519 => {
                for repl in &self.pk_map {
                    if !is_valid_replica(*repl.0, self.num_nodes) {
                        return Err(ParseError::InvalidMapEntry(*repl.0));
                    }
                    if repl.1.len() != crypto::ED25519_PK_SIZE {
                        return Err(ParseError::InvalidPkSize(repl.1.len()));
                    }
                }
                if self.secret_key_bytes.len() != crypto::ED25519_PVT_SIZE {
                    return Err(ParseError::InvalidSkSize(self.secret_key_bytes.len()));
                }
            }
            Algorithm::SECP256K1 => {
                for repl in &self.pk_map {
                    if !is_valid_replica(*repl.0, self.num_nodes) {
                        return Err(ParseError::InvalidMapEntry(*repl.0));
                    }
                    if repl.1.len() != crypto::SECP256K1_PK_SIZE {
                        return Err(ParseError::InvalidPkSize(repl.1.len()));
                    }
                }
                if self.secret_key_bytes.len() != crypto::SECP256K1_PVT_SIZE {
                    return Err(ParseError::InvalidSkSize(self.secret_key_bytes.len()));
                }
            }
            Algorithm::RSA => {
                // Because unimplemented
                return Err(ParseError::Unimplemented("RSA"));
            }
            Algorithm::NOPKI => {
                // In case of No PKI, use secret keys
                for repl in &self.sk_map {
                    if !is_valid_replica(*repl.0, self.num_nodes) {
                        return Err(ParseError::InvalidMapEntry(*repl.0));
                    }
                    if repl.1.len() != crypto::SECRET_KEY_SIZE {
                        return Err(ParseError::InvalidPkSize(repl.1.len()));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn new() -> Node {
        Node{
            block_size: 0,
            client_port: 0,
            client_addr: SocketAddrV4::new("0.0.0.0".parse().unwrap(),5000).into(),
            crypto_alg: Algorithm::ED25519,
            delay: 50,
            delphi: DelphiParams::default(),
            acs: ACSParams::default(),
            dkg: DKGParams::default(),
            drb: DRBParams::default(),
            id: 0,
            net_map_delphi: HashMap::default(),
            net_map_rbc: HashMap::default(),
            net_map_dkg: HashMap::default(),
            net_map_drb: HashMap::default(),
            num_faults: 0,
            num_nodes: 0,
            pk_map: HashMap::default(),
            secret_key_bytes: Vec::new(),
            sk_map: HashMap::default(),
            payload: 0,
            prot_payload: String::new(),
            my_cert: Vec::new(),
            root_cert:Vec::new(),
            my_cert_key: Vec::new(),
        }
    }

    pub fn from_json(filename:String) -> Node {
        let f = File::open(filename)
            .unwrap();
        let c: Node = from_reader(f)
            .unwrap();
        return c;
    }

    pub fn from_toml(filename:String) -> Node {
        let mut buf = String::new();
        let mut f = File::open(filename)
            .unwrap();
        f.read_to_string(&mut buf)
            .unwrap();
        let c:Node = from_str(&buf)
            .unwrap();
        return c;
    }

    pub fn from_yaml(filename:String) -> Node {
        let f = File::open(filename)
            .unwrap();
        let c:Node = serde_yaml::from_reader(f)
            .unwrap();
        return c;
    }

    pub fn from_bin(filename:String) -> Node {
        let mut buf = Vec::new();
        let mut f = File::open(filename)
            .unwrap();
        f.read_to_end(&mut buf)
            .unwrap();
        let bytes:&[u8] = &buf;
        let c:Node = bincode::deserialize(bytes)
            .unwrap();
        return c;
    }

    pub fn update_config(&mut self, ips: Vec<String>) {
        let mut idx = 0;
        let max_nodes = self.num_nodes;
        for ip in ips {
            // For self ip, put 0.0.0.0 with the same port
            if idx == max_nodes{
                // Syncer address
                let ip_a:Vec<&str> = ip.split(":").collect();
                let port:u16 = ip_a.last().expect("invalid ip found").parse().expect("failed to parse port number");
                let sock_addr = SocketAddrV4::new(ip_a.get(0).unwrap().parse().unwrap(), port);
                self.client_addr = sock_addr.into();
            }
            if idx == self.id {
                let port:u16 = ip.split(":")
                    .last()
                    .expect("invalid ip found; unable to split at :")
                    .parse()
                    .expect("failed to parse the port after :");
                self.net_map_delphi.insert(idx, format!("0.0.0.0:{}", port));
                idx += 1;
                continue;
            }
            // Put others ips in the config
            self.net_map_delphi.insert(idx, ip);
            idx += 1;
        }
        log::info!("Talking to servers: {:?}", self.net_map_delphi);
    }

    pub fn my_ip(&self) -> String {
        // Small string, so it is okay to clone
        self.net_map_delphi.get(&self.id)
            .expect("Failed to obtain IP for self. Incorrect config file.")
            .clone()
    }

    /// Returns the address at which a server should listen to incoming client
    /// connections
    pub fn client_ip(&self) -> String {
        format!("0.0.0.0:{}", self.client_port)
    }
}