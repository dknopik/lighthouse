use crate::bls_field_element::BlsFieldElement;
use crate::test_utils::RngCore;
use crate::test_utils::TestRandom;
use crate::EthSpec;
use serde::{Deserialize, Serialize};
use ssz::{Decode, DecodeError, Encode};
use ssz_types::VariableList;
use tree_hash::TreeHash;

#[derive(Default, Debug, PartialEq, Hash, Clone, Serialize, Deserialize)]
#[serde(transparent)]
#[serde(bound = "E: EthSpec")]
pub struct Blob<E: EthSpec>(pub VariableList<BlsFieldElement, E::FieldElementsPerBlob>);

impl<E: EthSpec> TestRandom for Blob<E> {
    fn random_for_test(rng: &mut impl RngCore) -> Self {
        Blob(VariableList::random_for_test(rng))
    }
}

impl<E: EthSpec> Encode for Blob<E> {
    fn is_ssz_fixed_len() -> bool {
        <VariableList<BlsFieldElement, E::FieldElementsPerBlob> as Encode>::is_ssz_fixed_len()
    }

    fn ssz_append(&self, buf: &mut Vec<u8>) {
        self.0.ssz_append(buf)
    }

    fn ssz_fixed_len() -> usize {
        <VariableList<BlsFieldElement, E::FieldElementsPerBlob> as Encode>::ssz_fixed_len()
    }

    fn ssz_bytes_len(&self) -> usize {
        self.0.ssz_bytes_len()
    }
}

impl<E: EthSpec> Decode for Blob<E> {
    fn is_ssz_fixed_len() -> bool {
        <VariableList<BlsFieldElement, E::FieldElementsPerBlob> as Decode>::is_ssz_fixed_len()
    }

    fn ssz_fixed_len() -> usize {
        <VariableList<BlsFieldElement, E::FieldElementsPerBlob> as Decode>::ssz_fixed_len()
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        <VariableList<BlsFieldElement, E::FieldElementsPerBlob>>::from_ssz_bytes(bytes).map(Self)
    }
}

impl<E: EthSpec> TreeHash for Blob<E> {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        <VariableList<BlsFieldElement, E::FieldElementsPerBlob>>::tree_hash_type()
    }

    fn tree_hash_packed_encoding(&self) -> Vec<u8> {
        self.0.tree_hash_packed_encoding()
    }

    fn tree_hash_packing_factor() -> usize {
        <VariableList<BlsFieldElement, E::FieldElementsPerBlob>>::tree_hash_packing_factor()
    }

    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        self.0.tree_hash_root()
    }
}
