//! Directioner OS Coverage and CI Quality Gate Tool (dos-coverage)
//!
//! Introspects live image surface and measures catalog coverage (U087),
//! providing CI coverage gates to prevent regression (U088).

pub mod coverage;

pub use coverage::{AreaCoverage, CoverageEvaluator, CoverageReport};
