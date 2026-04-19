import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { S0narProgram } from "../target/types/s0nar_program";
import { PublicKey } from "@solana/web3.js";

module.exports = async function (provider: anchor.AnchorProvider) {
  anchor.setProvider(provider);

  const program = anchor.workspace.S0narProgram as Program<S0narProgram>;

  const [registryPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("registry")],
    program.programId
  );

  const [networkHealthPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("network_health")],
    program.programId
  );

  console.log("Program ID:       ", program.programId.toBase58());
  console.log("Registry PDA:     ", registryPda.toBase58());
  console.log("NetworkHealth PDA:", networkHealthPda.toBase58());
  console.log("Authority:        ", provider.wallet.publicKey.toBase58());

  // Check if already initialized — skip if so
  try {
    const existing = await program.account.registryAccount.fetch(registryPda);
    console.log(
      "Already initialized — authority:",
      existing.authority.toBase58()
    );
    return;
  } catch {
    console.log("Registry not found, initializing...");
  }

  const minStakeLamports = new anchor.BN(1_000_000); // 0.001 SOL
  const maxObservers = 100;

  const tx = await program.methods
    .initialize(minStakeLamports, maxObservers)
    .accounts({
      authority: provider.wallet.publicKey,
    })
    .rpc();

  console.log("Initialized! tx:", tx);
  console.log(
    "min_stake_lamports:",
    minStakeLamports.toString(),
    "(0.001 SOL)"
  );
  console.log("max_observers:", maxObservers);
};
