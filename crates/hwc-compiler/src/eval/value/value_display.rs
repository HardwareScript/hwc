use hwc_types::SiDimension;
use super::value_enum::Value;

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Void => write!(f, "()"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::String(s) => write!(f, "{}", s),
            Value::Measurement(m) => {
                // Human-readable engineering unit formatting
                if m.dimension == SiDimension::LENGTH {
                    let pm = m.raw;
                    if pm.abs() >= 1_000_000_000_000 {
                        write!(f, "{:.2}m", pm as f64 / 1e12)
                    } else if pm.abs() >= 1_000_000_000 {
                        write!(f, "{:.2}mm", pm as f64 / 1e9)
                    } else if pm.abs() >= 1_000_000 {
                        write!(f, "{:.2}um", pm as f64 / 1e6)
                    } else if pm.abs() >= 1_000 {
                        write!(f, "{:.2}nm", pm as f64 / 1e3)
                    } else {
                        write!(f, "{}pm", pm)
                    }
                } else if m.dimension == SiDimension::VOLTAGE {
                    let nv = m.raw;
                    if nv.abs() >= 1_000_000_000 {
                        write!(f, "{:.2}kV", nv as f64 / 1e12)
                    } else if nv.abs() >= 1_000_000 {
                        write!(f, "{:.2}V", nv as f64 / 1e9)
                    } else if nv.abs() >= 1_000 {
                        write!(f, "{:.2}mV", nv as f64 / 1e6)
                    } else {
                        write!(f, "{:.2}uV", nv as f64 / 1e3)
                    }
                } else if m.dimension == SiDimension::CURRENT {
                    let pa = m.raw;
                    if pa.abs() >= 1_000_000_000_000 {
                        write!(f, "{:.2}A", pa as f64 / 1e12)
                    } else if pa.abs() >= 1_000_000_000 {
                        write!(f, "{:.2}mA", pa as f64 / 1e9)
                    } else if pa.abs() >= 1_000_000 {
                        write!(f, "{:.2}uA", pa as f64 / 1e6)
                    } else if pa.abs() >= 1_000 {
                        write!(f, "{:.2}nA", pa as f64 / 1e3)
                    } else {
                        write!(f, "{}pA", pa)
                    }
                } else if m.dimension == SiDimension::RESISTANCE || m.dimension == SiDimension::SHEET_RES {
                    let uohm = m.raw;
                    if uohm.abs() >= 1_000_000_000 {
                        write!(f, "{:.2}MOhm", uohm as f64 / 1e9)
                    } else if uohm.abs() >= 1_000_000 {
                        write!(f, "{:.2}kOhm", uohm as f64 / 1e6)
                    } else if uohm.abs() >= 1_000 {
                        write!(f, "{:.2}Ohm", uohm as f64 / 1e3)
                    } else {
                        write!(f, "{:.2}mOhm", uohm as f64 / 1.0)
                    }
                } else if m.dimension == SiDimension::CAPACITANCE || m.dimension == SiDimension::CAPACITANCE_DENSITY {
                    let af = m.raw;
                    if af.abs() >= 1_000_000_000_000 {
                        write!(f, "{:.2}uF", af as f64 / 1e18)
                    } else if af.abs() >= 1_000_000_000 {
                        write!(f, "{:.2}nF", af as f64 / 1e15)
                    } else if af.abs() >= 1_000_000 {
                        write!(f, "{:.2}pF", af as f64 / 1e12)
                    } else if af.abs() >= 1_000 {
                        write!(f, "{:.2}fF", af as f64 / 1e3)
                    } else {
                        write!(f, "{}aF", af)
                    }
                } else if m.dimension == SiDimension::INDUCTANCE {
                    let ph = m.raw;
                    if ph.abs() >= 1_000_000_000 {
                        write!(f, "{:.2}mH", ph as f64 / 1e12)
                    } else if ph.abs() >= 1_000_000 {
                        write!(f, "{:.2}uH", ph as f64 / 1e9)
                    } else if ph.abs() >= 1_000 {
                        write!(f, "{:.2}nH", ph as f64 / 1e6)
                    } else {
                        write!(f, "{}pH", ph)
                    }
                } else if m.dimension == SiDimension::TIME {
                    let fs = m.raw;
                    if fs.abs() >= 1_000_000_000_000 {
                        write!(f, "{:.2}s", fs as f64 / 1e15)
                    } else if fs.abs() >= 1_000_000_000 {
                        write!(f, "{:.2}ms", fs as f64 / 1e12)
                    } else if fs.abs() >= 1_000_000 {
                        write!(f, "{:.2}us", fs as f64 / 1e9)
                    } else if fs.abs() >= 1_000 {
                        write!(f, "{:.2}ns", fs as f64 / 1e6)
                    } else {
                        write!(f, "{}fs", fs)
                    }
                } else if m.dimension == SiDimension::FREQUENCY {
                    let hz = m.raw as f64;
                    if hz.abs() >= 1e9 {
                        write!(f, "{:.2}GHz", hz / 1e9)
                    } else if hz.abs() >= 1e6 {
                        write!(f, "{:.2}MHz", hz / 1e6)
                    } else if hz.abs() >= 1e3 {
                        write!(f, "{:.2}kHz", hz / 1e3)
                    } else {
                        write!(f, "{:.2}Hz", hz)
                    }
                } else if m.dimension == SiDimension::POWER {
                    let pw = m.raw;
                    if pw.abs() >= 1_000_000_000 {
                        write!(f, "{:.2}mW", pw as f64 / 1e12)
                    } else if pw.abs() >= 1_000_000 {
                        write!(f, "{:.2}uW", pw as f64 / 1e9)
                    } else if pw.abs() >= 1_000 {
                        write!(f, "{:.2}nW", pw as f64 / 1e6)
                    } else {
                        write!(f, "{}pW", pw)
                    }
                } else if m.dimension == SiDimension::ANGLE {
                    let udeg = m.raw;
                    write!(f, "{:.2}deg", udeg as f64 / 1e6)
                } else if m.dimension == SiDimension::TEMPERATURE {
                    let mk = m.raw;
                    write!(f, "{:.2}K", mk as f64 / 1e3)
                } else if m.dimension == SiDimension::AREA {
                    let pm2 = m.raw;
                    if pm2.abs() >= 1_000_000_000_000_000_000_000_000 {
                        write!(f, "{:.2}m^2", pm2 as f64 / 1e24)
                    } else if pm2.abs() >= 1_000_000_000_000_000_000 {
                        write!(f, "{:.2}mm^2", pm2 as f64 / 1e18)
                    } else if pm2.abs() >= 1_000_000_000_000 {
                        write!(f, "{:.2}um^2", pm2 as f64 / 1e12)
                    } else if pm2.abs() >= 1_000_000 {
                        write!(f, "{:.2}nm^2", pm2 as f64 / 1e6)
                    } else {
                        write!(f, "{}pm^2", pm2)
                    }
                } else if m.dimension == SiDimension::VOLUME {
                    let pm3 = m.raw;
                    write!(f, "{}pm^3", pm3)
                } else {
                    write!(f, "{:?}({})", m.dimension, m.raw)
                }
            }
            Value::Point2D { x, y } => write!(f, "Point2D[{}, {}]", x, y),
            Value::Point3D { x, y, z } => write!(f, "Point3D[{}, {}, {}]", x, y, z),
            Value::Vector2D { dx, dy } => write!(f, "Vector2D[{}, {}]", dx, dy),
            Value::BoundingBox {
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                write!(
                    f,
                    "BoundingBox[{}, {}, {}, {}]",
                    min_x, min_y, max_x, max_y
                )
            }
            Value::CellLayout(cell) => {
                write!(f, "<CellLayout '{}' ({} polys, {} ports)>", cell.name, cell.polygons.len(), cell.ports.len())
            }
            Value::PlacedCell(inst) => {
                write!(f, "<PlacedCell '{}' at ({}, {})>", inst.cell.name, inst.placement_x, inst.placement_y)
            }
            Value::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            Value::StructInstance { name, fields } => {
                write!(f, "{} {{ ", name)?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, " }}")
            }
            Value::EnumVariant {
                enum_name,
                variant_name,
                payload,
            } => match payload {
                Some(p) => write!(f, "{}::{}({:?})", enum_name, variant_name, p),
                None => write!(f, "{}::{}", enum_name, variant_name),
            },
            Value::EnumType { name, .. } => write!(f, "<enum {}>", name),
            Value::FunctionRef(id) => write!(f, "<fn {:?}>", id),
            Value::NetHandle(id) => write!(f, "<net #{}>", id.0),
            Value::SpaceHandle(id) => write!(f, "<space #{}>", id.0),
            Value::DeviceHandle(id) => write!(f, "<device #{}>", id.0),
            Value::PlacedPort(p) => write!(f, "{}.port(\"{}\")", p.instance_name, p.port_name),
        }
    }
}
