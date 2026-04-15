use crate::{error::CustomErrors, ObserverAccount, RegistryAccount, OBSERVER_SEED, REGISTRY_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SlashObserver<'info> {
    pub authority: Signer<'info>,

    /// CHECK: Verified implicitly - observer_account PDA is derived from this key.
    pub observer_wallet: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [OBSERVER_SEED, observer_wallet.key().as_ref()],
        bump = observer_account.bump,
        constraint = observer_account.is_active @ CustomErrors::ObserverNotActive,
    )]
    pub observer_account: Account<'info, ObserverAccount>,

    #[account(
        mut,
        seeds = [REGISTRY_SEED],
        bump = registry.bump,
        has_one = authority @ CustomErrors::UnAuthorizedCaller,
    )]
    pub registry: Account<'info, RegistryAccount>,

    /// CHECK: Treasury can be any lamport-receiving account selected by the authority.
    #[account(mut)]
    pub treasury: AccountInfo<'info>,
}

pub fn slash(ctx: Context<SlashObserver>, slash_bps: u16) -> Result<()> {
    require!(slash_bps <= 10_000, CustomErrors::InvalidSlashBps);

    let observer_account = &mut ctx.accounts.observer_account;
    let slash_amount = observer_account
        .stake_lamports
        .saturating_mul(slash_bps as u64)
        .saturating_div(10_000);

    if slash_amount == 0 {
        return Ok(());
    }

    let observer_info = &mut observer_account.to_account_info();
    let treasury_info = &mut ctx.accounts.treasury.to_account_info();
    let current_pda_balance = observer_info.lamports();
    let rent_exempt_min = Rent::get()?.minimum_balance(observer_info.data_len());

    require!(
        current_pda_balance >= rent_exempt_min + slash_amount,
        CustomErrors::InsufficientBalanceForSlash
    );

    **observer_info.try_borrow_mut_lamports()? -= slash_amount;
    **treasury_info.try_borrow_mut_lamports()? += slash_amount;

    observer_account.stake_lamports = observer_account.stake_lamports.saturating_sub(slash_amount);

    if observer_account.stake_lamports < ctx.accounts.registry.min_stake_lamports {
        observer_account.is_active = false;
        ctx.accounts.registry.active_count = ctx.accounts.registry.active_count.saturating_sub(1);
    }

    Ok(())
}
