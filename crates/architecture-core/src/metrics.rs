use crate::error::{ArchitectureError, Result};

/// A score value between 0.0 and 1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    pub value: f64,
}

impl Score {
    /// Create a new score, validating that it's in [0.0, 1.0].
    ///
    /// # Errors
    ///
    /// Returns `InvalidScore` if the value is outside the valid range.
    #[must_use = "consider using the result"]
    pub fn new(value: f64) -> Result<Self> {
        if !(0.0..=1.0).contains(&value) {
            return Err(ArchitectureError::InvalidScore { value });
        }
        Ok(Self { value })
    }
}

/// A metric expressed as a ratio with an optional score.
#[derive(Debug, Clone, PartialEq)]
pub struct RatioMetric {
    pub numerator: f64,
    pub denominator: f64,
    pub score: Option<Score>,
}

impl RatioMetric {
    /// Create a new ratio metric.
    ///
    /// If denominator is 0.0, score is None. Otherwise, ratio is clamped to [0.0, 1.0].
    ///
    /// # Errors
    ///
    /// Returns errors from Score validation.
    pub fn new(numerator: f64, denominator: f64) -> Result<Self> {
        #[allow(clippy::float_cmp)]
        if denominator == 0.0 {
            return Ok(Self {
                numerator,
                denominator,
                score: None,
            });
        }
        let ratio = (numerator / denominator).clamp(0.0, 1.0);
        let score = Some(Score::new(ratio)?);
        Ok(Self {
            numerator,
            denominator,
            score,
        })
    }
}

/// A metric for object/module instability.
#[derive(Debug, Clone, PartialEq)]
pub struct InstabilityMetric {
    pub outgoing_dependency_weight: f64,
    pub incoming_dependent_weight: f64,
    pub ratio: RatioMetric,
}

/// The basis for computing abstractness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbstractnessBasis {
    IntrinsicObjectKind,
    ObjectConstructionAndOperations,
    CompositionalConstruction,
    ModuleObjects,
}

/// A metric for object/module abstractness.
#[derive(Debug, Clone, PartialEq)]
pub struct AbstractnessMetric {
    pub abstract_weight: f64,
    pub total_weight: f64,
    pub ratio: RatioMetric,
    pub basis: AbstractnessBasis,
}
