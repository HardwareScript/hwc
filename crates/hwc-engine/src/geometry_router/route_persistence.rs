//! Vector Route Persistence — Base-36 RLC encoding (Roadmap 6.3)
//!
//! Compresses Manhattan-routed traces into dense direction-magnitude strings.
//! Coordinates are i64 nanometers. No f64 in core path.

/// Direction of movement along Manhattan axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Right,
    Left,
    Up,
    Down,
}

impl Direction {
    /// Encode direction as an uppercase character.
    #[inline]
    pub fn to_char(self) -> char {
        match self {
            Direction::Right => 'R',
            Direction::Left => 'L',
            Direction::Up => 'U',
            Direction::Down => 'D',
        }
    }

    /// Decode a direction character.
    #[inline]
    pub fn from_char(ch: char) -> Option<Self> {
        match ch {
            'R' => Some(Direction::Right),
            'L' => Some(Direction::Left),
            'U' => Some(Direction::Up),
            'D' => Some(Direction::Down),
            _ => None,
        }
    }
}

/// Errors produced during RLC decoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RlcError {
    InvalidDirection(char),
    InvalidMagnitude(char),
    EmptyInput,
}

impl std::fmt::Display for RlcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RlcError::InvalidDirection(ch) => write!(f, "invalid direction character: '{ch}'"),
            RlcError::InvalidMagnitude(ch) => write!(f, "invalid magnitude character: '{ch}'"),
            RlcError::EmptyInput => write!(f, "empty RLC input"),
        }
    }
}

impl std::error::Error for RlcError {}

// ---------------------------------------------------------------------------
// Base-36 helpers
// ---------------------------------------------------------------------------

/// Encode a non-negative integer as a lowercase base-36 string.
fn encode_base36(mut value: i64) -> String {
    if value == 0 {
        return "0".into();
    }
    let mut buf = [0u8; 12];
    let mut pos = buf.len();
    while value > 0 {
        pos -= 1;
        let digit = (value % 36) as u8;
        buf[pos] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + (digit - 10)
        };
        value /= 36;
    }
    // SAFETY: buf[pos..] contains ASCII base-36 digits
    String::from_utf8_lossy(&buf[pos..]).into_owned()
}

/// Decode a lowercase base-36 character to its numeric value.
#[inline]
fn decode_base36_char(ch: char) -> Option<i64> {
    match ch {
        '0'..='9' => Some(ch as i64 - '0' as i64),
        'a'..='z' => Some(ch as i64 - 'a' as i64 + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// RLC encoding
// ---------------------------------------------------------------------------

/// Encode a trace (sequence of absolute 2D Manhattan-routed coordinates) into
/// a direction-magnitude RLC string.
///
/// Each consecutive pair of coordinates must differ on exactly one axis
/// (Manhattan routing constraint). The delta is encoded as:
/// - Right: `R<base36_mag>` (positive X)
/// - Left:  `L<base36_mag>` (negative X)
/// - Up:    `U<base36_mag>` (positive Y)
/// - Down:  `D<base36_mag>` (negative Y)
///
/// Consecutive collinear segments are coalesced automatically.
///
/// Example: `"RkU46Dk"` = Right 20, Up 150, Down 20
#[inline]
pub fn encode_rlc(trace: &[(i64, i64)]) -> String {
    if trace.len() < 2 {
        return String::new();
    }

    let mut out = String::with_capacity(trace.len() * 3);

    for window in trace.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];
        let dx = x1 - x0;
        let dy = y1 - y0;

        if dx == 0 && dy == 0 {
            continue;
        }

        let (dir, mag) = if dx != 0 {
            if dx > 0 {
                (Direction::Right, dx)
            } else {
                (Direction::Left, -dx)
            }
        } else if dy > 0 {
            (Direction::Up, dy)
        } else {
            (Direction::Down, -dy)
        };

        out.push(dir.to_char());
        out.push_str(&encode_base36(mag));
    }

    out
}

// ---------------------------------------------------------------------------
// RLC decoding
// ---------------------------------------------------------------------------

/// Decode a direction-magnitude RLC string into absolute 2D coordinates.
///
/// The first coordinate is `(0, 0)`. Each direction-magnitude pair moves
/// relative to the previous position.
pub fn decode_rlc(encoded: &str) -> Result<Vec<(i64, i64)>, RlcError> {
    if encoded.is_empty() {
        return Err(RlcError::EmptyInput);
    }

    let mut points = vec![(0i64, 0i64)];
    let mut x: i64 = 0;
    let mut y: i64 = 0;
    let mut magnitude: i64 = 0;
    let mut has_magnitude = false;
    let mut prev_dir: Option<Direction> = None;

    for ch in encoded.chars() {
        if let Some(dir) = Direction::from_char(ch) {
            // Flush previous direction segment
            if let Some(d) = prev_dir {
                if has_magnitude {
                    apply_direction_2d(&mut x, &mut y, d, magnitude);
                    points.push((x, y));
                }
                magnitude = 0;
                has_magnitude = false;
            }
            prev_dir = Some(dir);
        } else if prev_dir.is_some() {
            // Expecting a magnitude digit
            let digit = decode_base36_char(ch).ok_or(RlcError::InvalidMagnitude(ch))?;
            magnitude = magnitude * 36 + digit;
            has_magnitude = true;
        } else {
            // Expecting a direction but got something else
            return Err(RlcError::InvalidDirection(ch));
        }
    }

    // Flush final segment
    if let Some(d) = prev_dir {
        if has_magnitude {
            apply_direction_2d(&mut x, &mut y, d, magnitude);
            points.push((x, y));
        }
    }

    Ok(points)
}

#[inline]
fn apply_direction_2d(x: &mut i64, y: &mut i64, dir: Direction, mag: i64) {
    match dir {
        Direction::Right => *x += mag,
        Direction::Left => *x -= mag,
        Direction::Up => *y += mag,
        Direction::Down => *y -= mag,
    }
}

// ---------------------------------------------------------------------------
// Compression metrics
// ---------------------------------------------------------------------------

/// Compute the compression ratio: `original_lines / encoded_len`.
/// Higher values mean better compression. A ratio > 1.0 means the encoded
/// form is shorter than the original.
pub fn compression_ratio(original_lines: usize, encoded_len: usize) -> f64 {
    if encoded_len == 0 {
        return f64::INFINITY;
    }
    original_lines as f64 / encoded_len as f64
}

// ---------------------------------------------------------------------------
// Batch persistence
// ---------------------------------------------------------------------------

/// Encode a batch of `(net_id, trace)` pairs into `(net_id, rlc_string)`.
pub fn encode_all_traces(traces: &[(u32, Vec<(i64, i64)>)]) -> Vec<(u32, String)> {
    traces
        .iter()
        .map(|(net_id, trace)| (*net_id, encode_rlc(trace)))
        .collect()
}

/// Decode a batch of `(net_id, rlc_string)` pairs back to traces.
pub fn decode_all_traces(
    encoded: &[(u32, String)],
) -> Result<Vec<(u32, Vec<(i64, i64)>)>, RlcError> {
    let mut out = Vec::with_capacity(encoded.len());
    for (net_id, rlc) in encoded {
        let trace = decode_rlc(rlc)?;
        out.push((*net_id, trace));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_known_string() {
        // Rk = Right 20, U46 = Up 150, Dk = Down 20
        let decoded = decode_rlc("RkU46Dk").expect("decode");
        let expected = vec![
            (0, 0),
            (20, 0),   // Rk: Right 20
            (20, 150), // U46: Up 150  (4*36 + 6 = 150)
            (20, 130), // Dk: Down 20
        ];
        assert_eq!(decoded, expected);
    }

    #[test]
    fn encode_decode_roundtrip() {
        let original = vec![
            (0i64, 0i64),
            (1000, 0),
            (1000, 500),
            (2500, 500),
            (2500, -300),
        ];
        let encoded = encode_rlc(&original);
        let decoded = decode_rlc(&encoded).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn empty_trace_encodes_to_empty() {
        let trace: Vec<(i64, i64)> = vec![];
        assert_eq!(encode_rlc(&trace), "");

        let single = vec![(100, 200)];
        assert_eq!(encode_rlc(&single), "");
    }

    #[test]
    fn single_segment() {
        let trace = vec![(0, 0), (5000, 0)];
        let encoded = encode_rlc(&trace);
        // 5000 in base-36: 5000/36=138 r32→w, 138/36=3 r30→u, 3→3 => "3uw"
        assert_eq!(encoded, "R3uw");

        let decoded = decode_rlc(&encoded).expect("decode");
        assert_eq!(decoded, trace);
    }

    #[test]
    fn negative_directions() {
        let trace = vec![(0, 0), (-500, 0), (-500, -300)];
        let encoded = encode_rlc(&trace);
        let decoded = decode_rlc(&encoded).expect("decode");
        assert_eq!(decoded, trace);
    }

    #[test]
    fn compression_ratio_favorable() {
        // Build a proper Manhattan-routed trace with 49 segments
        // Trace alternates X moves and Y moves (Manhattan)
        let mut trace = vec![(0i64, 0i64)];
        let mut x = 0i64;
        let mut y = 0i64;
        for i in 1..50i64 {
            if i % 2 == 1 {
                x += 1000;
            } else {
                y += 500;
            }
            trace.push((x, y));
        }

        let encoded = encode_rlc(&trace);
        let decoded = decode_rlc(&encoded).expect("decode");
        assert_eq!(decoded, trace);

        // Original: 49 coordinate pairs ≈ 50 * 8 = 400 chars (approximate)
        // Encoded: ~100 chars
        // Ratio should be > 1.0
        let original_chars = trace.len() * 8;
        let ratio = compression_ratio(original_chars, encoded.len());
        assert!(
            ratio > 1.0,
            "compression ratio should be > 1.0, got {ratio}"
        );
    }

    #[test]
    fn large_magnitudes() {
        let trace = vec![(0, 0), (1_000_000, 0)];
        let encoded = encode_rlc(&trace);
        assert!(encoded.starts_with('R'));
        let decoded = decode_rlc(&encoded).expect("decode");
        assert_eq!(decoded, trace);
    }

    #[test]
    fn error_cases() {
        assert_eq!(decode_rlc(""), Err(RlcError::EmptyInput));
        // 'X' is not a direction character → InvalidDirection
        assert_eq!(decode_rlc("X10"), Err(RlcError::InvalidDirection('X')));
        // 'R' is valid direction, then '{' is not a valid base-36 digit
        assert_eq!(decode_rlc("R{"), Err(RlcError::InvalidMagnitude('{')));
    }

    #[test]
    fn batch_roundtrip() {
        let traces = vec![
            (1u32, vec![(0, 0), (100, 0), (100, 200)]),
            (2u32, vec![(0, 0), (0, 100), (150, 100)]),
        ];
        let encoded = encode_all_traces(&traces);
        let decoded = decode_all_traces(&encoded).expect("batch decode");
        assert_eq!(decoded, traces);
    }

    #[test]
    fn direction_chars() {
        assert_eq!(Direction::Right.to_char(), 'R');
        assert_eq!(Direction::Left.to_char(), 'L');
        assert_eq!(Direction::Up.to_char(), 'U');
        assert_eq!(Direction::Down.to_char(), 'D');

        assert_eq!(Direction::from_char('R'), Some(Direction::Right));
        assert_eq!(Direction::from_char('L'), Some(Direction::Left));
        assert_eq!(Direction::from_char('U'), Some(Direction::Up));
        assert_eq!(Direction::from_char('D'), Some(Direction::Down));
        assert_eq!(Direction::from_char('X'), None);
    }
}
