//! CSI parameter parsing.
//!
//! Parameters in CSI sequences are semicolon-separated numbers, optionally with
//! colon-separated sub-parameters. This module provides zero-allocation parsing
//! using a fixed-size inline array.

/// Maximum number of parameters in a CSI sequence.
pub const MAX_PARAMS: usize = 16;

/// CSI parameters, stored inline with no heap allocation.
#[derive(Debug, Clone)]
pub struct Params {
    /// Parameter values (-1 means default/omitted).
    values: [i32; MAX_PARAMS],
    /// Number of parameters.
    count: usize,
}

impl Params {
    /// Create empty params.
    pub const fn new() -> Self {
        Self { values: [-1; MAX_PARAMS], count: 0 }
    }

    /// Reset to empty.
    pub fn clear(&mut self) {
        self.values = [-1; MAX_PARAMS];
        self.count = 0;
    }

    /// Parse parameters from a byte slice (e.g., "1;2;3" or "38;2;255;0;0").
    pub fn parse(&mut self, data: &[u8]) {
        self.clear();
        if data.is_empty() {
            return;
        }

        let mut val: i32 = -1;
        for &byte in data {
            match byte {
                b'0'..=b'9' => {
                    if val < 0 {
                        val = 0;
                    }
                    val = val.saturating_mul(10).saturating_add(i32::from(byte - b'0'));
                }
                b';' | b':' => {
                    if self.count < MAX_PARAMS {
                        self.values[self.count] = val;
                        self.count += 1;
                    }
                    val = -1;
                }
                _ => break,
            }
        }
        // Final parameter
        if self.count < MAX_PARAMS {
            self.values[self.count] = val;
            self.count += 1;
        }
    }

    /// Number of parameters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether there are no parameters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get parameter at index, returning the default value if omitted or out of bounds.
    #[must_use]
    pub fn get(&self, index: usize, default: i32) -> i32 {
        if index < self.count && self.values[index] >= 0 { self.values[index] } else { default }
    }

    /// Get parameter at index as u32, with a default.
    #[must_use]
    pub fn get_u32(&self, index: usize, default: u32) -> u32 {
        let v = self.get(index, default as i32);
        if v < 0 { default } else { v as u32 }
    }

    /// Raw access to parameter values.
    #[must_use]
    pub fn values(&self) -> &[i32] {
        &self.values[..self.count]
    }
}

impl Default for Params {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_params() {
        let p = Params::new();
        assert!(p.is_empty());
        assert_eq!(p.get(0, 1), 1); // default
    }

    #[test]
    fn single_param() {
        let mut p = Params::new();
        p.parse(b"42");
        assert_eq!(p.len(), 1);
        assert_eq!(p.get(0, 0), 42);
    }

    #[test]
    fn multiple_params() {
        let mut p = Params::new();
        p.parse(b"1;2;3");
        assert_eq!(p.len(), 3);
        assert_eq!(p.get(0, 0), 1);
        assert_eq!(p.get(1, 0), 2);
        assert_eq!(p.get(2, 0), 3);
    }

    #[test]
    fn omitted_params() {
        let mut p = Params::new();
        p.parse(b";2;");
        assert_eq!(p.len(), 3);
        assert_eq!(p.get(0, 1), 1); // default
        assert_eq!(p.get(1, 0), 2);
        assert_eq!(p.get(2, 1), 1); // default
    }

    #[test]
    fn sgr_rgb() {
        let mut p = Params::new();
        p.parse(b"38;2;255;128;0");
        assert_eq!(p.len(), 5);
        assert_eq!(p.get(0, 0), 38);
        assert_eq!(p.get(1, 0), 2);
        assert_eq!(p.get(2, 0), 255);
        assert_eq!(p.get(3, 0), 128);
        assert_eq!(p.get(4, 0), 0);
    }

    #[test]
    fn overflow_clamped() {
        let mut p = Params::new();
        // More than MAX_PARAMS parameters
        p.parse(b"1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17;18");
        assert_eq!(p.len(), MAX_PARAMS);
    }

    #[test]
    fn colon_separator() {
        let mut p = Params::new();
        p.parse(b"4:3"); // Curly underline
        assert_eq!(p.len(), 2);
        assert_eq!(p.get(0, 0), 4);
        assert_eq!(p.get(1, 0), 3);
    }
}
