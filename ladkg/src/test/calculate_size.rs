use avsss::components::{share, supple_share, BETA, ID, X_LEN};
use avsss::r_ring::R;
use avsss::{PublicKey, SecretKey, VE};
use nalgebra::DVector;
use rand::prelude::StdRng;
use rand::rngs::OsRng;
use rand::SeedableRng;

#[test]
fn calculate_size() {
    let n = 121;
    let t = 40;
    let (pks, sks) = gen_keypairs(n);
    let mut rng = StdRng::from_rng(OsRng).unwrap();
    let sigma = 1f64;
    let secret =
        DVector::from_iterator(X_LEN, (0..X_LEN).map(|_| R::random_gaussian(&mut rng, sigma)));

    let (prs, pus, st) = share(secret.clone(), n, t, &pks);
    let prs0 = bincode::serialize(&prs[0]).expect("Serialization failed");
    let pus = bincode::serialize(&pus).expect("Serialization failed");
    let share_size = prs0.len() + pus.len();
    let share_size = share_size as f64 / 1024.0;
    println!("Size of share: {:.3} KB", share_size);

    let cipher = st.ciphers[0].clone();
    let mut store = st.clone();
    store.ciphers = vec![cipher];
    let supple_shares = supple_share(store, &pks);
    let sup_share = bincode::serialize(&supple_shares[0]).expect("Serialization failed");
    let sup_share_size = sup_share.len();
    let sup_share_size = sup_share_size as f64 / 1024.0;
    println!("Size of sup_share: {:.3} KB", sup_share_size);

    let sk0 = bincode::serialize(&secret[0]).expect("Serialization failed");
    let sk_size = sk0.len() * secret.len();
    let sk_size = sk_size as f64 / 1024.0;
    println!("Size of secret key: {:.3} KB", sk_size);

    let a = R::random_gaussian(&mut rng, 1f64);
    let beta = R::from(BETA);
    let temp = secret[0].add_mod_p(&a.mul_mod_p(&secret[1]));
    let b_i = beta.sub_mod_p(&temp);
    let pk = bincode::serialize(&b_i).expect("Serialization failed");
    let pk_size = pk.len();
    let pk_size = pk_size as f64 / 1024.0;
    println!("Size of public key: {:.3} KB", pk_size);

}

fn gen_keypairs(n: usize) -> (Vec<(ID, PublicKey)>, Vec<(ID, SecretKey)>) {
    let mut ve_key_pairs = Vec::new();
    for i in 1..=n {
        let (ve_pk, ve_sk) = VE::gen_keypair();
        ve_key_pairs.push((i, ve_pk, ve_sk));
    }
    let pks: Vec<_> = ve_key_pairs.iter().map(|(i, pk, _sk)| (*i as ID, pk.clone())).collect();
    let sks: Vec<_> = ve_key_pairs.iter().map(|(i, _pk, sk)| (*i as ID, sk.clone())).collect();
    (pks,sks)
}