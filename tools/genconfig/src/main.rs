// A tool that builds config files for all the nodes and the clients for the
// protocol.
#![allow(warnings)]
use avsss::{PublicKey, VE};
use clap::{load_yaml, App};
use config::{ACSParams, Client, DKGParams, DRBParams, DelphiParams, Node};
use crypto::dilithum_sig::generate_keypair;
use crypto::dilithum_sig::PublicKey as DilithiumPublicKey;
use crypto::Algorithm;
use crypto::{ed25519, secp256k1::{self, SecretKey}};
use fnv::FnvHashMap as HashMap;
use rand::Rng;
use std::{error::Error, fs::File, io::{BufWriter, Write}};
use std::net::SocketAddrV4;
use types::{Replica, Val};
use util::io::*;

#[cfg(not(feature = "remote"))]
fn main() -> Result<(), Box<dyn Error>> {
    let yaml = load_yaml!("cli.yml");
    let m = App::from_yaml(yaml).get_matches();
    let num_nodes:usize =  m.value_of("num_nodes")
        .expect("number of nodes not specified")
        .parse::<usize>()
        .expect("unable to convert number of nodes into a number");
    let num_faults:usize = match m.value_of("num_faults") {
        Some(x) => x.parse::<usize>()
            .expect("unable to convert number of faults into a number"),
        None => (num_nodes-1)/3,
    };
    let delta = m.value_of("delta")
        .expect("Value required").parse::<Val>().unwrap();
    let epsilon = m.value_of("epsilon")
        .expect("Value required").parse::<Val>().unwrap();
    let tri = m.value_of("tri")
        .expect("Value required").parse::<Val>().unwrap();
    let expo = m.value_of("expo")
        .expect("Unable to parse exponent").parse::<f32>().unwrap();
    let kappa:usize = match m.value_of("kappa") {
        Some(x) => x.parse::<usize>()
            .expect("unable to convert kappa into a number"),
        None => (num_nodes-1)/3 + 1,
    };

    let trans_delay:u64 = match m.value_of("trans_delay") {
        Some(x) => x.parse::<u64>()
            .expect("unable to convert trans_delay into a number"),
        None => 500,
    };

    let delay:u64 = m.value_of("delay")
        .expect("delay value not specified")
        .parse::<u64>()
        .expect("unable to parse delay value into a number");
    let base_port: u16 = m.value_of("base_port")
        .expect("base_port value not specified")
        .parse::<u16>()
        .expect("failed to parse base_port into a number");
    let rbc_base_port: u16 = m.value_of("rbc_base_port")
        .expect("base_port value not specified")
        .parse::<u16>()
        .expect("failed to parse base_port into a number");
    let dkg_base_port: u16 = m.value_of("dkg_base_port")
        .expect("base_port value not specified")
        .parse::<u16>()
        .expect("failed to parse base_port into a number");
    let drb_base_port: u16 = m.value_of("drb_base_port")
        .expect("base_port value not specified")
        .parse::<u16>()
        .expect("failed to parse base_port into a number");
    let blocksize: usize = m.value_of("block_size")
        .expect("no block_size specified")
        .parse::<usize>()
        .expect("unable to convert blocksize into a number");
    let client_base_port:u16 = m.value_of("client_base_port")
        .expect("no client_base_port specified")
        .parse::<u16>()
        .expect("unable to parse client_base_port into an integer");
    let t:Algorithm = m.value_of("algorithm")
        .unwrap_or("NOPKI")
        .parse::<Algorithm>()
        .unwrap_or(Algorithm::ED25519);
    let out = m.value_of("out_type")
        .unwrap_or("json");
    let target = m.value_of("target")
        .expect("target directory for the config not specified");
    let payload:usize = m.value_of("payload")
        .unwrap_or("0")
        .parse()
        .unwrap();
    let local:String = m.value_of("local")
        .unwrap_or("false")
        .parse()
        .unwrap();
    let _c_rport:u16 = m.value_of("client_run_port")
        .expect("Client port expected")
        .parse::<u16>()
        .expect("unable to parse client's port into an integer");
    let hashrand_batch = m.value_of("hashrand_batch")
        .expect("Unable to parse hashrand_batch").parse::<usize>().unwrap();
    let hashrand_freq = m.value_of("hashrand_freq")
        .expect("Unable to parse hashrand_freq").parse::<u32>().unwrap();

    let mut client = Client::new();
    client.block_size = blocksize;
    client.crypto_alg = t.clone();
    client.num_nodes = num_nodes;
    client.num_faults = num_faults;

    let mut node:Vec<Node> = Vec::with_capacity(num_nodes);

    let mut pk = HashMap::default();
    let mut ip = HashMap::default();
    let mut ip_rbc = HashMap::default();
    let mut ip_dkg = HashMap::default();
    let mut ip_drb = HashMap::default();

    //let (cert, privkey) = new_root_cert()?;
    let mut sec_keys:Vec<Vec<SecretKey>> = Vec::with_capacity(num_nodes);
    (0..num_nodes).for_each(|_i| {
        sec_keys.push(Vec::with_capacity(num_nodes));
    });
    if t == Algorithm::NOPKI{
        // Generate secret keys above and pass them to the context
        for i in 0..num_nodes{
            for j in i..num_nodes{
                let skey:SecretKey = SecretKey::generate();
                sec_keys[i].push(skey.clone());
                if j!= i{
                    sec_keys[j].push(skey.clone());
                }
                //sec_keys.push(SecretKey::generate());
            }
        }
    }
    let mut ve_key_pairs = Vec::new();
    let mut dilithium_key_pairs = Vec::new();

    for i in 0..num_nodes{
        let (ve_pk,ve_sk) = VE::gen_keypair();
        ve_key_pairs.push((i,ve_pk,ve_sk));
        let (sig_pk, sig_sk) = generate_keypair();
        dilithium_key_pairs.push((i,sig_pk,sig_sk));
    }

    for i in 0..num_nodes {
        node.push(Node::new());
        let delphi = DelphiParams {
            delta,
            epsilon,
            tri,
            expo,
            high_val: 1 + tri,
            low_val: 1,
        };

        let acs = ACSParams {
            kappa,
        };

        let pks: Vec<(i32, PublicKey)> = ve_key_pairs.iter().map(|(i,pk,_)| ((i+1) as i32,pk.clone())).collect();
        let sk = ve_key_pairs[i].2.clone();

        let sig_pks: Vec<(i32, DilithiumPublicKey)> = dilithium_key_pairs.iter().map(|(i,pk,_)| ((i+1) as i32,pk.clone())).collect();
        let sig_sk = dilithium_key_pairs[i].2.clone();
        let dkg = DKGParams {
            pks,
            sk,
            sig_pks,
            sig_sk,
            trans_waiting_time: trans_delay
        };

        let drb = DRBParams {
            batch: hashrand_batch,
            frequency: hashrand_freq,
        };


        node[i].delphi = delphi;
        node[i].acs = acs;
        node[i].dkg = dkg;
        node[i].drb = drb;
        node[i].delay = delay;
        node[i].id = i as Replica;
        node[i].num_nodes = num_nodes;
        node[i].num_faults = num_faults;
        node[i].block_size = blocksize;
        node[i].payload = payload;
        node[i].client_port = client_base_port+(i as u16);
        // generate random number for approximate consensus
        let num = rand::thread_rng().gen_range(0, 20000000);
        node[i].prot_payload = format!("a,{},50000,100",num);
        //String::from("a,");
        //node[i].prot_payload = String::from("cc,/home/akhil/research/EEBA/libchatter/");
        node[i].crypto_alg = t.clone();
        match t {
            Algorithm::ED25519 => {
                let kp = ed25519::Keypair::generate();
                pk.insert(i as Replica, kp.public().encode().to_vec());
                node[i].secret_key_bytes = kp.encode().to_vec();

            }
            Algorithm::SECP256K1 => {
                let kp = secp256k1::Keypair::generate();
                pk.insert(i as Replica, kp.public().encode().to_vec());
                node[i].secret_key_bytes = kp.secret().to_bytes().to_vec();
            }
            Algorithm::NOPKI =>{
                for j in 0..num_nodes{
                    node[i].sk_map.insert(j, sec_keys[i][j].to_bytes().to_vec());
                }
            }
            _ => (),
        };
        ip.insert(i as Replica,
                  format!("{}:{}", "127.0.0.1", base_port+(i as u16))
        );
        ip_rbc.insert(i as Replica,
                      format!("{}:{}", "127.0.0.1", rbc_base_port+(i as u16))
        );
        ip_dkg.insert(i as Replica,
                      format!("{}:{}", "127.0.0.1", dkg_base_port+(i as u16))
        );
        ip_drb.insert(i as Replica,
                      format!("{}:{}", "127.0.0.1", drb_base_port+(i as u16))
        );
        client.net_map.insert(i as Replica,
                              format!("127.0.0.1:{}", client_base_port+(i as u16))
        );


        //let (new_cert, new_pkey) = get_signed_cert(&cert, &privkey)?;

        //node[i].root_cert = cert.to_der()?;
        //node[i].my_cert = new_cert.to_der()?;
        //node[i].my_cert_key = new_pkey.private_key_to_der()?;
    }

    // ip.insert(num_nodes, format!("127.0.0.1:{}",c_rport));

    //client.root_cert = cert.to_der()?;

    for i in 0..num_nodes {
        node[i].pk_map = pk.clone();
        node[i].net_map_delphi = ip.clone();
        node[i].net_map_rbc = ip_rbc.clone();
        node[i].net_map_dkg = ip_dkg.clone();
        node[i].net_map_drb = ip_drb.clone();
    }
    if local != String::from("false"){
        // write ip map to file
        //let filename = format!("ip_file");
        println!("Writing ips to ip_file");
        // write ips to ip_file
        {
            let file = File::create("ip_file")?;
            let mut writer = BufWriter::new(file);
            for iter in 0..num_nodes{
                writeln!(writer,"{}",ip.get(&iter).unwrap())?;
            }
            writer.flush()?;
        }
        {
            let file = File::create(format!("{}/syncer",target))?;
            let mut writer = BufWriter::new(file);
            for iter in 0..num_nodes{
                writeln!(writer,"{}",client.net_map.get(&iter).unwrap())?;
            }
            writer.flush()?;
        }
        //write_json(filename, &ip.clone());
    }
    let filename = format!("{}/syncer.json",target);
    write_json(filename, &client.net_map.clone());
    client.server_pk = pk;

    // Write all the files
    for i in 0..num_nodes {
        match out {
            "json" => {
                let filename = format!("{}/nodes-{}.json",target,i);
                write_json(filename, &node[i]);
            },
            "binary" => {
                let filename = format!("{}/nodes-{}.dat",target,i);
                write_bin(filename, &node[i]);
            },
            "toml" => {
                let filename = format!("{}/nodes-{}.toml",target,i);
                write_toml(filename, &node[i]);
            },
            "yaml" => {
                let filename = format!("{}/nodes-{}.yml",target,i);
                write_yaml(filename, &node[i]);
            },
            _ => (),
        }
        node[i].validate()
            .expect("failed to validate node config");
    }

    // Write the client file
    match out {
        "json" => {
            let filename = format!("{}/client.json",target);
            write_json(filename, &client);
        },
        "binary" => {
            let filename = format!("{}/client.dat",target);
            write_bin(filename, &client);
        },
        "toml" => {
            let filename = format!("{}/client.toml",target);
            write_toml(filename, &client);
        },
        "yaml" => {
            let filename = format!("{}/client.yml",target);
            write_yaml(filename, &client);
        },
        _ => (),
    }
    client.validate()
        .expect("failed to validate the client config");

    Ok(())
}


#[cfg(feature = "remote")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::net::IpAddr;
    let yaml = load_yaml!("cli.yml");
    let m = App::from_yaml(yaml).get_matches();
    let num_nodes: usize = m.value_of("num_nodes")
        .expect("number of nodes not specified")
        .parse::<usize>()
        .expect("unable to convert number of nodes into a number");
    let num_faults: usize = match m.value_of("num_faults") {
        Some(x) => x.parse::<usize>()
            .expect("unable to convert number of faults into a number"),
        None => (num_nodes - 1) / 3,
    };
    let delta = m.value_of("delta")
        .expect("Value required").parse::<Val>().unwrap();
    let epsilon = m.value_of("epsilon")
        .expect("Value required").parse::<Val>().unwrap();
    let tri = m.value_of("tri")
        .expect("Value required").parse::<Val>().unwrap();
    let expo = m.value_of("expo")
        .expect("Unable to parse exponent").parse::<f32>().unwrap();
    let kappa: usize = match m.value_of("kappa") {
        Some(x) => x.parse::<usize>()
            .expect("unable to convert kappa into a number"),
        None => (num_nodes - 1) / 3 + 1,
    };
    let trans_delay: u64 = match m.value_of("trans_delay") {
        Some(x) => x.parse::<u64>()
            .expect("unable to convert trans_delay into a number"),
        None => 500,
    };
    let delay: u64 = m.value_of("delay")
        .expect("delay value not specified")
        .parse::<u64>()
        .expect("unable to parse delay value into a number");
    let base_port: u16 = m.value_of("base_port")
        .expect("base_port value not specified")
        .parse::<u16>()
        .expect("failed to parse base_port into a number");
    let rbc_base_port: u16 = m.value_of("rbc_base_port")
        .expect("base_port value not specified")
        .parse::<u16>()
        .expect("failed to parse base_port into a number");
    let dkg_base_port: u16 = m.value_of("dkg_base_port")
        .expect("base_port value not specified")
        .parse::<u16>()
        .expect("failed to parse base_port into a number");
    let drb_base_port: u16 = m.value_of("drb_base_port")
        .expect("base_port value not specified")
        .parse::<u16>()
        .expect("failed to parse base_port into a number");
    let blocksize: usize = m.value_of("block_size")
        .expect("no block_size specified")
        .parse::<usize>()
        .expect("unable to convert blocksize into a number");
    let client_base_port: u16 = m.value_of("client_base_port")
        .expect("no client_base_port specified")
        .parse::<u16>()
        .expect("unable to parse client_base_port into an integer");
    let t: Algorithm = m.value_of("algorithm")
        .unwrap_or("NOPKI")
        .parse::<Algorithm>()
        .unwrap_or(Algorithm::ED25519);
    let out = m.value_of("out_type")
        .unwrap_or("json");
    let target = m.value_of("target")
        .expect("target directory for the config not specified");
    let payload: usize = m.value_of("payload")
        .unwrap_or("0")
        .parse()
        .unwrap();
    let local: String = m.value_of("local")
        .unwrap_or("false")
        .parse()
        .unwrap();
    let _c_rport: u16 = m.value_of("client_run_port")
        .expect("Client port expected")
        .parse::<u16>()
        .expect("unable to parse client's port into an integer");
    let hashrand_batch = m.value_of("hashrand_batch")
        .expect("Unable to parse hashrand_batch").parse::<usize>().unwrap();
    let hashrand_freq = m.value_of("hashrand_freq")
        .expect("Unable to parse hashrand_freq").parse::<u32>().unwrap();
    // Parse and validate remote IPs
    let remote_ips: Vec<String> = m.value_of("remote_ips")
        .expect("remote_ips required when remote feature is enabled")
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    if remote_ips.len() != num_nodes + 1 {
        return Err(format!("Expected {} IP addresses, got {}", num_nodes + 1, remote_ips.len()).into());
    }
    let mut unique_ips = std::collections::HashSet::new();
    for ip in &remote_ips {
        if !ip.parse::<IpAddr>().is_ok() {
            return Err(format!("Invalid IP address: {}", ip).into());
        }
        if !unique_ips.insert(ip.clone()) {
            return Err(format!("Duplicate IP address: {}", ip).into());
        }
    }
    let mut client = Client::new();
    client.block_size = blocksize;
    client.crypto_alg = t.clone();
    client.num_nodes = num_nodes;
    client.num_faults = num_faults;
    let mut node: Vec<Node> = Vec::with_capacity(num_nodes);
    let mut pk = HashMap::default();
    let mut ip = HashMap::default();
    let mut ip_rbc = HashMap::default();
    let mut ip_dkg = HashMap::default();
    let mut ip_drb = HashMap::default();
    let mut sec_keys: Vec<Vec<SecretKey>> = Vec::with_capacity(num_nodes);
    (0..num_nodes).for_each(|_i| {
        sec_keys.push(Vec::with_capacity(num_nodes));
    });
    if t == Algorithm::NOPKI {
        for i in 0..num_nodes {
            for j in i..num_nodes {
                let skey: SecretKey = SecretKey::generate();
                sec_keys[i].push(skey.clone());
                if j != i {
                    sec_keys[j].push(skey.clone());
                }
            }
        }
    }
    let mut ve_key_pairs = Vec::new();
    let mut dilithium_key_pairs = Vec::new();
    for i in 0..num_nodes {
        let (ve_pk, ve_sk) = VE::gen_keypair();
        ve_key_pairs.push((i, ve_pk, ve_sk));
        let (sig_pk, sig_sk) = generate_keypair();
        dilithium_key_pairs.push((i, sig_pk, sig_sk));
    }
    for i in 0..num_nodes {
        node.push(Node::new());
        let delphi = DelphiParams {
            delta,
            epsilon,
            tri,
            expo,
            high_val: 1 + tri,
            low_val: 1,
        };
        let acs = ACSParams { kappa };
        let pks: Vec<(i32, PublicKey)> = ve_key_pairs.iter().map(|(i, pk, _)| ((i + 1) as i32, pk.clone())).collect();
        let sk = ve_key_pairs[i].2.clone();
        let sig_pks: Vec<(i32, DilithiumPublicKey)> = dilithium_key_pairs.iter().map(|(i, pk, _)| ((i + 1) as i32, pk.clone())).collect();
        let sig_sk = dilithium_key_pairs[i].2.clone();
        let dkg = DKGParams {
            pks,
            sk,
            sig_pks,
            sig_sk,
            trans_waiting_time: trans_delay
        };
        let drb = DRBParams {
            batch: hashrand_batch,
            frequency: hashrand_freq,
        };
        node[i].delphi = delphi;
        node[i].acs = acs;
        node[i].dkg = dkg;
        node[i].drb = drb;
        node[i].delay = delay;
        node[i].id = i as Replica;
        node[i].num_nodes = num_nodes;
        node[i].num_faults = num_faults;
        node[i].block_size = blocksize;
        node[i].payload = payload;
        node[i].client_port = client_base_port; // Same port for all nodes
        node[i].client_addr = SocketAddrV4::new(remote_ips[num_nodes].parse().unwrap(),5000).into();
        let num = rand::thread_rng().gen_range(0, 20000000);
        node[i].prot_payload = format!("a,{},50000,100", num);
        node[i].crypto_alg = t.clone();
        match t {
            Algorithm::ED25519 => {
                let kp = ed25519::Keypair::generate();
                pk.insert(i as Replica, kp.public().encode().to_vec());
                node[i].secret_key_bytes = kp.encode().to_vec();
            }
            Algorithm::SECP256K1 => {
                let kp = secp256k1::Keypair::generate();
                pk.insert(i as Replica, kp.public().encode().to_vec());
                node[i].secret_key_bytes = kp.secret().to_bytes().to_vec();
            }
            Algorithm::NOPKI => {
                for j in 0..num_nodes {
                    node[i].sk_map.insert(j, sec_keys[i][j].to_bytes().to_vec());
                }
            }
            _ => (),
        };
        ip.insert(i as Replica, format!("{}:{}", remote_ips[i], base_port));
        ip_rbc.insert(i as Replica, format!("{}:{}", remote_ips[i], rbc_base_port));
        ip_dkg.insert(i as Replica, format!("{}:{}", remote_ips[i], dkg_base_port));
        ip_drb.insert(i as Replica, format!("{}:{}", remote_ips[i], drb_base_port));
        client.net_map.insert(i as Replica, format!("{}:{}", remote_ips[i], client_base_port));
    }
    for i in 0..num_nodes {
        node[i].pk_map = pk.clone();
        node[i].net_map_delphi = ip.clone();
        node[i].net_map_rbc = ip_rbc.clone();
        node[i].net_map_dkg = ip_dkg.clone();
        node[i].net_map_drb = ip_drb.clone();
    }
    if local != String::from("false") {
        println!("Writing ips to ip_file");
        {
            let file = File::create("ip_file")?;
            let mut writer = BufWriter::new(file);
            for iter in 0..num_nodes {
                writeln!(writer, "{}", ip.get(&iter).unwrap())?;
            }
            writer.flush()?;
        }
        {
            let file = File::create(format!("{}/syncer", target))?;
            let mut writer = BufWriter::new(file);
            for iter in 0..num_nodes {
                writeln!(writer, "{}", client.net_map.get(&iter).unwrap())?;
            }
            writer.flush()?;
        }
    }
    let filename = format!("{}/syncer.json", target);
    write_json(filename, &client.net_map.clone());
    client.server_pk = pk;
    for i in 0..num_nodes {
        match out {
            "json" => {
                let filename = format!("{}/nodes-{}.json", target, i);
                write_json(filename, &node[i]);
            },
            "binary" => {
                let filename = format!("{}/nodes-{}.dat", target, i);
                write_bin(filename, &node[i]);
            },
            "toml" => {
                let filename = format!("{}/nodes-{}.toml", target, i);
                write_toml(filename, &node[i]);
            },
            "yaml" => {
                let filename = format!("{}/nodes-{}.yml", target, i);
                write_yaml(filename, &node[i]);
            },
            _ => (),
        }
        node[i].validate().expect("failed to validate node config");
    }
    match out {
        "json" => {
            let filename = format!("{}/client.json", target);
            write_json(filename, &client);
        },
        "binary" => {
            let filename = format!("{}/client.dat", target);
            write_bin(filename, &client);
        },
        "toml" => {
            let filename = format!("{}/client.toml", target);
            write_toml(filename, &client);
        },
        "yaml" => {
            let filename = format!("{}/client.yml", target);
            write_yaml(filename, &client);
        },
        _ => (),
    }
    client.validate().expect("failed to validate the client config");
    Ok(())
}
