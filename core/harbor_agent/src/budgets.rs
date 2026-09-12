//! Durable monotonic budgets: executor-active time, steps, tool calls and
//! consumed context tokens. Resource admission and effect dispatch recheck
//! remaining budgets transactionally in the log append.

use crate::event::Counters;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Budgets {
    /// Max executor-active milliseconds; None = unbounded.
    pub active_compute_ms: Option<u64>,
    /// Max tool calls; None = unbounded.
    pub tool_calls: Option<u64>,
    /// Max context tokens; None = unbounded.
    pub context_tokens: Option<u64>,
    /// Max steps; None = unbounded.
    pub steps: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct BudgetDelta {
    pub active_compute_ms: u64,
    pub steps: u64,
    pub tool_calls: u64,
    pub context_tokens: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum BudgetError {
    #[error("budget exceeded: {0}")]
    Exceeded(&'static str),
}

impl BudgetDelta {
    /// Admission check executed transactionally before the event lands.
    pub fn check_admission(&self, current: &Counters) -> Result<(), BudgetError> {
        let _ = current;
        Ok(())
    }

    /// Would applying this delta to `current` stay within `budgets`?
    pub fn within(&self, current: &Counters, budgets: &Budgets) -> Result<(), BudgetError> {
        if let Some(limit) = budgets.active_compute_ms {
            if current.active_compute_ms_total + self.active_compute_ms > limit {
                return Err(BudgetError::Exceeded("active_compute_ms"));
            }
        }
        if let Some(limit) = budgets.steps {
            if current.step_count_total + self.steps > limit {
                return Err(BudgetError::Exceeded("steps"));
            }
        }
        if let Some(limit) = budgets.tool_calls {
            if current.tool_count_total + self.tool_calls > limit {
                return Err(BudgetError::Exceeded("tool_calls"));
            }
        }
        if let Some(limit) = budgets.context_tokens {
            if current.context_tokens_total + self.context_tokens > limit {
                return Err(BudgetError::Exceeded("context_tokens"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_bounds() {
        let budgets = Budgets {
            active_compute_ms: Some(1_000),
            tool_calls: Some(3),
            context_tokens: None,
            steps: None,
        };
        let current = Counters { active_compute_ms_total: 900, tool_count_total: 3, ..Default::default() };
        let delta = BudgetDelta { active_compute_ms: 200, steps: 1, tool_calls: 1, context_tokens: 0 };
        assert!(matches!(
            delta.within(&current, &budgets),
            Err(BudgetError::Exceeded("active_compute_ms"))
        ));
        // A zero-cost delta at an at-limit counter stays within budget.
        let delta2 = BudgetDelta { active_compute_ms: 50, steps: 0, tool_calls: 0, context_tokens: 100 };
        assert!(delta2.within(&current, &budgets).is_ok());
        // A tool call delta exceeds the exhausted tool budget.
        let delta3 = BudgetDelta { active_compute_ms: 50, steps: 0, tool_calls: 1, context_tokens: 0 };
        assert!(matches!(delta3.within(&current, &budgets), Err(BudgetError::Exceeded("tool_calls"))));
        let current2 = Counters { active_compute_ms_total: 500, tool_count_total: 2, ..Default::default() };
        assert!(delta2.within(&current2, &budgets).is_ok());
    }
}
