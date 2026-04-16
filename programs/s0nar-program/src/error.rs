use anchor_lang::prelude::*;

#[error_code]
pub enum CustomErrors {
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

    #[msg("Insufficient validators probed")]
    InsufficientValidatorsProbed,

    #[msg("Invalid reachability count")]
    InvalidReachabilityCount,

    #[msg("Invalid latency submitted")]
    InvalidLatencyValue,

    #[msg("Stale attestation")]
    StaleAttestation,

    #[msg("No active observers")]
    NoActiveObservers,

    #[msg("Observer already inactive")]
    ObserverAlreadyInActive,

    #[msg("Unauthorized caller")]
    UnAuthorizedCaller,

    #[msg("Insufficient balance in PDA for stake refund")]
    InsufficientBalanceForRefund,

    #[msg("Invalid slash basis points - must be <= 10000")]
    InvalidSlashBps,

    #[msg("Observer not found")]
    ObserverNotFound,

    #[msg("Insufficient balance in PDA for slash")]
    InsufficientBalanceForSlash,

    #[msg("Invalid or no pending authority for registry")]
    InvalidPendingAuthority,

    #[msg("Max observers cannot be less than active observers")]
    MaxObserversCannotBeLessThanActiveObservers,
}
