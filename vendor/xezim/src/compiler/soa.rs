//! Structure of Arrays (SOA) optimization for signal table.
//!
//! This module provides an alternative representation of the signal table
//! using Structure of Arrays instead of Array of Structures. This improves
//! cache efficiency when accessing signal values in patterns (e.g., during
//! combinational settle or VCD dumping).
//!
//! # Background
//!
//! The current `signal_table: Vec<Value>` uses Array of Structures (AoS):
//!
//! ```text
//! [Value] [Value] [Value] ...
//!  ├─ val_bits (u64)
//!  ├─ xz_bits (u64)
//!  ├─ width (u32)
//!  ├─ is_signed (bool)
//!  └─ is_real (bool)
//! ```
//!
//! For wide signals (>64 bits), each Value has a heap-allocated `Vec<LogicBit>`,
//! which is even worse for cache locality.
//!
//! # SOA Approach
//!
//! Structure of Arrays separates the data:
//!
//! ```text
//! val_bits:    [u64; N]      - value bits for all signals
//! xz_bits:    [u64; N]      - X/Z bits for all signals  
//! widths:     [u32; N]      - width of each signal
//! is_signed:  [bool; N]     - signed flag for each signal
//! is_real:    [bool; N]     - real flag for each signal
//! wide_data:  [Vec<u8>; N]  - packed wide bits (only for wide signals)
//! ```
//!
//! # Benefits
//!
//! 1. **Better cache locality**: When scanning signals, we access contiguous
//!    memory instead of jumping between struct fields.
//! 
//! 2. **SIMD-friendly**: Regular arrays enable SIMD operations.
//! 
//! 3. **Memory efficiency**: No padding between fields.
//!
//! # Implementation Strategy
//!
//! Since converting the entire codebase to SOA is a massive change, we use a
//! **hybrid approach**:
//!
//! 1. Keep `signal_table: Vec<Value>` as the source of truth
//! 2. Build SOA views on demand for hot paths
//! 3. Gradually migrate hot paths to use SOA
//!
//! The existing `signal_inline_bits: Vec<[u64; 2]>` is already SOA-like!
//! We extend this concept to other signal attributes.

use xezim_core::value::Value;

/// SOA representation of signal values for inline (<=64 bit) signals.
///
/// This is similar to the existing `signal_inline_bits` but provides a more
/// ergonomic API and can be extended to support operations.
#[derive(Debug, Clone)]
pub struct InlineSignalArray {
    /// Value bits for each signal (LSB = bit 0)
    pub val_bits: Vec<u64>,
    /// X/Z bits for each signal (LSB = bit 0)
    pub xz_bits: Vec<u64>,
    /// Width of each signal in bits
    pub widths: Vec<u32>,
    /// Number of signals
    pub len: usize,
}

impl InlineSignalArray {
    /// Create a new empty InlineSignalArray.
    pub fn new() -> Self {
        Self {
            val_bits: Vec::new(),
            xz_bits: Vec::new(),
            widths: Vec::new(),
            len: 0,
        }
    }

    /// Create from a slice of Values (only inline values).
    pub fn from_values(values: &[Value]) -> Self {
        let mut val_bits = Vec::with_capacity(values.len());
        let mut xz_bits = Vec::with_capacity(values.len());
        let mut widths = Vec::with_capacity(values.len());
        
        for v in values {
            if v.width <= 64 {
                // Inline value
                if let Some((vb, xb)) = v.inline_bits() {
                    val_bits.push(vb);
                    xz_bits.push(xb);
                    widths.push(v.width);
                } else {
                    val_bits.push(0);
                    xz_bits.push(0);
                    widths.push(v.width);
                }
            } else {
                // Wide value - use 0 for val/xz bits (low 64 bits)
                val_bits.push(0);
                xz_bits.push(0);
                widths.push(v.width);
            }
        }
        
        Self {
            val_bits,
            xz_bits,
            widths,
            len: values.len(),
        }
    }

    /// Get the number of signals.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get value and XZ bits for signal at index.
    #[inline]
    pub fn get_bits(&self, idx: usize) -> (u64, u64) {
        (self.val_bits[idx], self.xz_bits[idx])
    }

    /// Set value and XZ bits for signal at index.
    #[inline]
    pub fn set_bits(&mut self, idx: usize, val_bits: u64, xz_bits: u64) {
        self.val_bits[idx] = val_bits;
        self.xz_bits[idx] = xz_bits;
    }

    /// Get width for signal at index.
    #[inline]
    pub fn get_width(&self, idx: usize) -> u32 {
        self.widths[idx]
    }

    /// Mask value bits to the signal's width.
    #[inline]
    pub fn mask_val(&self, idx: usize, val: u64) -> u64 {
        let width = self.widths[idx];
        let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
        val & mask
    }

    /// Check if signal has X or Z bits set.
    #[inline]
    pub fn has_xz(&self, idx: usize) -> bool {
        self.xz_bits[idx] != 0
    }

    /// Extract a bit from a signal.
    #[inline]
    pub fn get_bit(&self, idx: usize, bit_pos: u8) -> u8 {
        if bit_pos >= 64 {
            return 0;  // Beyond inline range
        }
        let val = self.val_bits[idx];
        let xz = self.xz_bits[idx];
        let v = ((val >> bit_pos) & 1) as u8;
        let z = ((xz >> bit_pos) & 1) as u8;
        (z << 1) | v
    }
}

/// SOA representation for packed wide signals.
///
/// Uses the packed storage from packed_value.rs (2 bits per bit).
#[derive(Debug, Clone)]
pub struct PackedWideSignalArray {
    /// Packed bits for each wide signal (2 bits per bit)
    /// Each byte stores 4 bits (0.5 bytes per bit = 4x improvement over Vec<LogicBit>)
    pub data: Vec<Vec<u8>>,
    /// Width of each signal in bits
    pub widths: Vec<u32>,
    /// Number of signals
    pub len: usize,
}

impl PackedWideSignalArray {
    /// Create a new empty PackedWideSignalArray.
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            widths: Vec::new(),
            len: 0,
        }
    }

    /// Create from a slice of Values (only wide values > 64 bits).
    pub fn from_wide_values(values: &[Value]) -> Self {
        // For now, just store the raw bytes representation
        // TODO: Use packed_value when it's fully integrated
        let mut data = Vec::with_capacity(values.len());
        let mut widths = Vec::with_capacity(values.len());
        
        for v in values {
            if v.width > 64 {
                // For now, store as empty - packed representation will be added later
                data.push(Vec::new());
                widths.push(v.width);
            }
        }
        
        let len = data.len();
        Self {
            data,
            widths,
            len,
        }
    }

    /// Get the number of signals.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Conversion trait for converting between LogicBit and packed representation.
trait PackedBitConvert {
    fn to_packed(&self) -> u8;
    fn from_packed(packed: u8) -> Self;
}

impl PackedBitConvert for xezim_core::value::LogicBit {
    fn to_packed(&self) -> u8 {
        match self {
            Self::Zero => 0b00,
            Self::One => 0b01,
            Self::X => 0b10,
            Self::Z => 0b11,
        }
    }

    fn from_packed(packed: u8) -> Self {
        match packed & 0b11 {
            0b00 => Self::Zero,
            0b01 => Self::One,
            0b10 => Self::X,
            _ => Self::Z,
        }
    }
}

/// Helper to convert Vec<LogicBit> to packed bytes.
pub fn logic_bits_to_packed(bits: &[xezim_core::value::LogicBit]) -> Vec<u8> {
    let num_bytes = (bits.len() + 3) / 4;
    let mut packed = vec![0u8; num_bytes];
    
    for (i, &bit) in bits.iter().enumerate() {
        let byte_idx = i / 4;
        let shift = (i % 4) * 2;
        let code = bit.to_packed();
        packed[byte_idx] |= code << shift;
    }
    
    packed
}

/// Helper to convert packed bytes to Vec<LogicBit>.
pub fn packed_to_logic_bits(packed: &[u8], len: usize) -> Vec<xezim_core::value::LogicBit> {
    let mut bits = Vec::with_capacity(len);
    
    for i in 0..len {
        let byte_idx = i / 4;
        let shift = (i % 4) * 2;
        let code = (packed[byte_idx] >> shift) & 0b11;
        bits.push(xezim_core::value::LogicBit::from_packed(code));
    }
    
    bits
}

/// Combined SOA signal table that unifies inline and wide signals.
#[derive(Debug)]
pub struct SoaSignalTable {
    /// Inline signals (<= 64 bits)
    pub inline: InlineSignalArray,
    /// Wide signals (> 64 bits)
    pub wide: PackedWideSignalArray,
    /// Indices of wide signals in the original signal_table
    pub wide_indices: Vec<usize>,
    /// Total number of signals
    pub total_len: usize,
}

impl SoaSignalTable {
    /// Create a new SOA signal table from a slice of Values.
    pub fn from_values(values: &[Value]) -> Self {
        let inline = InlineSignalArray::from_values(values);
        
        // Find wide signals
        let mut wide_indices = Vec::new();
        let mut wide_values = Vec::new();
        
        for (idx, v) in values.iter().enumerate() {
            if v.width > 64 {
                wide_indices.push(idx);
                wide_values.push(v.clone());
            }
        }
        
        let wide = PackedWideSignalArray::from_wide_values(&wide_values);
        
        Self {
            inline,
            wide,
            wide_indices,
            total_len: values.len(),
        }
    }

    /// Get a signal's value as (val_bits, xz_bits) for inline signals.
    /// For wide signals, returns the low 64 bits.
    #[inline]
    pub fn get_inline_bits(&self, idx: usize) -> (u64, u64) {
        if self.wide_indices.contains(&idx) {
            // Wide signal - return low 64 bits
            // This would need to extract from packed storage
            (0, 0)  // TODO: implement
        } else {
            self.inline.get_bits(idx)
        }
    }
}

/// Benchmark: compare cache efficiency of AoS vs SOA.
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_soa_vs_aos_memory() {
        let num_signals = 10000;
        let width = 64;  // All inline
        
        // AoS: Vec<Value>
        let aos: Vec<Value> = (0..num_signals)
            .map(|_| Value::from_u64(0xAAAA_AAAA_AAAA_AAAA, width))
            .collect();
        
        // SOA: InlineSignalArray
        let soa = InlineSignalArray::from_values(&aos);
        
        // Memory usage
        let aos_size = aos.capacity() * std::mem::size_of::<Value>();
        let soa_size = soa.val_bits.capacity() * 8  // u64
            + soa.xz_bits.capacity() * 8
            + soa.widths.capacity() * 4;
        
        println!("Signals: {}, Width: {} bits", num_signals, width);
        println!("AoS memory: {} bytes ({:.2} KB)", aos_size, aos_size as f64 / 1024.0);
        println!("SOA memory: {} bytes ({:.2} KB)", soa_size, soa_size as f64 / 1024.0);
        println!("SOA is {:.1}x more efficient", aos_size as f64 / soa_size as f64);
    }

    #[test]
    fn test_soa_access() {
        let num_signals = 1000;
        let width = 64;
        
        let values: Vec<Value> = (0..num_signals)
            .map(|i| Value::from_u64(i as u64, width))
            .collect();
        
        let soa = InlineSignalArray::from_values(&values);
        
        // Test all access patterns
        for i in 0..num_signals {
            let (v, x) = soa.get_bits(i);
            let expected_v = i as u64;
            assert_eq!(v, expected_v);
            assert_eq!(x, 0);
        }
    }

    #[test]
    fn test_packed_conversion() {
        use xezim_core::value::LogicBit as CoreLogicBit;
        
        let bits = vec![
            CoreLogicBit::Zero,
            CoreLogicBit::One,
            CoreLogicBit::X,
            CoreLogicBit::Z,
            CoreLogicBit::One,
            CoreLogicBit::Zero,
        ];
        
        let packed = logic_bits_to_packed(&bits);
        let unpacked = packed_to_logic_bits(&packed, bits.len());
        
        assert_eq!(bits, unpacked);
    }
}
