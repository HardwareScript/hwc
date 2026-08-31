/// Continuous 2D Vector in physical nanometer/picometer space
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector2D {
    pub x: f64,
    pub y: f64,
}

impl Vector2D {
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn magnitude(&self) -> f64 {
        self.x.hypot(self.y)
    }

    #[inline]
    pub fn unit(&self) -> Result<Self, String> {
        let mag = self.magnitude();
        if mag <= 1e-9 {
            return Err("Cannot normalize zero-length vector (degenerate terminal centroids)".into());
        }
        Ok(Self {
            x: self.x / mag,
            y: self.y / mag,
        })
    }

    #[inline]
    pub fn dot(&self, other: &Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Transverse orthogonal normal vector: (-y, x)
    #[inline]
    pub fn normal(&self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }
}

/// Strongly-typed Conduction Flux defining the carrier transport vector
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConductionFlux {
    pub from_centroid: Vector2D,
    pub to_centroid: Vector2D,
    pub flux_vector: Vector2D,
    pub unit_flux: Vector2D,
    pub unit_transverse: Vector2D,
}

impl ConductionFlux {
    pub fn from_centroids(from_centroid: Vector2D, to_centroid: Vector2D) -> Result<Self, String> {
        let flux_vector = Vector2D::new(
            to_centroid.x - from_centroid.x,
            to_centroid.y - from_centroid.y,
        );

        let unit_flux = if flux_vector.magnitude() > 1e-6 {
            flux_vector.unit()?
        } else {
            Vector2D::new(1.0, 0.0)
        };
        let unit_transverse = unit_flux.normal();

        Ok(Self {
            from_centroid,
            to_centroid,
            flux_vector,
            unit_flux,
            unit_transverse,
        })
    }
}
