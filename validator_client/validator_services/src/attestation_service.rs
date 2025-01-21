use crate::duties_service::{DutiesService, DutyAndProof};
use beacon_node_fallback::{ApiTopic, BeaconNodeFallback};
use futures::future::join_all;
use logging::crit;
use slot_clock::SlotClock;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use task_executor::TaskExecutor;
use tokio::time::{sleep, sleep_until, Duration, Instant};
use tracing::{debug, error, info, trace, warn};
use tree_hash::TreeHash;
use types::{Attestation, AttestationData, ChainSpec, CommitteeIndex, EthSpec, Slot};
use validator_store::{Error as ValidatorStoreError, ValidatorStore};

/// Builds an `AttestationService`.
#[derive(Default)]
pub struct AttestationServiceBuilder<S: ValidatorStore, T: SlotClock + 'static> {
    duties_service: Option<Arc<DutiesService<S, T>>>,
    validator_store: Option<Arc<S>>,
    slot_clock: Option<T>,
    beacon_nodes: Option<Arc<BeaconNodeFallback<T>>>,
    executor: Option<TaskExecutor>,
    chain_spec: Option<Arc<ChainSpec>>,
}

impl<S: ValidatorStore + 'static, T: SlotClock + 'static> AttestationServiceBuilder<S, T> {
    pub fn new() -> Self {
        Self {
            duties_service: None,
            validator_store: None,
            slot_clock: None,
            beacon_nodes: None,
            executor: None,
            chain_spec: None,
        }
    }

    pub fn duties_service(mut self, service: Arc<DutiesService<S, T>>) -> Self {
        self.duties_service = Some(service);
        self
    }

    pub fn validator_store(mut self, store: Arc<S>) -> Self {
        self.validator_store = Some(store);
        self
    }

    pub fn slot_clock(mut self, slot_clock: T) -> Self {
        self.slot_clock = Some(slot_clock);
        self
    }

    pub fn beacon_nodes(mut self, beacon_nodes: Arc<BeaconNodeFallback<T>>) -> Self {
        self.beacon_nodes = Some(beacon_nodes);
        self
    }

    pub fn executor(mut self, executor: TaskExecutor) -> Self {
        self.executor = Some(executor);
        self
    }

    pub fn chain_spec(mut self, chain_spec: Arc<ChainSpec>) -> Self {
        self.chain_spec = Some(chain_spec);
        self
    }

    pub fn build(self) -> Result<AttestationService<S, T>, String> {
        Ok(AttestationService {
            inner: Arc::new(Inner {
                duties_service: self
                    .duties_service
                    .ok_or("Cannot build AttestationService without duties_service")?,
                validator_store: self
                    .validator_store
                    .ok_or("Cannot build AttestationService without validator_store")?,
                slot_clock: self
                    .slot_clock
                    .ok_or("Cannot build AttestationService without slot_clock")?,
                beacon_nodes: self
                    .beacon_nodes
                    .ok_or("Cannot build AttestationService without beacon_nodes")?,
                executor: self
                    .executor
                    .ok_or("Cannot build AttestationService without executor")?,
                chain_spec: self
                    .chain_spec
                    .ok_or("Cannot build AttestationService without chain_spec")?,
            }),
        })
    }
}

/// Helper to minimise `Arc` usage.
pub struct Inner<S, T> {
    duties_service: Arc<DutiesService<S, T>>,
    validator_store: Arc<S>,
    slot_clock: T,
    beacon_nodes: Arc<BeaconNodeFallback<T>>,
    executor: TaskExecutor,
    chain_spec: Arc<ChainSpec>,
}

/// Attempts to produce attestations for all known validators 1/3rd of the way through each slot.
///
/// If any validators are on the same committee, a single attestation will be downloaded and
/// returned to the beacon node. This attestation will have a signature from each of the
/// validators.
pub struct AttestationService<S, T> {
    inner: Arc<Inner<S, T>>,
}

impl<S, T> Clone for AttestationService<S, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S, T> Deref for AttestationService<S, T> {
    type Target = Inner<S, T>;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<S: ValidatorStore + 'static, T: SlotClock + 'static> AttestationService<S, T> {
    /// Starts the service which periodically produces attestations.
    pub fn start_update_service(self, spec: &ChainSpec) -> Result<(), String> {
        let slot_duration = Duration::from_secs(spec.seconds_per_slot);
        let duration_to_next_slot = self
            .slot_clock
            .duration_to_next_slot()
            .ok_or("Unable to determine duration to next slot")?;

        info!(
            next_update_millis = duration_to_next_slot.as_millis(),
            "Attestation production service started"
        );

        let executor = self.executor.clone();

        let interval_fut = async move {
            loop {
                if let Some(duration_to_next_slot) = self.slot_clock.duration_to_next_slot() {
                    sleep(duration_to_next_slot + slot_duration / 3).await;

                    if let Err(e) = self.spawn_attestation_tasks(slot_duration) {
                        crit!(error = e, "Failed to spawn attestation tasks")
                    } else {
                        trace!("Spawned attestation tasks");
                    }
                } else {
                    error!("Failed to read slot clock");
                    // If we can't read the slot clock, just wait another slot.
                    sleep(slot_duration).await;
                    continue;
                }
            }
        };

        executor.spawn(interval_fut, "attestation_service");
        Ok(())
    }

    /// For each each required attestation, spawn a new task that downloads, signs and uploads the
    /// attestation to the beacon node.
    fn spawn_attestation_tasks(&self, slot_duration: Duration) -> Result<(), String> {
        let slot = self.slot_clock.now().ok_or("Failed to read slot clock")?;
        let duration_to_next_slot = self
            .slot_clock
            .duration_to_next_slot()
            .ok_or("Unable to determine duration to next slot")?;

        // If a validator needs to publish an aggregate attestation, they must do so at 2/3
        // through the slot. This delay triggers at this time
        let aggregate_production_instant = Instant::now()
            + duration_to_next_slot
                .checked_sub(slot_duration / 3)
                .unwrap_or_else(|| Duration::from_secs(0));

        let duties_by_committee_index: HashMap<CommitteeIndex, Vec<DutyAndProof>> = self
            .duties_service
            .attesters(slot)
            .into_iter()
            .fold(HashMap::new(), |mut map, duty_and_proof| {
                map.entry(duty_and_proof.duty.committee_index)
                    .or_default()
                    .push(duty_and_proof);
                map
            });

        // For each committee index for this slot:
        //
        // - Create and publish an `Attestation` for all required validators.
        // - Create and publish `SignedAggregateAndProof` for all aggregating validators.
        duties_by_committee_index
            .into_iter()
            .for_each(|(committee_index, validator_duties)| {
                // Spawn a separate task for each attestation.
                self.inner.executor.spawn_ignoring_error(
                    self.clone().publish_attestations_and_aggregates(
                        slot,
                        committee_index,
                        validator_duties,
                        aggregate_production_instant,
                    ),
                    "attestation publish",
                );
            });

        // Schedule pruning of the slashing protection database once all unaggregated
        // attestations have (hopefully) been signed, i.e. at the same time as aggregate
        // production.
        self.spawn_slashing_protection_pruning_task(slot, aggregate_production_instant);

        Ok(())
    }

    /// Performs the first step of the attesting process: downloading `Attestation` objects,
    /// signing them and returning them to the validator.
    ///
    /// https://github.com/ethereum/eth2.0-specs/blob/v0.12.1/specs/phase0/validator.md#attesting
    ///
    /// ## Detail
    ///
    /// The given `validator_duties` should already be filtered to only contain those that match
    /// `slot` and `committee_index`. Critical errors will be logged if this is not the case.
    async fn publish_attestations_and_aggregates(
        self,
        slot: Slot,
        committee_index: CommitteeIndex,
        validator_duties: Vec<DutyAndProof>,
        aggregate_production_instant: Instant,
    ) -> Result<(), ()> {
        let attestations_timer = validator_metrics::start_timer_vec(
            &validator_metrics::ATTESTATION_SERVICE_TIMES,
            &[validator_metrics::ATTESTATIONS],
        );

        // There's not need to produce `Attestation` or `SignedAggregateAndProof` if we do not have
        // any validators for the given `slot` and `committee_index`.
        if validator_duties.is_empty() {
            return Ok(());
        }

        // Step 1.
        //
        // Download, sign and publish an `Attestation` for each validator.
        let attestation_opt = self
            .produce_and_publish_attestations(slot, committee_index, &validator_duties)
            .await
            .map_err(move |e| {
                crit!(
                    error = format!("{:?}", e),
                    committee_index,
                    slot = slot.as_u64(),
                    "Error during attestation routine"
                )
            })?;

        drop(attestations_timer);

        // Step 2.
        //
        // If an attestation was produced, make an aggregate.
        if let Some(attestation_data) = attestation_opt {
            // First, wait until the `aggregation_production_instant` (2/3rds
            // of the way though the slot). As verified in the
            // `delay_triggers_when_in_the_past` test, this code will still run
            // even if the instant has already elapsed.
            sleep_until(aggregate_production_instant).await;

            // Start the metrics timer *after* we've done the delay.
            let _aggregates_timer = validator_metrics::start_timer_vec(
                &validator_metrics::ATTESTATION_SERVICE_TIMES,
                &[validator_metrics::AGGREGATES],
            );

            // Then download, sign and publish a `SignedAggregateAndProof` for each
            // validator that is elected to aggregate for this `slot` and
            // `committee_index`.
            self.produce_and_publish_aggregates(
                &attestation_data,
                committee_index,
                &validator_duties,
            )
            .await
            .map_err(move |e| {
                crit!(
                    error = format!("{:?}", e),
                    committee_index,
                    slot = slot.as_u64(),
                    "Error during attestation routine"
                )
            })?;
        }

        Ok(())
    }

    /// Performs the first step of the attesting process: downloading `Attestation` objects,
    /// signing them and returning them to the validator.
    ///
    /// https://github.com/ethereum/eth2.0-specs/blob/v0.12.1/specs/phase0/validator.md#attesting
    ///
    /// ## Detail
    ///
    /// The given `validator_duties` should already be filtered to only contain those that match
    /// `slot` and `committee_index`. Critical errors will be logged if this is not the case.
    ///
    /// Only one `Attestation` is downloaded from the BN. It is then cloned and signed by each
    /// validator and the list of individually-signed `Attestation` objects is returned to the BN.
    async fn produce_and_publish_attestations(
        &self,
        slot: Slot,
        committee_index: CommitteeIndex,
        validator_duties: &[DutyAndProof],
    ) -> Result<Option<AttestationData>, String> {
        if validator_duties.is_empty() {
            return Ok(None);
        }

        let current_epoch = self
            .slot_clock
            .now()
            .ok_or("Unable to determine current slot from clock")?
            .epoch(S::E::slots_per_epoch());

        let attestation_data = self
            .beacon_nodes
            .first_success(|beacon_node| async move {
                let _timer = validator_metrics::start_timer_vec(
                    &validator_metrics::ATTESTATION_SERVICE_TIMES,
                    &[validator_metrics::ATTESTATIONS_HTTP_GET],
                );
                beacon_node
                    .get_validator_attestation_data(slot, committee_index)
                    .await
                    .map_err(|e| format!("Failed to produce attestation data: {:?}", e))
                    .map(|result| result.data)
            })
            .await
            .map_err(|e| e.to_string())?;

        // Create futures to produce signed `Attestation` objects.
        let attestation_data_ref = &attestation_data;
        let signing_futures = validator_duties.iter().map(|duty_and_proof| async move {
            let duty = &duty_and_proof.duty;
            let attestation_data = attestation_data_ref;

            // Ensure that the attestation matches the duties.
            if !duty.match_attestation_data::<S::E>(attestation_data, &self.chain_spec) {
                crit!(
                    validator = ?duty.pubkey,
                    duty_slot = ?duty.slot,
                    attestation_slot = %attestation_data.slot,
                    duty_index = duty.committee_index,
                    attestation_index = attestation_data.index,
                    "Inconsistent validator duties during signing"
                );
                return None;
            }

            let mut attestation = match Attestation::empty_for_signing(
                duty.committee_index,
                duty.committee_length as usize,
                attestation_data.slot,
                attestation_data.beacon_block_root,
                attestation_data.source,
                attestation_data.target,
                &self.chain_spec,
            ) {
                Ok(attestation) => attestation,
                Err(err) => {
                    crit!(
                        validator = ?duty.pubkey,
                        ?duty,
                        ?err,
                        "Invalid validator duties during signing"
                    );
                    return None;
                }
            };

            match self
                .validator_store
                .sign_attestation(
                    duty.pubkey,
                    duty.validator_committee_index as usize,
                    &mut attestation,
                    current_epoch,
                )
                .await
            {
                Ok(()) => Some((attestation, duty.validator_index)),
                Err(ValidatorStoreError::UnknownPubkey(pubkey)) => {
                    // A pubkey can be missing when a validator was recently
                    // removed via the API.
                    warn!(
                        info = "a validator may have recently been removed from this VC",
                        pubkey = ?pubkey,
                        validator = ?duty.pubkey,
                        committee_index = committee_index,
                        slot = slot.as_u64(),
                        "Missing pubkey for attestation"
                    );
                    None
                }
                Err(e) => {
                    crit!(
                        error = ?e,
                        validator = ?duty.pubkey,
                        committee_index,
                        slot = slot.as_u64(),
                        "Failed to sign attestation"
                    );
                    None
                }
            }
        });

        // Execute all the futures in parallel, collecting any successful results.
        let (ref attestations, ref validator_indices): (Vec<_>, Vec<_>) = join_all(signing_futures)
            .await
            .into_iter()
            .flatten()
            .unzip();

        if attestations.is_empty() {
            warn!("No attestations were published");
            return Ok(None);
        }
        let fork_name = self
            .chain_spec
            .fork_name_at_slot::<S::E>(attestation_data.slot);

        // Post the attestations to the BN.
        match self
            .beacon_nodes
            .request(ApiTopic::Attestations, |beacon_node| async move {
                let _timer = validator_metrics::start_timer_vec(
                    &validator_metrics::ATTESTATION_SERVICE_TIMES,
                    &[validator_metrics::ATTESTATIONS_HTTP_POST],
                );
                if fork_name.electra_enabled() {
                    let single_attestations = attestations
                        .iter()
                        .zip(validator_indices)
                        .filter_map(|(a, i)| {
                            match a.to_single_attestation_with_attester_index(*i as usize) {
                                Ok(a) => Some(a),
                                Err(e) => {
                                    // This shouldn't happen unless BN and VC are out of sync with
                                    // respect to the Electra fork.
                                    error!(
                                        log,
                                        "Unable to convert to SingleAttestation";
                                        "error" => ?e,
                                        "committee_index" => attestation_data.index,
                                        "slot" => slot.as_u64(),
                                        "type" => "unaggregated",
                                    );
                                    None
                                }
                            }
                        })
                        .collect::<Vec<_>>();
                    beacon_node
                        .post_beacon_pool_attestations_v2(&single_attestations, fork_name)
                        .await
                } else {
                    beacon_node
                        .post_beacon_pool_attestations_v1(attestations)
                        .await
                }
            })
            .await
        {
            Ok(()) => info!(
                count = attestations.len(),
                validator_indices = ?validator_indices,
                head_block = ?attestation_data.beacon_block_root,
                committee_index = attestation_data.index,
                slot = attestation_data.slot.as_u64(),
                "type" = "unaggregated",
                "Successfully published attestations"
            ),
            Err(e) => error!(
                error = %e,
                committee_index = attestation_data.index,
                slot = slot.as_u64(),
                "type" = "unaggregated",
                "Unable to publish attestations"
            ),
        }

        Ok(Some(attestation_data))
    }

    /// Performs the second step of the attesting process: downloading an aggregated `Attestation`,
    /// converting it into a `SignedAggregateAndProof` and returning it to the BN.
    ///
    /// https://github.com/ethereum/eth2.0-specs/blob/v0.12.1/specs/phase0/validator.md#broadcast-aggregate
    ///
    /// ## Detail
    ///
    /// The given `validator_duties` should already be filtered to only contain those that match
    /// `slot` and `committee_index`. Critical errors will be logged if this is not the case.
    ///
    /// Only one aggregated `Attestation` is downloaded from the BN. It is then cloned and signed
    /// by each validator and the list of individually-signed `SignedAggregateAndProof` objects is
    /// returned to the BN.
    async fn produce_and_publish_aggregates(
        &self,
        attestation_data: &AttestationData,
        committee_index: CommitteeIndex,
        validator_duties: &[DutyAndProof],
    ) -> Result<(), String> {
        if !validator_duties
            .iter()
            .any(|duty_and_proof| duty_and_proof.selection_proof.is_some())
        {
            // Exit early if no validator is aggregator
            return Ok(());
        }

        let fork_name = self
            .chain_spec
            .fork_name_at_slot::<S::E>(attestation_data.slot);

        let aggregated_attestation = &self
            .beacon_nodes
            .first_success(|beacon_node| async move {
                let _timer = validator_metrics::start_timer_vec(
                    &validator_metrics::ATTESTATION_SERVICE_TIMES,
                    &[validator_metrics::AGGREGATES_HTTP_GET],
                );
                if fork_name.electra_enabled() {
                    beacon_node
                        .get_validator_aggregate_attestation_v2(
                            attestation_data.slot,
                            attestation_data.tree_hash_root(),
                            committee_index,
                        )
                        .await
                        .map_err(|e| {
                            format!("Failed to produce an aggregate attestation: {:?}", e)
                        })?
                        .ok_or_else(|| format!("No aggregate available for {:?}", attestation_data))
                        .map(|result| result.data)
                } else {
                    beacon_node
                        .get_validator_aggregate_attestation_v1(
                            attestation_data.slot,
                            attestation_data.tree_hash_root(),
                        )
                        .await
                        .map_err(|e| {
                            format!("Failed to produce an aggregate attestation: {:?}", e)
                        })?
                        .ok_or_else(|| format!("No aggregate available for {:?}", attestation_data))
                        .map(|result| result.data)
                }
            })
            .await
            .map_err(|e| e.to_string())?;

        // Create futures to produce the signed aggregated attestations.
        let signing_futures = validator_duties.iter().map(|duty_and_proof| async move {
            let duty = &duty_and_proof.duty;
            let selection_proof = duty_and_proof.selection_proof.as_ref()?;

            if !duty.match_attestation_data::<S::E>(attestation_data, &self.chain_spec) {
                crit!("Inconsistent validator duties during signing");
                return None;
            }

            match self
                .validator_store
                .produce_signed_aggregate_and_proof(
                    duty.pubkey,
                    duty.validator_index,
                    aggregated_attestation.clone(),
                    selection_proof.clone(),
                )
                .await
            {
                Ok(aggregate) => Some(aggregate),
                Err(ValidatorStoreError::UnknownPubkey(pubkey)) => {
                    // A pubkey can be missing when a validator was recently
                    // removed via the API.
                    debug!(?pubkey, "Missing pubkey for aggregate");
                    None
                }
                Err(e) => {
                    crit!(
                        error = ?e,
                        pubkey = ?duty.pubkey,
                        "Failed to sign aggregate"
                    );
                    None
                }
            }
        });

        // Execute all the futures in parallel, collecting any successful results.
        let signed_aggregate_and_proofs = join_all(signing_futures)
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        if !signed_aggregate_and_proofs.is_empty() {
            let signed_aggregate_and_proofs_slice = signed_aggregate_and_proofs.as_slice();
            match self
                .beacon_nodes
                .first_success(|beacon_node| async move {
                    let _timer = validator_metrics::start_timer_vec(
                        &validator_metrics::ATTESTATION_SERVICE_TIMES,
                        &[validator_metrics::AGGREGATES_HTTP_POST],
                    );
                    if fork_name.electra_enabled() {
                        beacon_node
                            .post_validator_aggregate_and_proof_v2(
                                signed_aggregate_and_proofs_slice,
                                fork_name,
                            )
                            .await
                    } else {
                        beacon_node
                            .post_validator_aggregate_and_proof_v1(
                                signed_aggregate_and_proofs_slice,
                            )
                            .await
                    }
                })
                .await
            {
                Ok(()) => {
                    for signed_aggregate_and_proof in signed_aggregate_and_proofs {
                        let attestation = signed_aggregate_and_proof.message().aggregate();
                        info!(
                            aggregator = signed_aggregate_and_proof.message().aggregator_index(),
                            signatures = attestation.num_set_aggregation_bits(),
                            head_block = format!("{:?}", attestation.data().beacon_block_root),
                            committee_index = attestation.committee_index(),
                            slot = attestation.data().slot.as_u64(),
                            "type" = "aggregated",
                            "Successfully published attestation"
                        );
                    }
                }
                Err(e) => {
                    for signed_aggregate_and_proof in signed_aggregate_and_proofs {
                        let attestation = &signed_aggregate_and_proof.message().aggregate();
                        crit!(
                            error = %e,
                            aggregator = signed_aggregate_and_proof.message().aggregator_index(),
                            committee_index = attestation.committee_index(),
                            slot = attestation.data().slot.as_u64(),
                            "type" = "aggregated",
                            "Failed to publish attestation"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Spawn a blocking task to run the slashing protection pruning process.
    ///
    /// Start the task at `pruning_instant` to avoid interference with other tasks.
    fn spawn_slashing_protection_pruning_task(&self, slot: Slot, pruning_instant: Instant) {
        let attestation_service = self.clone();
        let executor = self.inner.executor.clone();
        let current_epoch = slot.epoch(S::E::slots_per_epoch());

        // Wait for `pruning_instant` in a regular task, and then switch to a blocking one.
        self.inner.executor.spawn(
            async move {
                sleep_until(pruning_instant).await;

                executor.spawn_blocking(
                    move || {
                        attestation_service
                            .validator_store
                            .prune_slashing_protection_db(current_epoch, false)
                    },
                    "slashing_protection_pruning",
                )
            },
            "slashing_protection_pre_pruning",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::FutureExt;
    use parking_lot::RwLock;

    /// This test is to ensure that a `tokio_timer::Sleep` with an instant in the past will still
    /// trigger.
    #[tokio::test]
    async fn delay_triggers_when_in_the_past() {
        let in_the_past = Instant::now() - Duration::from_secs(2);
        let state_1 = Arc::new(RwLock::new(in_the_past));
        let state_2 = state_1.clone();

        sleep_until(in_the_past)
            .map(move |()| *state_1.write() = Instant::now())
            .await;

        assert!(
            *state_2.read() > in_the_past,
            "state should have been updated"
        );
    }
}
