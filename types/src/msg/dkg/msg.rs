use std::collections::HashSet;
use avsss::components::{PrivateShare, PublicShare};
use avsss::r_ring::R;
use serde::{Deserialize, Serialize};
use crypto::dilithum_sig::Signature;
use crypto::hash::{do_mac, Hash};
use crate::appxcon::{CTRBCMsg, DelphiMsg, ProtMsg, Replica};
use crate::rbc::Msg;
use crate::{Round, Val};
use crate::dkg::trans::Transcript;

#[derive(Debug,Serialize,Deserialize,Clone)]
pub struct ACSWrapperMsg {
    pub protmsg: ACSMsg,
    pub sender:Replica,
    pub mac:Hash,
}

impl ACSWrapperMsg {
    pub fn new(msg: ACSMsg, sender:Replica, sk: &[u8]) -> Self{
        let new_msg = msg.clone();
        let bytes = bincode::serialize(&new_msg).expect("Failed to serialize protocol message");
        let mac = do_mac(&bytes.as_slice(), sk);
        Self{
            protmsg: new_msg,
            mac: mac,
            sender:sender,
        }
    }
}


#[derive(Debug,Serialize,Deserialize,Clone)]
pub enum ACSMsg {
    RBCTrans(Transcript, Replica),
    RBCIndexSet(HashSet<Replica>, Replica),
}


#[derive(Debug,Serialize,Deserialize,Clone)]
pub struct VSSWrapperMsg {
    pub protmsg: VSSMsg,
    pub sender:Replica,
    pub mac:Hash,
}

impl VSSWrapperMsg {
    pub fn new(msg: VSSMsg, sender:Replica, sk: &[u8]) -> Self{
        let new_msg = msg.clone();
        let bytes = bincode::serialize(&new_msg).expect("Failed to serialize protocol message");
        let mac = do_mac(&bytes.as_slice(), sk);
        Self{
            protmsg: new_msg,
            mac: mac,
            sender:sender,
        }
    }
}

#[derive(Debug,Serialize,Deserialize,Clone)]
pub enum VSSMsg {
    VSSPrivateShare(PrivateShare, Replica),
    VSSPublicShare(PublicShare, Replica),
    VSSReply(Signature, Replica),
    DKGPubKey(Box<R>,Replica),
}