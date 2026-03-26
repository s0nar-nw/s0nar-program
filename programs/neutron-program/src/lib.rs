pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("Au4AWwhGvJFpxgJh3Qe83V8Z4emdd3CoE7EVoSiR5P5L");

#[program]
pub mod neutron_program {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        min_stake_lamports: u64,
        max_observers: u16,
    ) -> Result<()> {
        initialize::init(ctx, min_stake_lamports, max_observers)
    }

    pub fn register_observer(ctx: Context<RegisterObserver>, region: Region) -> Result<()> {
        register_observer::register(ctx, region)
    }
}
