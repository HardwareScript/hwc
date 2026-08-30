//! OASIS (Open Artwork System Interchange Standard) Binary Stream Exporter
//!
//! Compact, high-density stream format supporting fast hierarchical geometric
//! streaming for advanced tapeouts.

use std::io::{self, Write};

pub mod records {
    pub const START: u8 = 1;
    pub const CELLNAME: u8 = 3;
    pub const TEXTSTRING: u8 = 5;
    pub const LAYERNAME: u8 = 11;
    pub const CELL: u8 = 13;
    pub const RECTANGLE: u8 = 19;
    pub const POLYGON: u8 = 20;
    pub const PATH: u8 = 21;
    pub const END: u8 = 2;
}

/// OASIS binary stream writer.
pub struct OasisWriter<W: Write> {
    writer: W,
    unit_m: f64,
}

impl<W: Write> OasisWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            unit_m: 1e-9, // 1 nm default
        }
    }

    pub fn with_unit(mut self, unit_m: f64) -> Self {
        self.unit_m = unit_m;
        self
    }

    /// Writes OASIS magic header `%SEMI-OASIS\r\n`.
    pub fn write_magic_header(&mut self) -> io::Result<()> {
        self.writer.write_all(b"%SEMI-OASIS\r\n")
    }

    /// Writes START record with version string.
    pub fn write_start(&mut self, version: &str) -> io::Result<()> {
        self.writer.write_all(&[records::START])?;
        self.write_a_string(version)?;
        self.write_real64(self.unit_m)
    }

    /// Writes a CELL definition header.
    pub fn write_cell_start(&mut self, cell_name: &str) -> io::Result<()> {
        self.writer.write_all(&[records::CELL])?;
        self.write_a_string(cell_name)
    }

    /// Writes a RECTANGLE record.
    pub fn write_rectangle(
        &mut self,
        layer: u32,
        datatype: u32,
        width: u64,
        height: u64,
        x: i64,
        y: i64,
    ) -> io::Result<()> {
        self.writer.write_all(&[records::RECTANGLE])?;
        self.write_unsigned_int(layer as u64)?;
        self.write_unsigned_int(datatype as u64)?;
        self.write_unsigned_int(width)?;
        self.write_unsigned_int(height)?;
        self.write_signed_int(x)?;
        self.write_signed_int(y)
    }

    /// Writes END record.
    pub fn write_end(&mut self) -> io::Result<()> {
        self.writer.write_all(&[records::END])
    }

    // Helper serialization formats for OASIS specification
    fn write_unsigned_int(&mut self, mut val: u64) -> io::Result<()> {
        // Variable-length 7-bit encoded integer
        loop {
            let byte = (val & 0x7F) as u8;
            val >>= 7;
            if val == 0 {
                self.writer.write_all(&[byte])?;
                break;
            } else {
                self.writer.write_all(&[byte | 0x80])?;
            }
        }
        Ok(())
    }

    fn write_signed_int(&mut self, val: i64) -> io::Result<()> {
        let uval = if val >= 0 {
            (val as u64) << 1
        } else {
            (((-val) as u64) << 1) | 1
        };
        self.write_unsigned_int(uval)
    }

    fn write_a_string(&mut self, s: &str) -> io::Result<()> {
        let bytes = s.as_bytes();
        self.write_unsigned_int(bytes.len() as u64)?;
        self.writer.write_all(bytes)
    }

    fn write_real64(&mut self, val: f64) -> io::Result<()> {
        self.writer.write_all(&val.to_le_bytes())
    }
}
