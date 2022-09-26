use crate::test_utils::{RngCore, TestRandom};
use std::fmt;
use serde::{Deserialize, Serialize};
use ssz::{Decode, DecodeError, Encode};
use tree_hash::TreeHash;

const KZG_COMMITMENT_BYTES_LEN: usize = 48;

#[derive(Debug, PartialEq, Hash, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KzgCommitment(#[serde(with = "eth2_serde_utils::hex_array")] pub [u8; KZG_COMMITMENT_BYTES_LEN]);

impl Default for KzgCommitment {
    fn default() -> Self {
        KzgCommitment([0; 48])
    }
}

impl fmt::Display for KzgCommitment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", eth2_serde_utils::hex::encode(&self.0))
    }
}

impl From<[u8; KZG_COMMITMENT_BYTES_LEN]> for KzgCommitment {
    fn from(bytes: [u8; KZG_COMMITMENT_BYTES_LEN]) -> Self {
        Self(bytes)
    }
}

impl Into<[u8; KZG_COMMITMENT_BYTES_LEN]> for KzgCommitment {
    fn into(self) -> [u8; KZG_COMMITMENT_BYTES_LEN] {
        self.0
    }
}

impl Encode for KzgCommitment {
    fn is_ssz_fixed_len() -> bool {
        <[u8; KZG_COMMITMENT_BYTES_LEN] as Encode>::is_ssz_fixed_len()
    }

    fn ssz_append(&self, buf: &mut Vec<u8>) {
        self.0.ssz_append(buf)
    }

    fn ssz_fixed_len() -> usize {
        <[u8; KZG_COMMITMENT_BYTES_LEN] as Encode>::ssz_fixed_len()
    }

    fn ssz_bytes_len(&self) -> usize {
        self.0.ssz_bytes_len()
    }
}

impl Decode for KzgCommitment {
    fn is_ssz_fixed_len() -> bool {
        <[u8; KZG_COMMITMENT_BYTES_LEN] as Decode>::is_ssz_fixed_len()
    }

    fn ssz_fixed_len() -> usize {
        <[u8; KZG_COMMITMENT_BYTES_LEN] as Decode>::ssz_fixed_len()
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        <[u8; KZG_COMMITMENT_BYTES_LEN]>::from_ssz_bytes(bytes).map(Self)
    }
}

impl TreeHash for KzgCommitment {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        <[u8; KZG_COMMITMENT_BYTES_LEN]>::tree_hash_type()
    }

    fn tree_hash_packed_encoding(&self) -> Vec<u8> {
        self.0.tree_hash_packed_encoding()
    }

    fn tree_hash_packing_factor() -> usize {
        <[u8; KZG_COMMITMENT_BYTES_LEN]>::tree_hash_packing_factor()
    }

    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        self.0.tree_hash_root()
    }
}

impl TestRandom for KzgCommitment {
    fn random_for_test(rng: &mut impl RngCore) -> Self {
        Self(<[u8; KZG_COMMITMENT_BYTES_LEN]>::random_for_test(rng))
    }
}
