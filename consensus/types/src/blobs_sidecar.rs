use crate::*;
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::VariableList;
use tree_hash_derive::TreeHash;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash, Derivative)]
#[serde(bound = "E: EthSpec")]
pub struct BlobsSidecar<E: EthSpec> {
    pub beacon_block_root: Hash256,
    pub beacon_block_slot: Slot,
    pub blobs: VariableList<Blob<E>, E::MaxBlobsPerBlock>,
    pub kzg_aggregate_proof: KzgProof,
}

impl<E: EthSpec> SignedRoot for BlobsSidecar<E> {}

impl<E: EthSpec> BlobsSidecar<E> {
    /// Signs `self`, producing a `SignedBlobsSidecar`.
    pub fn sign(
        self,
        secret_key: &SecretKey,
        fork: &Fork,
        genesis_validators_root: Hash256,
        spec: &ChainSpec,
    ) -> SignedBlobsSidecar<E> {
        let domain = spec.get_domain(
            self.beacon_block_slot.epoch(E::slots_per_epoch()),
            Domain::BeaconProposer,
            fork,
            genesis_validators_root,
        );
        let message = self.signing_root(domain);
        let signature = secret_key.sign(message);
        SignedBlobsSidecar {
            message: self,
            signature,
        }
    }
}