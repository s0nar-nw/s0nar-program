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

    #[msg("Unauthorized Observer")]
    UnauthorizedObserver,

    #[msg("Observer is not active")]
    ObserverNotActive,

    #[msg("Zero validators probed")]
    ZeroValidatorsProbed,

    #[msg("Invalid reachability count")]
    InvalidReachabilityCount,

    #[msg("Stale attestation")]
    StaleAttestation,
}
