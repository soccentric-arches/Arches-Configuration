//! SoCcentric WebConfig config engine.
//!
//! The engine is the canonical config store (spec §3.2): a single versioned
//! JSON tree persisted at `/data/webconfig/config.json` with atomic writes and
//! a backup copy. The UI never edits `/etc` directly — instead **appliers**
//! translate the store → system.
//!
//! This crate is deliberately free of any HTTP / async-runtime code so the
//! whole engine can be unit-tested against a tempdir fake sysroot on x86 CI
//! with no hardware (spec §6). The HTTP layer in `webconfigd` talks to the
//! engine over a channel and runs appliers in a dedicated blocking task.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations, rust_2018_idioms)]

pub mod applier;
pub mod appliers;
pub mod audit;
pub mod alert;
pub mod manifest;
pub mod monitoring;
pub mod schema;
pub mod store;
pub mod sysroot;
pub mod update;
pub mod users;

pub use applier::{Applier, ApplyError, ApplyOutcome, ApplyStage};
pub use manifest::{BoardManifest, IoKind, Peripheral};
pub use schema::Schema;
pub use store::{ConfigStore, StoreError};
pub use sysroot::Sysroot;

/// Canonical schema version of the config tree. Bumped only on a breaking
/// shape change; restore tolerates version skew by dropping unknown fields
/// (spec §4.4 backup/restore).
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// Re-export the root config model so callers don't need to reach into `store`.
pub use store::Config;