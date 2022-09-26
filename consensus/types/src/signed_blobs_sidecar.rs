use crate::blobs_sidecar::BlobsSidecar;
use crate::EthSpec;
use bls::Signature;
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash, Derivative)]
#[serde(bound = "E: EthSpec")]
pub struct SignedBlobsSidecar<E: EthSpec> {
    pub message: BlobsSidecar<E>,
    pub signature: Signature,
}