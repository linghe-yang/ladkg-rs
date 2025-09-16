use std::collections::HashSet;
use crate::Replica;
use avsss::components::{verify, PublicShare, SuppleShare};
use avsss::r_ring::R;
use avsss::PublicKey as VEPublicKey;
use crypto::dilithum_sig::{PublicKey as DilithiumPublicKey, Signature};
use crypto::hash::Hash;
use nalgebra::DVector;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Transcript {
    pub id: Replica,
    pub cert: Vec<(Replica, Signature)>,
    pub pub_share: PublicShare,
    pub supple_shares: Vec<SuppleShare>,
}

impl Transcript {
    pub fn verify(
        &self,
        sig_pks: &Vec<(i32, DilithiumPublicKey)>,
        ve_pks: &Vec<(i32, VEPublicKey)>,
        num_nodes: usize
    ) -> bool {
        if !self.is_id_valid(num_nodes) {return false}
        let h = self.pub_share.merkle_root;
        let u_vec = &self.pub_share.u_vec;
        for (rep,sig) in self.cert.iter() {
            if let Some((_,sig_pk)) = sig_pks.iter().find(|(id, _)| *id == (rep + 1) as i32){
                if sig.verify(&h, sig_pk).is_err() {return false}
            } else {return false}
        }

        for ss in self.supple_shares.iter() {
            if let Some((_,ve_pk)) = ve_pks.iter().find(|(id, _)| *id == ss.id){
                if let Some((_,u)) = u_vec.iter().find(|(id, _)| *id == ss.id) {
                    if !verify(ve_pk,ss,u,h) { return false}
                }else {
                    return false;
                }
            } else {return false}
        }
        true
    }


    fn is_id_valid(&self, n: usize) -> bool {
        // Collect and validate unique Replicas from cert (0 to n-1, no duplicates)
        let mut replica_set: HashSet<usize> = HashSet::new();
        for (rep, _sig) in &self.cert {
            let rep_id = rep;
            if rep_id >= &n || !replica_set.insert(*rep_id) {
                return false;  // Duplicate or out of range
            }
        }

        // Collect and validate unique ids from supple_shares (1 to n, no duplicates)
        let mut id_set: HashSet<i32> = HashSet::new();
        for share in &self.supple_shares {
            let share_id = share.id;
            if share_id < 1 || share_id > (n as i32) || !id_set.insert(share_id) {
                return false;  // Duplicate or out of range
            }
        }

        // Check for overlap: transformed replicas (rep + 1) should not intersect with ids
        for &rep in &replica_set {
            let transformed = (rep + 1) as i32;
            if id_set.contains(&transformed) {
                return false;
            }
        }

        // Compute union size: should be exactly n, and cover 1 to n
        let union_size = replica_set.len() + id_set.len();
        if union_size != n {
            return false;
        }

        // Since no overlap and sizes add to n, and each is within 1..=n (transformed replicas are 1 to n),
        // the union automatically covers 1 to n without gaps.
        true
    }
}
