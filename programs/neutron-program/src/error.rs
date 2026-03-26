use anchor_lang::prelude::*;

#[error_code]
pub enum NeutronErrors {
    #[msg("Value cannot be zero")]
    ValueCannotBeZero,

    #[msg("Registry is currently paused")]
    RegistryPaused,

    #[msg("Maximum number of observers has been reached")]
    MaxObserversReached,

    #[msg("Insufficient Lamports")]
    InsufficientLamports,
}
