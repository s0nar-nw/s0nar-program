use anchor_lang::prelude::*;

#[error_code]
pub enum NeutronErrors {
    #[msg("Value cannot be zero")]
    ValueCannotBeZero,
}
