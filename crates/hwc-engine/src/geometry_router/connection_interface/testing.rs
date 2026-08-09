//! Default routing database for testing and unit test suite.

use crate::geometry::Point3D;

use super::types::RoutingDatabase;

/// Default routing database with typical copper technology parameters.
///
/// Used for unit tests and as a reference implementation. Real designs
/// should use the Salsa-tracked query engine.
pub struct DefaultRoutingDatabase {
    /// Max current density in uA/nm (typical copper: 2 mA/μm² = 2 uA/nm)
    pub max_current_density_ua_per_nm: i64,
}

impl Default for DefaultRoutingDatabase {
    fn default() -> Self {
        Self {
            max_current_density_ua_per_nm: 2,
        }
    }
}

impl RoutingDatabase for DefaultRoutingDatabase {
    fn get_max_current_density_ua_per_nm(&self) -> i64 {
        self.max_current_density_ua_per_nm
    }

    fn get_local_temperature_at(&self, _pos: Point3D) -> i64 {
        300_000 // 300K in millikelvin
    }

    fn get_current_density_at(&self, _pos: Point3D) -> i64 {
        0
    }

    fn get_nearest_parallel_trace_distance(&self, _pos: Point3D) -> i64 {
        i64::MAX
    }

    fn is_in_reference_void(&self, _pos: Point3D) -> bool {
        false
    }
}
