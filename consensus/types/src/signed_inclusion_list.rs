use crate::test_utils::TestRandom;
use crate::{EthSpec, SignedInclusionListSummary, Transaction};
use bls::Signature;
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::VariableList;
use test_random_derive::TestRandom;
use tree_hash_derive::TreeHash;

pub type InclusionList<E> = VariableList<
    Transaction<<E as EthSpec>::MaxBytesPerTransaction>,
    <E as EthSpec>::MaxTransactionsPerInclusionList,
>;

#[derive(
    arbitrary::Arbitrary,
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Encode,
    Decode,
    TreeHash,
    TestRandom,
    Derivative,
)]
#[derivative(Hash(bound = "E: EthSpec"))]
#[serde(bound = "E: EthSpec")]
#[arbitrary(bound = "E: EthSpec")]
pub struct SignedInclusionList<E: EthSpec> {
    pub signed_summary: SignedInclusionListSummary<E>,
    pub transactions: InclusionList<E>,
    pub signature: Signature,
}
