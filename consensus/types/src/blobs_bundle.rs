use crate::{test_utils::TestRandom, *};
use serde::{Serialize, Deserialize};
use test_random_derive::TestRandom;

#[derive(Default, Debug, Clone, Serialize, Deserialize, TestRandom)]
#[serde(bound = "T: EthSpec")]
pub struct BlobsBundle<T: EthSpec> {
    pub block_hash: Hash256,
    pub kzgs: Vec<KzgCommitment>,
    pub blobs: Vec<Blob<T>>,
    pub aggregated_proof: KzgProof,
}