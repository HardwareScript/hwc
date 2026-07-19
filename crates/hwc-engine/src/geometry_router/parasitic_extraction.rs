use std::fmt::Write;

use crate::geometry_router::route_decomposition::VirtualJunction;
use crate::geometry_router::spatial_index::IndexedSegment;

const EPS_0: f64 = 8.854187817e-12;
const MU_0: f64 = 1.2566370614e-6;
const COPPER_RESISTIVITY: f64 = 1.68e-8;

/// Extraction parameters for a PCB/APCB stackup.
#[derive(Clone, Debug)]
pub struct ExtractionParams {
    pub freq_hz: f64,
    pub substrate_er: f64,
    pub substrate_height_m: f64,
    pub trace_thickness_m: f64,
    pub loss_tangent: f64,
}

/// Wheeler effective permittivity for microstrip.
///
/// `er_eff = (er + 1)/2 + (er - 1)/2 * (1 + 12*h/w)^(-0.5)`
///
/// - `er`: substrate relative permittivity
/// - `w`: trace width in meters
/// - `h`: substrate height in meters
#[inline]
pub fn wheeler_effective_permittivity(er: f64, w: f64, h: f64) -> f64 {
    if w <= 0.0 || h <= 0.0 {
        return er;
    }
    let ratio = 12.0 * h / w;
    (er + 1.0) / 2.0 + (er - 1.0) / 2.0 * (1.0 + ratio).powf(-0.5)
}

/// Sakurai coupling capacitance C12 between two parallel traces.
///
/// Ground-plane-aware with effective permittivity.
/// `C12 = er_eff * eps0 * t / s`
///
/// - `w`: trace width in meters
/// - `t`: trace thickness in meters
/// - `s`: spacing between traces in meters
/// - `h`: substrate height in meters
/// - `er_eff`: effective permittivity
#[inline]
pub fn sakurai_coupling_c12(w: f64, t: f64, s: f64, h: f64, er_eff: f64) -> f64 {
    if s <= 0.0 || t <= 0.0 {
        return 0.0;
    }
    let _ = (w, h); // used for field fringing in full model; simplified here
    er_eff * EPS_0 * t / s
}

/// Sakurai ground capacitance C1g for a single trace over ground plane.
///
/// `C1g = er_eff * eps0 * w / h`
///
/// - `w`: trace width in meters
/// - `h`: substrate height in meters
/// - `er_eff`: effective permittivity
#[inline]
pub fn sakurai_ground_capacitance(w: f64, h: f64, er_eff: f64) -> f64 {
    if w <= 0.0 || h <= 0.0 {
        return 0.0;
    }
    er_eff * EPS_0 * w / h
}

/// Series resistance of a trace segment.
///
/// `Rs = rho * L / (W * t)`
///
/// - `rho`: resistivity in ohm*m
/// - `length_m`: trace length in meters
/// - `width_m`: trace width in meters
/// - `thickness_m`: trace thickness in meters
#[inline]
pub fn series_resistance(rho: f64, length_m: f64, width_m: f64, thickness_m: f64) -> f64 {
    if width_m <= 0.0 || thickness_m <= 0.0 || length_m <= 0.0 {
        return 0.0;
    }
    rho * length_m / (width_m * thickness_m)
}

/// Via self-inductance (analytical cylinder model).
///
/// `L = mu0 * length / (pi/2) * ln(2*length/diameter)`
///
/// - `diameter_m`: via barrel diameter in meters
/// - `length_m`: via barrel length (layer span) in meters
#[inline]
pub fn via_self_inductance(diameter_m: f64, length_m: f64) -> f64 {
    if diameter_m <= 0.0 || length_m <= 0.0 {
        return 0.0;
    }
    let ratio = 2.0 * length_m / diameter_m;
    if ratio <= 0.0 {
        return 0.0;
    }
    MU_0 * length_m / (std::f64::consts::FRAC_PI_2) * ratio.ln()
}

/// Greenhouse mutual inductance approximation for parallel trace runs.
///
/// - `length_m`: coupled length in meters
/// - `distance_m`: center-to-center distance between traces in meters
#[inline]
pub fn greenhouse_mutual_inductance(length_m: f64, distance_m: f64) -> f64 {
    if length_m <= 0.0 || distance_m <= 0.0 {
        return 0.0;
    }
    MU_0 * length_m / (2.0 * std::f64::consts::PI) * (2.0 * length_m / distance_m).ln()
}

/// Parasitic values at a virtual junction.
#[derive(Clone, Debug)]
pub struct JunctionParasitics {
    pub c_junc: f64,
    pub l_junc: f64,
}

/// Compute lumped parasitics for a virtual T-junction.
///
/// - `c_junc`: lumped capacitance at junction (F)
/// - `l_junc`: series inductance through junction (H)
pub fn junction_parasitics(junction: &VirtualJunction) -> JunctionParasitics {
    let seg_count = junction.connected_segments.len().max(1) as f64;
    let c_junc = junction.capacitance_pf * 1.0e-12 / seg_count;
    let l_junc = junction.inductance_nh * 1.0e-9;
    JunctionParasitics { c_junc, l_junc }
}

/// Extracted parasitics for a single routed trace.
#[derive(Clone, Debug)]
pub struct ExtractedTrace {
    pub net_id: u32,
    pub segments: Vec<ExtractedSegment>,
    pub junction_parasitics: Vec<JunctionParasitics>,
}

/// Parasitic values for a single trace segment.
#[derive(Clone, Debug)]
pub struct ExtractedSegment {
    pub segment_id: usize,
    pub resistance_ohm: f64,
    pub capacitance_f: f64,
    pub inductance_h: f64,
    pub length_m: f64,
}

/// Result of parasitic extraction.
#[derive(Clone, Debug)]
pub struct ExtractionResult {
    pub traces: Vec<ExtractedTrace>,
    pub spice_netlist: String,
    pub extraction_time_ms: u64,
}

/// Convert nanometer dimension to meters.
#[inline]
fn nm_to_m(nm: i64) -> f64 {
    nm as f64 * 1.0e-9
}

/// Extract parasitics for all routed traces.
pub fn extract_parasitics(
    segments: &[IndexedSegment],
    junctions: &[VirtualJunction],
    params: &ExtractionParams,
) -> ExtractionResult {
    let start = std::time::Instant::now();

    let mut traces: Vec<ExtractedTrace> = Vec::new();
    let mut net_segments: Vec<Vec<&IndexedSegment>> = Vec::new();
    let mut net_ids: Vec<u32> = Vec::new();

    for seg in segments {
        let net = seg.net_id as u32;
        if let Some(idx) = net_ids.iter().position(|&n| n == net) {
            net_segments[idx].push(seg);
        } else {
            net_ids.push(net);
            net_segments.push(vec![seg]);
        }
    }

    for (i, &net_id) in net_ids.iter().enumerate() {
        let er_eff = wheeler_effective_permittivity(
            params.substrate_er,
            nm_to_m(
                segments
                    .iter()
                    .find(|s| s.net_id as u32 == net_id)
                    .map_or(200_000, |s| s.width_nm),
            ),
            params.substrate_height_m,
        );

        let mut extracted_segs = Vec::with_capacity(net_segments[i].len());
        for seg in &net_segments[i] {
            let width_m = nm_to_m(seg.width_nm);
            let dx = seg.end.x - seg.start.x;
            let dy = seg.end.y - seg.start.y;
            let length_nm = ((dx * dx + dy * dy) as f64).sqrt();
            let length_m = length_nm * 1.0e-9;

            let r = series_resistance(
                COPPER_RESISTIVITY,
                length_m,
                width_m,
                params.trace_thickness_m,
            );
            let c =
                sakurai_ground_capacitance(width_m, params.substrate_height_m, er_eff) * length_m;
            let l = MU_0 * length_m / (2.0 * std::f64::consts::PI)
                * (2.0 * length_m / (width_m + params.substrate_height_m))
                    .ln()
                    .max(0.0);

            extracted_segs.push(ExtractedSegment {
                segment_id: seg.segment_id,
                resistance_ohm: r,
                capacitance_f: c,
                inductance_h: l,
                length_m,
            });
        }

        let net_junctions: Vec<JunctionParasitics> = junctions
            .iter()
            .filter(|j| j.net_id.raw() == net_id)
            .map(junction_parasitics)
            .collect();

        traces.push(ExtractedTrace {
            net_id,
            segments: extracted_segs,
            junction_parasitics: net_junctions,
        });
    }

    let spice_netlist = export_spice_netlist(&traces, params);

    let elapsed = start.elapsed();
    let extraction_time_ms = elapsed.as_millis() as u64;

    ExtractionResult {
        traces,
        spice_netlist,
        extraction_time_ms,
    }
}

/// Export a SPICE-compatible netlist from extracted parasitics.
///
/// Generates subcircuit elements for each segment (R, C, L) and
/// K_coupling cards for mutual inductance between coupled traces.
pub fn export_spice_netlist(traces: &[ExtractedTrace], params: &ExtractionParams) -> String {
    let mut netlist = String::with_capacity(1024);

    let _ = writeln!(netlist, "* HWC Parasitic Extraction Netlist");
    let _ = writeln!(netlist, "* Frequency: {} Hz", params.freq_hz);
    let _ = writeln!(netlist, ".SUBCKT BOARD_PARASITICS");

    for trace in traces {
        for seg in &trace.segments {
            let r_node_p = format!("N{}_{}_P", trace.net_id, seg.segment_id);
            let r_node_n = format!("N{}_{}_N", trace.net_id, seg.segment_id);

            if seg.resistance_ohm > 0.0 {
                let _ = writeln!(
                    netlist,
                    "R{}_{} {} {} {:.6e}",
                    trace.net_id, seg.segment_id, r_node_p, r_node_n, seg.resistance_ohm
                );
            }
            if seg.capacitance_f > 0.0 {
                let _ = writeln!(
                    netlist,
                    "C{}_{} {} 0 {:.6e}",
                    trace.net_id, seg.segment_id, r_node_n, seg.capacitance_f
                );
            }
            if seg.inductance_h > 0.0 {
                let _ = writeln!(
                    netlist,
                    "L{}_{} {} {} {:.6e}",
                    trace.net_id, seg.segment_id, r_node_p, r_node_n, seg.inductance_h
                );
            }
        }

        for (ji, jp) in trace.junction_parasitics.iter().enumerate() {
            let junc_node = format!("JUNC{}_{}", trace.net_id, ji);
            if jp.c_junc > 0.0 {
                let _ = writeln!(
                    netlist,
                    "CJ{}_{} {} 0 {:.6e}",
                    trace.net_id, ji, junc_node, jp.c_junc
                );
            }
            if jp.l_junc > 0.0 {
                let _ = writeln!(
                    netlist,
                    "LJ{}_{} {} {} {:.6e}",
                    trace.net_id, ji, junc_node, junc_node, jp.l_junc
                );
            }
        }
    }

    for i in 0..traces.len() {
        for j in (i + 1)..traces.len() {
            let coupled_length = traces[i]
                .segments
                .iter()
                .filter_map(|si| {
                    traces[j]
                        .segments
                        .iter()
                        .find(|sj| si.segment_id == sj.segment_id)
                        .map(|_| si.length_m)
                })
                .sum::<f64>();

            if coupled_length > 0.0 {
                let _ = writeln!(
                    netlist,
                    "K_{}_{} L{}_0 L{}_0 {:.6e}",
                    traces[i].net_id,
                    traces[j].net_id,
                    traces[i].net_id,
                    traces[j].net_id,
                    coupled_length
                );
            }
        }
    }

    let _ = writeln!(netlist, ".ENDS BOARD_PARASITICS");
    netlist
}
