#[cfg(test)]
mod tests {
    use {
        anchor_lang::{AccountDeserialize, InstructionData},
        litesvm::LiteSVM,
        solana_clock::Clock as SolanaClock,
        solana_instruction::{AccountMeta, Instruction},
        solana_keypair::Keypair,
        solana_message::Message,
        solana_native_token::LAMPORTS_PER_SOL,
        solana_pubkey::Pubkey,
        solana_sdk::msg,
        solana_signer::Signer,
        solana_transaction::Transaction,
        solana_transaction_error::TransactionError,
        std::path::PathBuf,
    };

    fn program_id() -> Pubkey {
        Pubkey::from(crate::ID.to_bytes())
    }

    fn clock_id() -> Pubkey {
        Pubkey::from(anchor_lang::solana_program::sysvar::clock::ID.to_bytes())
    }

    fn setup() -> (LiteSVM, Keypair) {
        let mut svm = LiteSVM::new();
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL)
            .unwrap();

        let so_path = PathBuf::from("../../target/sbpf-solana-solana/release/s0nar_program.so");
        msg!("The path is!! {:?}", so_path);

        let program_data = std::fs::read(so_path).expect("Failed to read program SO file");
        svm.add_program(program_id(), &program_data).unwrap();

        (svm, payer)
    }

    fn get_registry_pda() -> Pubkey {
        Pubkey::find_program_address(&[crate::state::REGISTRY_SEED], &program_id()).0
    }

    fn get_network_health_pda() -> Pubkey {
        Pubkey::find_program_address(&[crate::state::NETWORK_HEALTH_SEED], &program_id()).0
    }

    fn get_observer_pda(pubkey: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[crate::state::OBSERVER_SEED, pubkey.as_ref()],
            &program_id(),
        )
        .0
    }

    fn send_tx(
        svm: &mut LiteSVM,
        ixs: &[Instruction],
        signers: &[&Keypair],
    ) -> Result<(), TransactionError> {
        if signers.is_empty() {
            return Err(TransactionError::InvalidAccountIndex);
        }
        let payer_pubkey = signers[0].pubkey();
        let message = Message::new(ixs, Some(&payer_pubkey));
        let blockhash = svm.latest_blockhash();
        let tx = Transaction::new(signers, message, blockhash);
        svm.send_transaction(tx).map_err(|e| e.err).map(|_| ())
    }

    fn init_protocol(svm: &mut LiteSVM, auth: &Keypair, min_stake: u64, max_obs: u16) {
        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(auth.pubkey(), true),
                AccountMeta::new(get_registry_pda(), false),
                AccountMeta::new(get_network_health_pda(), false),
                AccountMeta::new_readonly(solana_sdk_ids::system_program::ID, false),
            ],
            data: crate::instruction::Initialize {
                min_stake_lamports: min_stake,
                max_observers: max_obs,
            }
            .data(),
        };
        send_tx(svm, &[ix], &[auth]).unwrap();
    }

    fn register_observer(
        svm: &mut LiteSVM,
        obs: &Keypair,
        region: crate::Region,
    ) -> Result<(), TransactionError> {
        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(obs.pubkey(), true),
                AccountMeta::new(get_observer_pda(&obs.pubkey()), false),
                AccountMeta::new(get_registry_pda(), false),
                AccountMeta::new_readonly(solana_sdk_ids::system_program::ID, false),
            ],
            data: crate::instruction::RegisterObserver { region }.data(),
        };
        send_tx(svm, &[ix], &[obs])
    }

    fn submit_attestation(
        svm: &mut LiteSVM,
        obs: &Keypair,
        reachable: u16,
        probed: u16,
        lat: u32,
    ) -> Result<(), TransactionError> {
        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(obs.pubkey(), true),
                AccountMeta::new(get_observer_pda(&obs.pubkey()), false),
                AccountMeta::new(get_network_health_pda(), false),
                AccountMeta::new_readonly(get_registry_pda(), false),
                AccountMeta::new_readonly(clock_id(), false),
            ],
            data: crate::instruction::SubmitAttestation {
                tpu_reachable: reachable,
                tpu_probed: probed,
                avg_rtt_us: 1000,
                p95_rtt_us: 2000,
                slot_latency_ms: lat,
                agave_count: 0,
                firedancer_count: 0,
                jito_count: 0,
                solana_labs_count: 0,
                other_count: 0,
            }
            .data(),
        };
        send_tx(svm, &[ix], &[obs])
    }

    /// Returns Ok if crank succeeded, Err if the transaction failed (e.g. NoActiveObservers).
    #[allow(clippy::too_many_arguments)]
    fn submit_attestation_with_clients(
        svm: &mut LiteSVM,
        obs: &Keypair,
        reachable: u16,
        probed: u16,
        lat: u32,
        agave: u16,
        firedancer: u16,
        jito: u16,
        labs: u16,
        other: u16,
    ) -> Result<(), TransactionError> {
        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(obs.pubkey(), true),
                AccountMeta::new(get_observer_pda(&obs.pubkey()), false),
                AccountMeta::new(get_network_health_pda(), false),
                AccountMeta::new_readonly(get_registry_pda(), false),
                AccountMeta::new_readonly(clock_id(), false),
            ],
            data: crate::instruction::SubmitAttestation {
                tpu_reachable: reachable,
                tpu_probed: probed,
                avg_rtt_us: 1000,
                p95_rtt_us: 2000,
                slot_latency_ms: lat,
                agave_count: agave,
                firedancer_count: firedancer,
                jito_count: jito,
                solana_labs_count: labs,
                other_count: other,
            }
            .data(),
        };
        send_tx(svm, &[ix], &[obs])
    }

    fn crank_aggregation(
        svm: &mut LiteSVM,
        payer: &Keypair,
        observers: &[Pubkey],
    ) -> Result<(), TransactionError> {
        let mut accounts = vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(get_network_health_pda(), false),
            AccountMeta::new_readonly(get_registry_pda(), false),
            AccountMeta::new_readonly(clock_id(), false),
        ];
        for o in observers {
            accounts.push(AccountMeta::new_readonly(get_observer_pda(o), false));
        }
        let ix = Instruction {
            program_id: program_id(),
            accounts,
            data: crate::instruction::CrankAggregation {}.data(),
        };
        send_tx(svm, &[ix], &[payer])
    }

    fn deregister_observer(
        svm: &mut LiteSVM,
        caller: &Keypair,
        observer: &Pubkey,
    ) -> Result<(), TransactionError> {
        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(caller.pubkey(), true),
                AccountMeta::new(*observer, false),
                AccountMeta::new(get_observer_pda(observer), false),
                AccountMeta::new(get_registry_pda(), false),
                AccountMeta::new_readonly(solana_sdk_ids::system_program::ID, false),
            ],
            data: crate::instruction::DeregisterObserver {}.data(),
        };
        send_tx(svm, &[ix], &[caller])
    }

    fn slash_observer(
        svm: &mut LiteSVM,
        auth: &Keypair,
        observer: &Pubkey,
        treasury: &Pubkey,
        slash_bps: u16,
    ) -> Result<(), TransactionError> {
        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(auth.pubkey(), true),
                AccountMeta::new_readonly(*observer, false),
                AccountMeta::new(get_observer_pda(observer), false),
                AccountMeta::new(get_registry_pda(), false),
                AccountMeta::new(*treasury, false),
            ],
            data: crate::instruction::SlashObserver { slash_bps }.data(),
        };
        send_tx(svm, &[ix], &[auth])
    }

    fn update_config(
        svm: &mut LiteSVM,
        auth: &Keypair,
        min_stake_lamports: Option<u64>,
        max_observers: Option<u16>,
        paused: Option<bool>,
    ) -> Result<(), TransactionError> {
        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(auth.pubkey(), true),
                AccountMeta::new(get_registry_pda(), false),
            ],
            data: crate::instruction::UpdateConfig {
                min_stake_lamports,
                max_observers,
                paused,
            }
            .data(),
        };
        send_tx(svm, &[ix], &[auth])
    }

    fn propose_authority(
        svm: &mut LiteSVM,
        auth: &Keypair,
        new_auth: &Pubkey,
    ) -> Result<(), TransactionError> {
        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(get_registry_pda(), false),
                AccountMeta::new_readonly(auth.pubkey(), true),
            ],
            data: crate::instruction::ProposeAuthority {
                new_authority: anchor_lang::prelude::Pubkey::new_from_array(new_auth.to_bytes()),
            }
            .data(),
        };
        send_tx(svm, &[ix], &[auth])
    }

    fn accept_authority(svm: &mut LiteSVM, new_auth: &Keypair) -> Result<(), TransactionError> {
        let ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(get_registry_pda(), false),
                AccountMeta::new_readonly(new_auth.pubkey(), true),
            ],
            data: crate::instruction::AcceptAuthority {}.data(),
        };
        send_tx(svm, &[ix], &[new_auth])
    }

    fn advance_slot(svm: &mut LiteSVM, slots: u64) {
        let mut clock: SolanaClock = svm.get_sysvar::<SolanaClock>();
        clock.slot += slots;
        svm.set_sysvar(&clock);
        svm.warp_to_slot(clock.slot);
        svm.expire_blockhash();
    }

    // -------------------test begins----------------------

    // ----------------------base--------------------------

    #[test]
    fn test_happy_path_and_multi_aggregation() {
        let (mut svm, authority) = setup();
        // initializing the protocol
        init_protocol(&mut svm, &authority, 1, 10);

        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

        // registering 2 observers
        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &obs2, crate::Region::US).unwrap();

        let reg = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();
        assert_eq!(reg.active_count, 2);

        // advancing slot
        advance_slot(&mut svm, 1);

        // submitting attestation from observers
        submit_attestation(&mut svm, &obs1, 90, 100, 400).unwrap();
        submit_attestation(&mut svm, &obs2, 95, 100, 300).unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(health.total_attestations, 2);

        crank_aggregation(&mut svm, &authority, &[obs1.pubkey(), obs2.pubkey()]).unwrap();
    }

    #[test]
    fn test_initialize_populates_all_region_slots() {
        let (mut svm, authority) = setup();
        init_protocol(&mut svm, &authority, 1, 10);

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        assert_eq!(
            health.region_scores.len(),
            crate::state::NetworkHealthAccount::REGION_COUNT
        );
        assert!(health
            .region_scores
            .iter()
            .any(|rs| rs.region == crate::Region::Asia));
        assert!(health
            .region_scores
            .iter()
            .any(|rs| rs.region == crate::Region::US));
        assert!(health
            .region_scores
            .iter()
            .any(|rs| rs.region == crate::Region::EU));
        assert!(health
            .region_scores
            .iter()
            .any(|rs| rs.region == crate::Region::SouthAmerica));
        assert!(health
            .region_scores
            .iter()
            .any(|rs| rs.region == crate::Region::Africa));
        assert!(health
            .region_scores
            .iter()
            .any(|rs| rs.region == crate::Region::Oceania));
        assert!(health
            .region_scores
            .iter()
            .any(|rs| rs.region == crate::Region::Other));
    }

    #[test]
    fn test_failures_and_refund() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, LAMPORTS_PER_SOL, 2);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

        // First register must succeed — transfers 1 SOL stake into the observer PDA
        register_observer(&mut svm, &obs, crate::Region::EU).unwrap();

        // Double-register must fail (PDA already initialized)
        let err = register_observer(&mut svm, &obs, crate::Region::EU);
        assert!(err.is_err(), "double register should fail");

        advance_slot(&mut svm, 1);

        // reachable > probed — InvalidReachabilityCount
        let err = submit_attestation(&mut svm, &obs, 110, 100, 400);
        assert!(err.is_err(), "invalid attestation should fail");

        let obs_bal_before = svm.get_balance(&obs.pubkey()).unwrap();

        // Deregister — transfers 1 SOL stake back from PDA to observer wallet
        deregister_observer(&mut svm, &obs, &obs.pubkey()).unwrap();

        // 1 SOL returned > tx fees, so balance strictly increases
        let obs_bal_after = svm.get_balance(&obs.pubkey()).unwrap();
        assert!(
            obs_bal_after > obs_bal_before,
            "stake refund should exceed fees: before={} after={}",
            obs_bal_before,
            obs_bal_after
        );

        // Submit after deregister must fail (ObserverNotActive)
        let err = submit_attestation(&mut svm, &obs, 90, 100, 400);
        assert!(err.is_err(), "submit after deregister should fail");
    }

    #[test]
    fn test_stale_logic() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 2);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 100, 100, 200).unwrap();

        // Advance past STALE_SLOTS (150) so the observer is skipped in crank
        advance_slot(&mut svm, 200);

        // Crank should return an error: NoActiveObservers (all observers stale)
        let result = crank_aggregation(&mut svm, &auth, &[obs.pubkey()]);
        assert!(
            result.is_err(),
            "crank with all-stale observers should fail with NoActiveObservers"
        );
    }

    // --------------------------core--------------------------

    /// Repeated attestations (same observer, 20 submissions)
    #[test]
    fn test_repeated_attestations_same_observer() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 10);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();

        for i in 1..=20u64 {
            advance_slot(&mut svm, 1);
            submit_attestation(&mut svm, &obs, 80, 100, 300).unwrap();

            let oa = crate::state::ObserverAccount::try_deserialize(
                &mut svm
                    .get_account(&get_observer_pda(&obs.pubkey()))
                    .unwrap()
                    .data
                    .as_ref(),
            )
            .unwrap();
            assert_eq!(oa.attestation_count, i, "count mismatch at iteration {}", i);

            let health = crate::state::NetworkHealthAccount::try_deserialize(
                &mut svm
                    .get_account(&get_network_health_pda())
                    .unwrap()
                    .data
                    .as_ref(),
            )
            .unwrap();
            assert_eq!(
                health.total_attestations, i,
                "total mismatch at iteration {}",
                i
            );
        }

        // Verify latest_attestation reflects newest data
        let oa = crate::state::ObserverAccount::try_deserialize(
            &mut svm
                .get_account(&get_observer_pda(&obs.pubkey()))
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(oa.attestation_count, 20);
        assert!(oa.latest_attestation.slot > 0);
    }

    /// Same-slot double submission (spam) → rejected as StaleAttestation
    #[test]
    fn test_same_slot_double_submission() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 10);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 90, 100, 200).unwrap();

        // Second submission in the SAME slot → must fail (StaleAttestation)
        let err = submit_attestation(&mut svm, &obs, 95, 100, 150);
        assert!(err.is_err(), "same-slot double submit must be rejected");

        // Verify count is still 1
        let oa = crate::state::ObserverAccount::try_deserialize(
            &mut svm
                .get_account(&get_observer_pda(&obs.pubkey()))
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(
            oa.attestation_count, 1,
            "double submit must not double-count"
        );
    }

    /// High-frequency multi-observer load (5 observers, 10 rounds each)
    #[test]
    fn test_high_frequency_multi_observer_load() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 10);

        let regions = [
            crate::Region::Asia,
            crate::Region::US,
            crate::Region::EU,
            crate::Region::Asia,
            crate::Region::US,
        ];
        let mut observers = Vec::new();
        for region in regions.iter() {
            let obs = Keypair::new();
            svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
            register_observer(&mut svm, &obs, *region).unwrap();
            observers.push(obs);
        }

        let rounds = 10u64;
        for _ in 0..rounds {
            advance_slot(&mut svm, 1);
            for obs in observers.iter() {
                submit_attestation(&mut svm, obs, 85, 100, 250).unwrap();
            }
        }

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(
            health.total_attestations,
            rounds * observers.len() as u64,
            "total attestations must match observers × rounds"
        );

        // Crank should succeed
        let pubkeys: Vec<Pubkey> = observers.iter().map(|o| o.pubkey()).collect();
        crank_aggregation(&mut svm, &auth, &pubkeys).unwrap();
    }

    /// Interleaved operations: submit → submit → crank → submit → crank
    #[test]
    fn test_interleaved_operations() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 10);

        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &obs2, crate::Region::EU).unwrap();

        let pubkeys = [obs1.pubkey(), obs2.pubkey()];

        // submit → submit
        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs1, 90, 100, 300).unwrap();
        submit_attestation(&mut svm, &obs2, 80, 100, 350).unwrap();

        // crank
        crank_aggregation(&mut svm, &auth, &pubkeys).unwrap();

        let h1 = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        let score_after_first_crank = h1.health_score;
        assert_eq!(h1.total_attestations, 2);
        assert!(h1.health_score > 0 && h1.health_score <= 100);

        // submit (new slot)
        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs1, 95, 100, 200).unwrap();

        // crank again
        crank_aggregation(&mut svm, &auth, &pubkeys).unwrap();

        let h2 = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(h2.total_attestations, 3);
        assert!(
            h2.health_score > 0,
            "health score must be non-zero after valid submissions"
        );
        // Score should remain logical (no corruption)
        assert!(h2.health_score <= 100);

        // state must evolve (no stale reuse / no corruption)
        assert_ne!(
            h2.health_score, score_after_first_crank,
            "health score should change after new attestation"
        );

        // Optional stronger invariant: second score should reflect improvement
        // (since obs1 improved from 90→95 and latency 300→200)
        assert!(
            h2.health_score >= score_after_first_crank,
            "improved input should not reduce health score"
        );
    }

    // ----------------------staleness and time logic----------------------

    /// Stale boundary: exact edge (STALE_SLOTS = 150)
    /// crank uses `> STALE_SLOTS` → at 150 slots gap = still active, at 151 = stale
    #[test]
    fn test_stale_boundary_exact_edge() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 90, 100, 200).unwrap();

        // At exactly STALE_SLOTS (150) gap → should still be active
        advance_slot(&mut svm, 150);
        let result = crank_aggregation(&mut svm, &auth, &[obs.pubkey()]);
        assert!(
            result.is_ok(),
            "at exactly 150 slots gap, observer should still be active"
        );

        // One more slot → 151 gap → stale
        advance_slot(&mut svm, 1);
        let result = crank_aggregation(&mut svm, &auth, &[obs.pubkey()]);
        assert!(
            result.is_err(),
            "at 151 slots gap, observer must be stale → NoActiveObservers"
        );
    }

    /// Observer becomes stale → excluded from aggregation
    #[test]
    fn test_observer_becomes_stale_excluded() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &obs2, crate::Region::EU).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs1, 90, 100, 200).unwrap();
        submit_attestation(&mut svm, &obs2, 80, 100, 300).unwrap();

        // obs1 stops submitting; advance past stale threshold
        advance_slot(&mut svm, 151);

        // obs2 submits fresh data
        submit_attestation(&mut svm, &obs2, 85, 100, 250).unwrap();

        // Crank with both → only obs2 should count
        crank_aggregation(&mut svm, &auth, &[obs1.pubkey(), obs2.pubkey()]).unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(
            health.active_observer_count, 1,
            "only obs2 should be active"
        );
    }

    /// Observer resumes after being stale → included again
    #[test]
    fn test_observer_resumes_after_stale() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 90, 100, 200).unwrap();

        // Go stale
        advance_slot(&mut svm, 200);
        let result = crank_aggregation(&mut svm, &auth, &[obs.pubkey()]);
        assert!(result.is_err(), "should be stale");

        // Resume
        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 95, 100, 150).unwrap();

        // Crank again → should succeed now
        crank_aggregation(&mut svm, &auth, &[obs.pubkey()]).unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert!(
            health.active_observer_count >= 1,
            "resumed observer should be active"
        );
    }

    /// Mixed state: some active, some stale → only active used
    #[test]
    fn test_mixed_state_aggregation() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let active_obs = Keypair::new();
        let stale_obs = Keypair::new();
        svm.airdrop(&active_obs.pubkey(), 10 * LAMPORTS_PER_SOL)
            .unwrap();
        svm.airdrop(&stale_obs.pubkey(), 10 * LAMPORTS_PER_SOL)
            .unwrap();
        register_observer(&mut svm, &active_obs, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &stale_obs, crate::Region::EU).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &active_obs, 90, 100, 200).unwrap();
        submit_attestation(&mut svm, &stale_obs, 50, 100, 390).unwrap();

        // Make stale_obs stale
        advance_slot(&mut svm, 151);
        // Keep active_obs fresh
        submit_attestation(&mut svm, &active_obs, 95, 100, 150).unwrap();

        crank_aggregation(&mut svm, &auth, &[active_obs.pubkey(), stale_obs.pubkey()]).unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(
            health.active_observer_count, 1,
            "only active observer should count"
        );
        assert!(health.health_score > 0);

        let asia_score = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::Asia)
            .unwrap();
        let eu_score = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::EU)
            .unwrap();

        assert!(
            asia_score.health_score > 0,
            "active region should remain populated"
        );
        assert_eq!(
            eu_score.health_score, 0,
            "stale region score should be cleared"
        );
        assert_eq!(
            eu_score.reachability_pct, 0,
            "stale region reachability should be cleared"
        );
        assert_eq!(eu_score.avg_rtt_us, 0, "stale region RTT should be cleared");
        assert_eq!(
            eu_score.slot_latency_ms, 0,
            "stale region latency should be cleared"
        );
        assert!(
            eu_score.last_updated_slot > 0,
            "stale region should retain last update marker"
        );
    }

    // ----------------------input validation----------------------

    /// Invalid reachability: reachable > probed → must fail
    #[test]
    fn test_invalid_reachability() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        advance_slot(&mut svm, 1);
        let err = submit_attestation(&mut svm, &obs, 110, 100, 200);
        assert!(err.is_err(), "reachable > probed must fail");
    }

    /// Zero probing: probed = 0 → must fail (ZeroValidatorsProbed)
    #[test]
    fn test_zero_probing() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::EU).unwrap();

        advance_slot(&mut svm, 1);
        let err = submit_attestation(&mut svm, &obs, 0, 0, 200);
        assert!(err.is_err(), "probed=0 must fail with ZeroValidatorsProbed");
    }

    /// Extreme values: latency=0, latency=u32::MAX
    #[test]
    fn test_extreme_latency_values() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();

        // latency = 0 → max latency score
        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 100, 100, 0).unwrap();
        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert!(health.health_score > 0, "latency=0 should yield good score");
        assert!(health.health_score <= 100, "score must not overflow");

        // latency = very high → must fail
        advance_slot(&mut svm, 1);
        let err = submit_attestation(&mut svm, &obs, 100, 100, u32::MAX);
        assert!(
            err.is_err(),
            "extreme latency must fail with InvalidLatencyValue"
        );
    }

    /// Minimum valid inputs
    #[test]
    fn test_minimum_valid_inputs() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        advance_slot(&mut svm, 1);
        // Smallest valid: reachable=0, probed=10 (MIN_PROBE_COUNT), latency=0
        submit_attestation(&mut svm, &obs, 0, 10, 0).unwrap();

        let oa = crate::state::ObserverAccount::try_deserialize(
            &mut svm
                .get_account(&get_observer_pda(&obs.pubkey()))
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(oa.attestation_count, 1);
    }

    // ----------------------access and state safety----------------------

    /// Attestation without registration → must fail
    #[test]
    fn test_attestation_without_registration() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let unregistered = Keypair::new();
        svm.airdrop(&unregistered.pubkey(), 10 * LAMPORTS_PER_SOL)
            .unwrap();

        advance_slot(&mut svm, 1);
        let err = submit_attestation(&mut svm, &unregistered, 90, 100, 200);
        assert!(err.is_err(), "unregistered observer must not submit");
    }

    /// Attestation after deregistration → must fail
    #[test]
    fn test_attestation_after_deregistration() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::EU).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 90, 100, 200).unwrap();

        deregister_observer(&mut svm, &obs, &obs.pubkey()).unwrap();

        advance_slot(&mut svm, 1);
        let err = submit_attestation(&mut svm, &obs, 90, 100, 200);
        assert!(
            err.is_err(),
            "submit after deregister must fail (ObserverNotActive)"
        );
    }

    /// Unauthorized deregistration → must fail
    #[test]
    fn test_unauthorized_deregistration() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();

        // Random user tries to deregister obs
        let random = Keypair::new();
        svm.airdrop(&random.pubkey(), 10 * LAMPORTS_PER_SOL)
            .unwrap();
        let err = deregister_observer(&mut svm, &random, &obs.pubkey());
        assert!(
            err.is_err(),
            "random user must not deregister another observer"
        );
    }

    #[test]
    fn test_slash_observer_moves_funds_to_treasury() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1_000, 5);

        let obs = Keypair::new();
        let treasury = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&treasury.pubkey(), 1).unwrap();
        register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();

        let treasury_before = svm.get_balance(&treasury.pubkey()).unwrap();
        slash_observer(&mut svm, &auth, &obs.pubkey(), &treasury.pubkey(), 2_500).unwrap();

        let observer_account = crate::state::ObserverAccount::try_deserialize(
            &mut svm
                .get_account(&get_observer_pda(&obs.pubkey()))
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        assert_eq!(observer_account.stake_lamports, 750);
        assert_eq!(
            svm.get_balance(&treasury.pubkey()).unwrap(),
            treasury_before + 250
        );
        // 750 < min_stake (1_000) → must be deactivated
        assert!(
            !observer_account.is_active,
            "observer should be deactivated when stake drops below minimum"
        );
    }

    #[test]
    fn test_slash_observer_stays_active_above_minimum() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1_000, 5); // Start with 1000 min_stake

        let obs = Keypair::new();
        let treasury = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&treasury.pubkey(), 1).unwrap();
        register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();

        // Lower min_stake to 500 so 750 remaining keeps observer active
        update_config(&mut svm, &auth, Some(500), None, None).unwrap();

        slash_observer(&mut svm, &auth, &obs.pubkey(), &treasury.pubkey(), 2_500).unwrap();

        let observer_account = crate::state::ObserverAccount::try_deserialize(
            &mut svm
                .get_account(&get_observer_pda(&obs.pubkey()))
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        assert_eq!(observer_account.stake_lamports, 750);
        assert!(
            observer_account.is_active,
            "observer should remain active when stake stays above minimum"
        );
    }

    #[test]
    fn test_slash_observer_requires_authority() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1_000, 5);

        let obs = Keypair::new();
        let attacker = Keypair::new();
        let treasury = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&attacker.pubkey(), 10 * LAMPORTS_PER_SOL)
            .unwrap();
        svm.airdrop(&treasury.pubkey(), 1).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        let err = slash_observer(
            &mut svm,
            &attacker,
            &obs.pubkey(),
            &treasury.pubkey(),
            1_000,
        );
        assert!(err.is_err(), "non-authority must not slash observer");
    }

    /// Insufficient stake → registration must fail
    #[test]
    fn test_insufficient_stake() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 5 * LAMPORTS_PER_SOL, 5);

        let obs = Keypair::new();
        // Give only 1 SOL, but min_stake is 5 SOL
        svm.airdrop(&obs.pubkey(), LAMPORTS_PER_SOL).unwrap();
        let err = register_observer(&mut svm, &obs, crate::Region::US);
        assert!(err.is_err(), "insufficient stake must fail registration");
    }

    // ----------------------aggregation correctness----------------------

    /// Total attestation counter increments correctly across observers
    #[test]
    fn test_total_attestation_counter() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 10);

        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        let obs3 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs3.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &obs2, crate::Region::US).unwrap();
        register_observer(&mut svm, &obs3, crate::Region::EU).unwrap();

        let mut expected_total = 0u64;
        for _ in 0..5 {
            advance_slot(&mut svm, 1);
            submit_attestation(&mut svm, &obs1, 90, 100, 200).unwrap();
            expected_total += 1;
            submit_attestation(&mut svm, &obs2, 85, 100, 250).unwrap();
            expected_total += 1;
            submit_attestation(&mut svm, &obs3, 80, 100, 300).unwrap();
            expected_total += 1;
        }

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(health.total_attestations, expected_total);
    }

    /// Health recomputation consistency: no sudden spikes without reason
    #[test]
    fn test_health_recomputation_consistency() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        // Submit identical data 5 times → score must remain stable
        let mut scores = Vec::new();
        for _ in 0..5 {
            advance_slot(&mut svm, 1);
            submit_attestation(&mut svm, &obs, 90, 100, 200).unwrap();
            let health = crate::state::NetworkHealthAccount::try_deserialize(
                &mut svm
                    .get_account(&get_network_health_pda())
                    .unwrap()
                    .data
                    .as_ref(),
            )
            .unwrap();
            scores.push(health.health_score);
        }

        // All scores should be identical (same input = same output)
        for s in &scores {
            assert_eq!(
                *s, scores[0],
                "identical inputs must produce identical scores"
            );
        }
    }

    /// Min/max health tracking
    #[test]
    fn test_min_max_health_tracking() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();

        // Submit high-quality data → sets initial min/max
        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 100, 100, 0).unwrap();
        let h1 = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        let high_score = h1.health_score;

        // Submit poor-quality data → should update min
        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 10, 100, 399).unwrap();
        let h2 = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        let low_score = h2.health_score;

        assert!(
            h2.min_health_ever <= low_score,
            "min must track the lowest score"
        );
        assert!(
            h2.max_health_ever >= high_score,
            "max must track the highest score"
        );
        assert!(
            h2.max_health_ever >= h2.min_health_ever,
            "max >= min invariant"
        );
    }

    // ----------------------system behavior----------------------

    /// Overwrite vs accumulate: only latest attestation stored per observer
    #[test]
    fn test_overwrite_not_accumulate() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::EU).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 50, 100, 300).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 95, 100, 100).unwrap();

        let oa = crate::state::ObserverAccount::try_deserialize(
            &mut svm
                .get_account(&get_observer_pda(&obs.pubkey()))
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        // Latest attestation should reflect the SECOND submission, not accumulated
        assert_eq!(
            oa.latest_attestation.tpu_reachable, 95,
            "must store latest, not accumulate"
        );
        assert_eq!(oa.latest_attestation.slot_latency_ms, 100);
        assert_eq!(oa.attestation_count, 2, "count should increment though");
    }

    /// No duplicate counting: same observer doesn't inflate metrics
    #[test]
    fn test_no_duplicate_counting() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 90, 100, 200).unwrap();

        // Same slot → rejected → no inflation
        let _ = submit_attestation(&mut svm, &obs, 90, 100, 200);

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(
            health.total_attestations, 1,
            "rejected tx must not increment counter"
        );
    }

    /// State invariants: active_count <= observer_count, no negatives/overflow
    #[test]
    fn test_state_invariants() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 10);

        let mut observers = Vec::new();
        for _ in 0..5 {
            let obs = Keypair::new();
            svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
            register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();
            observers.push(obs);
        }

        // Check invariants after registration
        let reg = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();
        assert_eq!(reg.observer_count, 5);
        assert_eq!(reg.active_count, 5);
        assert!(reg.active_count <= reg.observer_count, "active <= total");

        // Deregister 2
        deregister_observer(&mut svm, &observers[0], &observers[0].pubkey()).unwrap();
        deregister_observer(&mut svm, &observers[1], &observers[1].pubkey()).unwrap();

        let reg = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();
        assert_eq!(reg.active_count, 3);
        assert_eq!(
            reg.observer_count, 3,
            "observer_count should decrement on deregister"
        );
        assert!(
            reg.active_count <= reg.observer_count,
            "active <= total after deregister"
        );

        // Submit from remaining 3
        advance_slot(&mut svm, 1);
        for obs in observers.iter().skip(2) {
            submit_attestation(&mut svm, obs, 90, 100, 200).unwrap();
        }

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert!(health.health_score <= 100, "score must be in [0,100]");
        assert!(
            health.total_attestations >= 3,
            "total must reflect submissions"
        );
        assert!(
            health.max_health_ever >= health.min_health_ever,
            "max >= min"
        );
    }

    // tests security invariants
    #[test]
    fn test_max_observers_cap_enforced() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 1); // max_obs = 1

        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        let err = register_observer(&mut svm, &obs2, crate::Region::US);
        assert!(err.is_err(), "should fail because max observers is reached");
    }

    // test emergency stop mechanism
    #[test]
    fn test_paused_registryblocks() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();

        // Pause the registry
        update_config(&mut svm, &auth, None, None, Some(true)).unwrap();

        // Register observer must fail
        let obs2 = Keypair::new();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        let err_reg = register_observer(&mut svm, &obs2, crate::Region::US);
        assert!(
            err_reg.is_err(),
            "Registration should be blocked when paused"
        );

        // Submit attestation must fail
        advance_slot(&mut svm, 1);
        let err_submit = submit_attestation(&mut svm, &obs, 100, 100, 200);
        assert!(
            err_submit.is_err(),
            "Submission should be blocked when paused"
        );

        // Crank aggregation must fail
        let err_crank = crank_aggregation(&mut svm, &auth, &[obs.pubkey()]);
        assert!(err_crank.is_err(), "Crank should be blocked when paused");
    }

    // tests crank with all stale observers
    #[test]
    fn test_crank_with_all_stale() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &obs2, crate::Region::EU).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs1, 100, 100, 200).unwrap();
        submit_attestation(&mut svm, &obs2, 90, 100, 300).unwrap();

        // Advance slot enough to make them stale
        // 150 slot threshold
        advance_slot(&mut svm, 200);

        let err_crank = crank_aggregation(&mut svm, &auth, &[obs1.pubkey(), obs2.pubkey()]);
        assert!(
            err_crank.is_err(),
            "Crank should fail if all observed nodes are stale"
        );
    }

    // tests region score updates
    #[test]
    fn test_crank_updates_region_scores_correctly() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &obs2, crate::Region::EU).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs1, 90, 100, 200).unwrap();
        submit_attestation(&mut svm, &obs2, 80, 100, 300).unwrap();

        crank_aggregation(&mut svm, &auth, &[obs1.pubkey(), obs2.pubkey()]).unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        let asia_score = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::Asia)
            .unwrap();
        let eu_score = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::EU)
            .unwrap();
        let us_score = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::US)
            .unwrap();

        assert!(asia_score.health_score > 0, "Asia score should be updated");
        assert!(eu_score.health_score > 0, "EU score should be updated");
        assert_eq!(
            us_score.last_updated_slot, 0,
            "US score should remain un-updated"
        );
    }

    #[test]
    fn test_same_region_observers_are_averaged_instead_of_overwritten() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &obs2, crate::Region::Asia).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs1, 90, 100, 200).unwrap();
        submit_attestation(&mut svm, &obs2, 80, 100, 300).unwrap();

        let mut health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        let asia_score = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::Asia)
            .unwrap();

        assert_eq!(asia_score.observer_count, 2);
        assert_eq!(asia_score.reachability_pct, 85);
        assert_eq!(asia_score.slot_latency_ms, 250);
        assert_eq!(asia_score.health_score, 70);

        crank_aggregation(&mut svm, &auth, &[obs1.pubkey(), obs2.pubkey()]).unwrap();

        health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        let asia_score = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::Asia)
            .unwrap();

        assert_eq!(asia_score.observer_count, 2);
        assert_eq!(asia_score.reachability_pct, 85);
        assert_eq!(asia_score.slot_latency_ms, 250);
        assert_eq!(asia_score.health_score, 70);
    }

    // tests min max score tracking
    #[test]
    fn test_min_max_survives_crank() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::US).unwrap();

        // Round 1
        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 100, 100, 0).unwrap();
        crank_aggregation(&mut svm, &auth, &[obs.pubkey()]).unwrap();

        let h1 = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        let score1 = h1.health_score;

        // Round 2
        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 10, 100, 399).unwrap();
        crank_aggregation(&mut svm, &auth, &[obs.pubkey()]).unwrap();

        let h2 = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        let score2 = h2.health_score;

        assert!(h2.min_health_ever <= score2, "min health track lowest");
        assert!(h2.max_health_ever >= score1, "max health track highest");
        assert!(h2.max_health_ever >= h2.min_health_ever);
    }

    // test update config ix
    #[test]
    fn test_update_config() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 5);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();

        update_config(&mut svm, &auth, Some(3), None, None).unwrap();

        let registry = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();

        assert_eq!(
            registry.min_stake_lamports, 3,
            "Min stake should be updated"
        );

        update_config(&mut svm, &auth, None, Some(10), None).unwrap();

        let registry = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();

        assert_eq!(
            registry.max_observers, 10,
            "Max observers should be updated"
        );
        let registry = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();

        assert_eq!(
            registry.max_observers, 10,
            "Max observers should be updated"
        );

        update_config(&mut svm, &auth, None, None, Some(true)).unwrap();

        let registry = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();

        assert_eq!(registry.paused, true, "Paused should be updated");

        update_config(&mut svm, &auth, None, None, Some(false)).unwrap();

        let registry = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();

        assert_eq!(registry.paused, false, "Paused should be unpaused");
    }

    #[test]
    fn test_active_observer_count_vs_active_region_count() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 10);

        // 3 observers, 2 in Asia and 1 in EU → 3 observers but only 2 regions
        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        let obs3 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs3.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &obs2, crate::Region::Asia).unwrap(); // same region as obs1
        register_observer(&mut svm, &obs3, crate::Region::EU).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs1, 90, 100, 200).unwrap();
        submit_attestation(&mut svm, &obs2, 80, 100, 300).unwrap();
        submit_attestation(&mut svm, &obs3, 85, 100, 250).unwrap();

        crank_aggregation(
            &mut svm,
            &auth,
            &[obs1.pubkey(), obs2.pubkey(), obs3.pubkey()],
        )
        .unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        // The key assertion: these must differ — 3 observers across 2 regions
        assert_eq!(health.active_observer_count, 3, "3 observers submitted");
        assert_eq!(
            health.active_region_count, 2,
            "only 2 distinct regions active"
        );
        assert_ne!(
            health.active_observer_count, health.active_region_count,
            "observer count and region count must differ when multiple observers share a region"
        );

        // Also verify the Asia region aggregated correctly (avg of obs1+obs2)
        let asia = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::Asia)
            .unwrap();
        assert_eq!(asia.observer_count, 2, "Asia should have 2 observers");
        assert_eq!(
            asia.reachability_pct, 85,
            "Asia reachability should be averaged"
        );
    }

    #[test]
    fn test_transfer_authority() {
        let (mut svm, auth) = setup();
        init_protocol(&mut svm, &auth, 1, 10);

        let new_auth = Keypair::new();
        svm.airdrop(&new_auth.pubkey(), 10 * LAMPORTS_PER_SOL)
            .unwrap();

        propose_authority(&mut svm, &auth, &new_auth.pubkey()).unwrap();

        let registry = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();
        assert_eq!(
            registry.pending_authority,
            Some(anchor_lang::prelude::Pubkey::new_from_array(
                new_auth.pubkey().to_bytes()
            ))
        );

        let random_user = Keypair::new();
        svm.airdrop(&random_user.pubkey(), 10 * LAMPORTS_PER_SOL)
            .unwrap();
        let err2 = accept_authority(&mut svm, &random_user);
        assert!(err2.is_err(), "non-pending authority must not accept");

        accept_authority(&mut svm, &new_auth).unwrap();

        let registry = crate::state::RegistryAccount::try_deserialize(
            &mut svm.get_account(&get_registry_pda()).unwrap().data.as_ref(),
        )
        .unwrap();
        assert_eq!(
            registry.authority,
            anchor_lang::prelude::Pubkey::new_from_array(new_auth.pubkey().to_bytes())
        );
        assert_eq!(registry.pending_authority, None);

        let random_auth = Keypair::new();
        let err = propose_authority(&mut svm, &auth, &random_auth.pubkey());
        assert!(err.is_err(), "old authority must not propose");
    }

    /// Single observer submits a known client mix.
    /// Region averages must equal the submitted mix (count=1 → avg=value).
    #[test]
    fn test_client_distribution_region_average() {
        let (mut svm, authority) = setup();
        init_protocol(&mut svm, &authority, 1, 10);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::Asia).unwrap();

        advance_slot(&mut svm, 1);

        // mix: agave=50, fd=30, jito=10, labs=5, other=5
        submit_attestation_with_clients(&mut svm, &obs, 90, 100, 200, 50, 30, 10, 5, 5).unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        let asia = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::Asia)
            .expect("asia slot");

        assert_eq!(asia.observer_count, 1);
        assert_eq!(asia.agave_count, 50);
        assert_eq!(asia.firedancer_count, 30);
        assert_eq!(asia.jito_count, 10);
        assert_eq!(asia.solana_labs_count, 5);
        assert_eq!(asia.other_count, 5);
    }

    /// Same observer submits twice with different mixes.
    /// Region totals must reflect only the latest, not the sum.
    #[test]
    fn test_client_distribution_subtract_on_resubmit() {
        let (mut svm, authority) = setup();
        init_protocol(&mut svm, &authority, 1, 10);

        let obs = Keypair::new();
        svm.airdrop(&obs.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        register_observer(&mut svm, &obs, crate::Region::EU).unwrap();

        advance_slot(&mut svm, 1);
        submit_attestation_with_clients(&mut svm, &obs, 90, 100, 200, 80, 10, 5, 3, 2).unwrap();

        advance_slot(&mut svm, 1);
        // resubmit with different mix
        submit_attestation_with_clients(&mut svm, &obs, 95, 100, 180, 40, 40, 10, 5, 5).unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        let eu = health
            .region_scores
            .iter()
            .find(|rs| rs.region == crate::Region::EU)
            .expect("eu slot");

        assert_eq!(eu.observer_count, 1, "still 1 observer, not 2");
        // averages = totals / 1 → equal to latest mix
        assert_eq!(eu.agave_count, 40);
        assert_eq!(eu.firedancer_count, 40);
        assert_eq!(eu.jito_count, 10);
        assert_eq!(eu.solana_labs_count, 5);
        assert_eq!(eu.other_count, 5);
        // running totals must also equal the latest, not first+second
        assert_eq!(eu.total_agave_count, 40);
        assert_eq!(eu.total_firedancer_count, 40);
    }

    /// 2 observers in 2 regions with different mixes.
    /// Global pcts must equal average of region averages.
    #[test]
    fn test_client_distribution_global_pct() {
        let (mut svm, authority) = setup();
        init_protocol(&mut svm, &authority, 1, 10);

        let obs1 = Keypair::new();
        let obs2 = Keypair::new();
        svm.airdrop(&obs1.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        svm.airdrop(&obs2.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

        register_observer(&mut svm, &obs1, crate::Region::Asia).unwrap();
        register_observer(&mut svm, &obs2, crate::Region::US).unwrap();

        advance_slot(&mut svm, 1);

        // Asia: agave=60 fd=20 jito=10 labs=5 other=5
        submit_attestation_with_clients(&mut svm, &obs1, 90, 100, 200, 60, 20, 10, 5, 5).unwrap();
        // US:   agave=40 fd=40 jito=10 labs=5 other=5
        submit_attestation_with_clients(&mut svm, &obs2, 95, 100, 180, 40, 40, 10, 5, 5).unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();

        // global avg = (asia_avg + us_avg) / 2
        assert_eq!(health.agave_pct, (60 + 40) / 2);
        assert_eq!(health.firedancer_pct, (20 + 40) / 2);
        assert_eq!(health.jito_pct, (10 + 10) / 2);
        assert_eq!(health.solana_labs_pct, (5 + 5) / 2);
        assert_eq!(health.other_pct, (5 + 5) / 2);

        // crank should produce same result
        crank_aggregation(&mut svm, &authority, &[obs1.pubkey(), obs2.pubkey()]).unwrap();

        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert_eq!(health.agave_pct, 50);
        assert_eq!(health.firedancer_pct, 30);
    }
}
