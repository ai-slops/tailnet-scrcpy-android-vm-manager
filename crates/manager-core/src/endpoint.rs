use std::collections::BTreeSet;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortRange {
    start: u16,
    end: u16,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PortRangeError {
    #[error("endpoint port range must be ordered and unprivileged")]
    Invalid,
    #[error("no endpoint port is available")]
    Exhausted,
}

impl PortRange {
    pub fn new(start: u16, end: u16) -> Result<Self, PortRangeError> {
        if start < 1024 || start > end {
            return Err(PortRangeError::Invalid);
        }
        Ok(Self { start, end })
    }

    pub fn allocate(&self, occupied: &BTreeSet<u16>) -> Result<u16, PortRangeError> {
        (self.start..=self.end)
            .find(|port| !occupied.contains(port))
            .ok_or(PortRangeError::Exhausted)
    }

    #[must_use]
    pub fn contains(&self, port: u16) -> bool {
        (self.start..=self.end).contains(&port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_first_free_port_deterministically() {
        let range = PortRange::new(31_000, 31_002).unwrap();
        let occupied = BTreeSet::from([31_000, 31_002]);
        assert_eq!(range.allocate(&occupied).unwrap(), 31_001);
    }

    #[test]
    fn reports_exhaustion() {
        let range = PortRange::new(31_000, 31_001).unwrap();
        let occupied = BTreeSet::from([31_000, 31_001]);
        assert_eq!(range.allocate(&occupied), Err(PortRangeError::Exhausted));
    }

    #[test]
    fn rejects_privileged_or_reversed_range() {
        assert_eq!(PortRange::new(80, 90), Err(PortRangeError::Invalid));
        assert_eq!(PortRange::new(32_000, 31_000), Err(PortRangeError::Invalid));
    }
}
