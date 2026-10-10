//! Token estimation for context budgets.

/// Estimates how many model tokens a text costs. Budgets are enforced against
/// this estimate, so an estimator must be deterministic for a given text.
pub trait TokenEstimator: Send + Sync {
    fn estimate(&self, text: &str) -> usize;
}

/// The default estimator: `ceil(chars / 4)`, a conservative average for code and
/// English with byte-pair tokenizers. Swap in a model-specific tokenizer for
/// tighter packing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CharEstimator;

impl TokenEstimator for CharEstimator {
    fn estimate(&self, text: &str) -> usize {
        text.chars().count().div_ceil(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_estimator_rounds_up_by_characters() {
        let estimator = CharEstimator;
        assert_eq!(estimator.estimate(""), 0);
        assert_eq!(estimator.estimate("a"), 1);
        assert_eq!(estimator.estimate("abcd"), 1);
        assert_eq!(estimator.estimate("abcde"), 2);
        // Characters, not bytes.
        assert_eq!(estimator.estimate("ééé"), 1);
    }
}
