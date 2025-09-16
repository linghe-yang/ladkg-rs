pub mod context;
pub use context::*;

pub mod handler;
pub use handler::*;

pub mod process;
pub use process::*;

pub mod rbc_state;
pub use rbc_state::*;

pub mod rbc;
pub use rbc::*;

pub mod coin;
pub use coin::*;

pub mod roundvals_bin;
pub use roundvals_bin::*;

pub mod baainit;
pub use baainit::*;

#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum Error {
    KeyGenMisMatchedVectors,
    KeyGenBadCommitment,
    KeyGenInvalidShare,
    KeyGenDlogProofError,
    PartialSignatureVerificationError,
    SigningMisMatchedVectors,
}