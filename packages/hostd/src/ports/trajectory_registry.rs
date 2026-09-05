//! Read-side trajectory registry port (F-36).
//!
//! The concrete per-session registry lives in `crate::infra::trajectory` and
//! is owned by the turn runner; application and protocol layers reach it
//! only through this trait so they never depend on `crate::infra`.

use std::collections::HashMap;
use std::sync::Arc;

use piko_orchd_api::TrajectoryCapturePort;

/// Read-only view over the shared per-session trajectory recorders.
pub trait TrajectoryRegistryPort: Send + Sync {
    fn get(&self, session_id: &str) -> Option<Arc<dyn TrajectoryCapturePort>>;

    /// Per-run dropped-record counts for one session.
    fn dropped_counts(&self, session_id: &str) -> HashMap<String, u32>;
}

/// No-op registry used by runners that do not capture trajectories.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopTrajectoryRegistry;

impl TrajectoryRegistryPort for NoopTrajectoryRegistry {
    fn get(&self, _session_id: &str) -> Option<Arc<dyn TrajectoryCapturePort>> {
        None
    }

    fn dropped_counts(&self, _session_id: &str) -> HashMap<String, u32> {
        HashMap::new()
    }
}
