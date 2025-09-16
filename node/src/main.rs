use anyhow::{Result,anyhow};
use clap::{
    load_yaml, 
    App
};
use config::Node;
use fnv::FnvHashMap;
use node::Syncer;
use signal_hook::{iterator::Signals, consts::{SIGINT, SIGTERM}};
use types::{Replica, SyncMsg, Val};
use std::{net::{SocketAddr, SocketAddrV4}};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::thread::{sleep, sleep_ms};
use std::time::Duration;
use futures::channel::mpsc::Sender;
use network::Acknowledgement;
use network::plaintcp::TcpReceiver;
use rand::prelude::SliceRandom;
use tokio::sync::mpsc::{channel, unbounded_channel, UnboundedReceiver, UnboundedSender};
use node::sync_handler::SyncHandler;
use types::appxcon::WrapperMsg;
use rand::rngs::StdRng;
use rand::SeedableRng;
use types::dkg::trans::Transcript;

use hashrand::node::HashRand;
use hrconfig::Node as HashRandConfig;
use hrcrypto::{Algorithm as HashRandAlgorithm};
use log::info;
use tokio::sync::oneshot;
use crypto::Algorithm;

#[tokio::main]
async fn main() -> Result<()> {
    log::error!("{}", std::env::current_dir().unwrap().display());
    let yaml = load_yaml!("cli.yml");
    let m = App::from_yaml(yaml).get_matches();
    //println!("{:?}",m);
    let conf_str = m.value_of("config")
        .expect("unable to convert config file into a string");
    let vss_type = m.value_of("vsstype")
        .expect("Unable to detect VSS type");
    let sleep = m.value_of("sleep")
        .expect("Unable to detect sleep time").parse::<u128>().unwrap();
    let _batch = m.value_of("batch")
        .expect("Unable to parse batch size").parse::<usize>().unwrap();
    let val_appx = m.value_of("val")
        .expect("Value required");
    let mut val_appx = parse_val_string(val_appx).unwrap();
    
    // let delta = m.value_of("delta")
    //     .expect("Value required").parse::<Val>().unwrap();
    // let epsilon = m.value_of("epsilon")
    //     .expect("Value required").parse::<Val>().unwrap();
    // let tri = m.value_of("tri")
    //     .expect("Value required").parse::<Val>().unwrap();
    let syncer_file = m.value_of("syncer")
        .expect("Unable to parse syncer ip file");
    let rand = m.value_of("rand")
        .expect("Unable to parse random number").parse::<usize>().unwrap();
    // let expo = m.value_of("expo")
    //     .expect("Unable to parse exponent").parse::<f32>().unwrap();
    let conf_file = std::path::Path::new(conf_str);
    let str = String::from(conf_str);
    let mut config = match conf_file
        .extension()
        .expect("Unable to get file extension")
        .to_str()
        .expect("Failed to convert the extension into ascii string") 
    {
        "json" => Node::from_json(str),
        "dat" => Node::from_bin(str),
        "toml" => Node::from_toml(str),
        "yaml" => Node::from_yaml(str),
        _ => panic!("Invalid config file extension"),
    };

    simple_logger::SimpleLogger::new().with_utc_timestamps().init().unwrap();
    // match m.occurrences_of("debug") {
    //     0 => log::set_max_level(log::LevelFilter::Info),
    //     1 => log::set_max_level(log::LevelFilter::Debug),
    //     2 | _ => log::set_max_level(log::LevelFilter::Trace),
    // }
    // log::info!("epsilon: {:?},delta: {:?},value: {:?}, tri:{:?}",config.delphi.epsilon,config.delphi.delta,val_appx,config.delphi.tri);
    log::set_max_level(log::LevelFilter::Info);
    // config
    //     .validate()
    //     .expect("The decoded config is not valid");
    // if let Some(f) = m.value_of("ip") {
    //     let f_str = f.to_string();
    //     log::info!("Logging the file f {}",f_str);
    //     config.update_config(util::io::file_to_ips(f.to_string()));
    // }
    // let config = config;
    // Start the Reliable Broadcast protocol
    let mut exit_tx_vec ;
    let mut exit_tx_dkg;
    let mut exit_tx_acs;


    match vss_type{
        // "ped" =>{
        //     //exit_tx = pedavss_cc::node::Context::spawn(config,sleep).unwrap();
        // },
        // "fre" => {
        //     //exit_tx = hash_cc::node::Context::spawn(config,sleep).unwrap();
        // },
        // "hr" => {
        //     //exit_tx = hash_cc_baa::node::Context::spawn(config,sleep,batch).unwrap();
        // },
        // "appx" => {
        //     exit_tx = appxcon::node::Context::spawn(config, sleep, val_appx as u64,epsilon as u64).unwrap();
        // },
        // "hyb" =>{
        //     exit_tx = hyb_appxcon::node::Context::spawn(config,sleep,val_appx as u64,delta as u64,epsilon as u64,tri as u64).unwrap();
        // },
        "del" =>{
            let (val_tx, val_rx) = channel(160);

            let (res_tx, mut res_rx) = channel(160);
            exit_tx_vec = delphi::node::Delphi::spawn(config, val_rx, Arc::new(res_tx))?;

            for (i,value) in val_appx.iter().enumerate() {
                let inst_id = i;
                val_tx.send((inst_id, *value)).await?;
            }

            while let Some((inst_id,value)) = res_rx.recv().await {
                log::info!("Delphi inst: {} has terminated with value: {}", inst_id, value);
            }
        },

        "acs" =>{
            let trans = Transcript::default();

            let (coin_req_tx, mut coin_req_rx) = channel(10);
            let (coin_recv_tx, coin_recv_rx) = channel(10);
            let (trans_tx, trans_rx) = channel(10);
            let (acs_res_tx, mut acs_res_rx) = channel(10);
            tokio::spawn(async move {
                while let Some(coin_seq_num) = coin_req_rx.recv().await {
                    coin_recv_tx.send((coin_seq_num, 20)).await.expect("fail to send coin result");
                }
            });
            exit_tx_acs = acs::node::Context::spawn(config,coin_req_tx,coin_recv_rx,trans_rx, acs_res_tx)?;

            trans_tx.send(trans).await?;
            while let Some(acs) = acs_res_rx.recv().await {
                log::info!("ACS result: {:?}", acs);
            }
        },

        "dkg" => {
            exit_tx_dkg = dkg::node::Context::spawn(config)?;
        },

        // "hashrand" => {
        //     let (coin_construct, coin_const_recv) = channel(10000);
        //     let (coin_send, mut coin_recv) = channel(10000);
        //     let hr_config = gen_hashrand_config(&config);
        //     let _exit_tx_hr = HashRand::spawn(
        //         hr_config,
        //         0,
        //         config.drb.batch,
        //         config.drb.frequency,
        //         coin_const_recv,
        //         coin_send,
        //     )?;
        //     coin_construct.send(0).await?;
        //     coin_construct.send(10).await?;
        //     coin_construct.send(20).await?;
        //
        //
        //     while let Some(val) = coin_recv.recv().await {
        //         info!("hashrand output: {:?}", val);
        //     }
        // }


        // "delrbc" =>{
        //     exit_tx = delphi_rbc::node::Context::spawn(config,val_appx,epsilon,delta,tri,expo).unwrap();
        // },
        // "fin" =>{
        //     let rand = rand.to_string();
        //     let mut arr_strsplit:Vec<&str> = conf_str.split("/").collect();
        //     let id_str = ((config.id +1)).to_string();
        //     //let id_str_1  = ((config.id)).to_string();
        //     let key_str = "sec".to_string();
        //     
        //     let concat_str = key_str + &id_str;
        //     let _last_elem = arr_strsplit.pop();
        // 
        //     let mut vec_native = Vec::new();
        //     for i in 1..config.num_nodes+1{
        //         let pkey_str = "pub".to_string();
        //         let mut tpub = arr_strsplit.clone();
        //         let iter_str = pkey_str.clone()+ &(i.to_string());
        //         tpub.push(iter_str.as_str());
        //         vec_native.push(tpub.join("/"));
        //     }
        //     arr_strsplit.push(concat_str.as_str());
        //     println!("{:?} {:?}", arr_strsplit.join("/").as_str(), vec_native);
        //     exit_tx = fin::node::Context::spawn(
        //         config, 
        //         arr_strsplit.join("/").as_str(),
        //         vec_native,
        //         val_appx,
        //         rand
        //     ).unwrap();
        // },
        // "sync" => {
        //     let f_str = syncer_file.to_string();
        //     log::info!("Logging the file f {}",f_str);
        //     let ip_str = util::io::file_to_ips(f_str);
        //     let mut net_map = FnvHashMap::default();
        //     let mut idx = 0;
        //     for ip in ip_str{
        //         net_map.insert(idx, ip.clone());
        //         idx += 1;
        //     }
        //
        //     let n = config.num_nodes;
        //     let kappa = 2;
        //     let reps = select_set(n,kappa,20);
        //
        //     let mut sync_senders: HashMap<usize, UnboundedSender<SyncMsg>> = HashMap::new();
        //     let mut sync_receivers: HashMap<usize, UnboundedReceiver<SyncMsg>> = HashMap::new();
        //     for inst_id in reps.iter() {
        //         let (sync_tx, sync_rx) = unbounded_channel::<SyncMsg>();
        //         sync_senders.insert(*inst_id, sync_tx);
        //         sync_receivers.insert(*inst_id, sync_rx);
        //     }
        //
        //
        //     let new_sock_address = SocketAddrV4::new("0.0.0.0".parse().unwrap(), config.client_addr.clone().port());
        //     TcpReceiver::<Acknowledgement, SyncMsg, _>::spawn(
        //         std::net::SocketAddr::V4(new_sock_address),
        //         SyncHandler::new(sync_senders),
        //     );
        //
        //     for inst_id in reps.iter() {
        //         let rx_net_to_server = sync_receivers.remove(&inst_id).unwrap();
        //         let tx = Syncer::spawn(net_map.clone(), config.client_addr.clone(), rx_net_to_server, *inst_id).unwrap();
        //         exit_tx_vec.push(tx);
        //     }
        //
        //     // let tx = Syncer::spawn(net_map, config.client_addr.clone() ,0).unwrap();
        //     // exit_tx.push(tx);
        //     //let client_addr = net_map.get(&(net_map.len()-1)).unwrap();
        //     // for (id,mut tx) in exit_tx.iter().enumerate(){
        //     //     tx = &Syncer::spawn(net_map.clone(), config.client_addr.clone(), id + 1).unwrap();
        //     // }
        //     // exit_tx = Syncer::spawn(net_map, config.client_addr.clone()).unwrap();
        // },
        _ =>{
            log::error!("Matching VSS not provided {}, canceling execution",vss_type);
            return Ok(());
        }
    }



    //let exit_tx = pedavss_cc::node::Context::spawn(config).unwrap();
    // Implement a waiting strategy
    let mut signals = Signals::new(&[SIGINT, SIGTERM])?;
    signals.forever().next();
    log::error!("Received termination signal");
    // for tx in exit_tx_vec.lock() {
    //     tx.send(())
    //         .map_err(|_| anyhow!("Server already shut down"))?;
    // }
    // exit_tx
    //     .send(())
    //     .map_err(|_| anyhow!("Server already shut down"))?;
    log::error!("Shutting down server");
    Ok(())
}



pub fn to_socket_address(
    ip_str: &str,
    port: u16,
) -> SocketAddr {
    let addr = SocketAddrV4::new(ip_str.parse().unwrap(), port);
    addr.into()
}

fn parse_val_string(input: &str) -> Result<Vec<Val>, String> {
    let values = input
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty()) // Remove empty strings after trimming
        .map(|s| {
            s.parse::<Val>()
                .map_err(|e| format!("Failed to parse '{}' as i64: {}", s, e))
        })
        .collect::<Result<Vec<Val>, String>>()?;
    Ok(values)
}


// fn gen_hashrand_config(config: &Node) -> HashRandConfig {
//     HashRandConfig {
//         net_map: config.net_map_drb.clone(),
//         delta: config.delay,
//         id: config.id,
//         num_nodes: config.num_nodes,
//         num_faults: config.num_faults,
//         block_size: config.block_size,
//         client_port: config.client_port,
//         client_addr: config.client_addr,
//         payload: config.payload,
//         prot_payload: "cc,123".to_string(),
//         crypto_alg: trans_hashrand_algorithm(&config.crypto_alg),
//         pk_map: config.pk_map.clone(),
//         secret_key_bytes: config.secret_key_bytes.clone(),
//         sk_map: config.sk_map.clone(),
//         my_cert: config.my_cert.clone(),
//         my_cert_key: config.my_cert_key.clone(),
//         root_cert: config.root_cert.clone(),
//     }
// }
//
// fn trans_hashrand_algorithm(alg: &Algorithm) -> HashRandAlgorithm {
//     match alg {
//         Algorithm::RSA => HashRandAlgorithm::RSA,
//         Algorithm::ED25519 => HashRandAlgorithm::ED25519,
//         Algorithm::NOPKI => HashRandAlgorithm::NOPKI,
//         Algorithm::SECP256K1 => HashRandAlgorithm::SECP256K1,
//     }
// }
