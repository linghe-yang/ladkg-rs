use avsss::r_ring::R;
use serde::{Serialize, Deserialize};

use crate::{WireReady, Replica};

#[derive(Debug,Serialize,Deserialize,Clone)]
pub enum SyncState{
    ALIVE,
    StartVSS,
    SkComplete,
    PkComplete(Box<R>),
    STOP
}

#[derive(Debug,Serialize,Deserialize,Clone)]
pub struct SyncMsg{
    pub sender:Replica,
    pub state:SyncState,
}

impl WireReady for SyncMsg{
    fn from_bytes(bytes: &[u8]) -> Self {
        let c:Self = bincode::deserialize(bytes)
            .expect("failed to decode the protocol message");
        c.init()
    }

    fn init(self) -> Self {
        match self {
            _x=>_x
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let bytes = bincode::serialize(self).expect("Failed to serialize client message");
        bytes
    }
}