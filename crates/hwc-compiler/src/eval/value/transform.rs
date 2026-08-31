/// 2D Transformation (Rotation, Mirror, Translation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Transform2D {
    pub rotation_deg: i32,
    pub mirror_x: bool,
    pub mirror_y: bool,
    pub offset_x: i64,
    pub offset_y: i64,
}

impl Transform2D {
    pub fn apply_point(&self, pt: (i64, i64)) -> (i64, i64) {
        let mut x = pt.0;
        let mut y = pt.1;

        if self.mirror_x {
            y = -y;
        }
        if self.mirror_y {
            x = -x;
        }

        let rot = ((self.rotation_deg % 360) + 360) % 360;
        let (rx, ry) = match rot {
            90 => (-y, x),
            180 => (-x, -y),
            270 => (y, -x),
            _ => {
                if rot != 0 {
                    let rad = (rot as f64) * std::f64::consts::PI / 180.0;
                    let cos_r = rad.cos();
                    let sin_r = rad.sin();
                    let nx = (x as f64 * cos_r - y as f64 * sin_r).round() as i64;
                    let ny = (x as f64 * sin_r + y as f64 * cos_r).round() as i64;
                    (nx, ny)
                } else {
                    (x, y)
                }
            }
        };

        (rx + self.offset_x, ry + self.offset_y)
    }
}
