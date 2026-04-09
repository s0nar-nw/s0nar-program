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
            }
            .data(),
        };
        send_tx(svm, &[ix], &[obs])
    }

    /// Returns Ok if crank succeeded, Err if the transaction failed (e.g. NoActiveObservers).
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

        // latency = very high → latency component = 0
        advance_slot(&mut svm, 1);
        submit_attestation(&mut svm, &obs, 100, 100, u32::MAX).unwrap();
        let health = crate::state::NetworkHealthAccount::try_deserialize(
            &mut svm
                .get_account(&get_network_health_pda())
                .unwrap()
                .data
                .as_ref(),
        )
        .unwrap();
        assert!(
            health.health_score <= 100,
            "extreme latency must not cause overflow"
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
        // Smallest valid: reachable=0, probed=1, latency=0
        submit_attestation(&mut svm, &obs, 0, 1, 0).unwrap();

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
}
