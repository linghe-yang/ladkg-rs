#![allow(warnings)]

use anyhow::{Result};
use clap::{
    load_yaml, 
    App
};
use config::Node;
use signal_hook::{iterator::Signals, consts::{SIGINT, SIGTERM}};
use types::{Val};
use std::{net::{SocketAddr, SocketAddrV4}};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use env_logger::Env;
use fnv::FnvHashMap;
use log::{info};
use tokio::sync::mpsc::{channel};
use node::Syncer;
use types::dkg::trans::Transcript;


#[tokio::main]
async fn main() -> Result<()> {
    let mut logger = env_logger::Builder::from_env(Env::default().default_filter_or("info"));

    logger.format_timestamp_millis();
    logger.init();

    log::error!("{}", std::env::current_dir().unwrap().display());
    let yaml = load_yaml!("cli.yml");
    let m = App::from_yaml(yaml).get_matches();
    let conf_str = m.value_of("config")
        .expect("unable to convert config file into a string");
    let vss_type = m.value_of("vsstype")
        .expect("Unable to detect VSS type");
    let _sleep = m.value_of("sleep")
        .expect("Unable to detect sleep time").parse::<u128>().unwrap();
    let _batch = m.value_of("batch")
        .expect("Unable to parse batch size").parse::<usize>().unwrap();
    // let val_appx = m.value_of("val")
    //     .expect("Value required");
    // let val_appx = parse_val_string(val_appx).unwrap();
    let syncer_file = m.value_of("syncer")
        .expect("Unable to parse syncer ip file");
    let _rand = m.value_of("rand")
        .expect("Unable to parse random number").parse::<usize>().unwrap();
    let conf_file = std::path::Path::new(conf_str);
    let str = String::from(conf_str);
    let config = match conf_file
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

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time error")
        .as_millis();

    if _sleep > now {
        let duration_ms = (_sleep - now) as u64;
        let sleep_duration = Duration::from_millis(duration_ms);

        // 线程休眠
        tokio::time::sleep(sleep_duration).await;
    }

    // simple_logger::SimpleLogger::new().with_utc_timestamps().init().unwrap();
    //
    // log::set_max_level(log::LevelFilter::Info);

    // match m.occurrences_of("debug") {
    //     0 => log::set_max_level(log::LevelFilter::Info),
    //     1 => log::set_max_level(log::LevelFilter::Debug),
    //     2 | _ => log::set_max_level(log::LevelFilter::Trace),
    // }
    // log::info!("epsilon: {:?},delta: {:?},value: {:?}, tri:{:?}",config.delphi.epsilon,config.delphi.delta,val_appx,config.delphi.tri);
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
    // let exit_tx_vec ;
    let exit_tx_dkg;
    let exit_tx_acs;
    let sync_exit_tx;


    match vss_type{
        // "del" =>{
        //     let (val_tx, val_rx) = channel(160);
        //
        //     let (res_tx, mut res_rx) = channel(160);
        //     exit_tx_vec = delphi::node::Delphi::spawn(config, val_rx, Arc::new(res_tx))?;
        //
        //     for (i,value) in val_appx.iter().enumerate() {
        //         let inst_id = i;
        //         val_tx.send((inst_id, *value)).await?;
        //     }
        //
        //     while let Some((inst_id,value)) = res_rx.recv().await {
        //         log::info!("Delphi inst: {} has terminated with value: {}", inst_id, value);
        //     }
        // },

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

        "sync" => {
            print_params(&config);
            let f_str = syncer_file.to_string();
            info!("Logging the file f {}",f_str);
            let ip_str = util::io::file_to_ips(f_str);
            let mut net_map = FnvHashMap::default();
            let mut idx = 0;
            for ip in ip_str{
                net_map.insert(idx, ip.clone());
                idx += 1;
            }
            //let client_addr = net_map.get(&(net_map.len()-1)).unwrap();
            sync_exit_tx = Syncer::spawn(net_map, config.client_addr.clone())?;
        },
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


fn print_params(config: &Node){
    info!("delta: {}", config.delphi.delta);
    info!("epsilon: {}", config.delphi.epsilon);
    info!("tri: {}", config.delphi.tri);
    info!("kappa: {}", config.acs.kappa);
    info!("trans_waiting_time: {}", config.dkg.trans_waiting_time);
    info!("hashrand batch: {}", config.drb.batch);
    info!("hashrand frequency: {}", config.drb.frequency);

}