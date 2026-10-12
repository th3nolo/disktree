//! The per-entry continuation rule; observations remain external operations.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Cancellation,
    Cloud,
    Delete,
}

impl Step {
    pub const fn next(self, passed: bool) -> Option<Self> {
        match (self, passed) {
            (Self::Cancellation, true) => Some(Self::Cloud),
            (Self::Cloud, true) => Some(Self::Delete),
            _ => None,
        }
    }
}

// A failed observation stops the loop and retains its original error. Do not
// sample later checks eagerly: cancellation must precede any cloud/delete call.
pub fn run<E>(mut observe: impl FnMut(Step) -> Result<(), E>) -> Result<(), E> {
    let mut step = Some(Step::Cancellation);
    let mut result = Ok(());
    while let Some(current) = step {
        result = observe(current);
        step = current.next(result.is_ok());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;
    use std::fs::{self, OpenOptions};
    use std::io::Write as _;

    const CASES: &str = include_str!("../../../formal/lean/policy-cases.txt");

    fn name(step: Option<Step>) -> &'static str {
        match step {
            Some(Step::Cancellation) => "cancel",
            Some(Step::Cloud) => "cloud",
            Some(Step::Delete) => "delete",
            None => "stop",
        }
    }

    #[test]
    fn removal_policy_transitions_match_lean() {
        let mut actual = String::from("phase|passed|next\n");
        for step in [Step::Cancellation, Step::Cloud, Step::Delete] {
            for passed in [false, true] {
                writeln!(
                    actual,
                    "{}|{}|{}",
                    name(Some(step)),
                    u8::from(passed),
                    name(step.next(passed))
                )
                .expect("policy row");
            }
        }
        assert_eq!(actual, CASES, "fixed continuation contract changed");
        let model =
            std::env::var_os("DISKTREE_LEAN_POLICY_RESULTS").map(|path| {
                fs::read_to_string(path).expect("Lean policy transcript")
            });
        if let Some(path) = std::env::var_os("DISKTREE_NATIVE_POLICY_RESULTS") {
            assert!(model.is_some(), "exporting policy results requires Lean");
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .expect("fresh policy evidence destination");
            output
                .write_all(actual.as_bytes())
                .expect("policy evidence");
        }
        if let Some(model) = model {
            assert_eq!(actual, model, "Lean/Rust removal policy differs");
        }
        println!("all 6 continuation transitions checked");
    }

    #[test]
    fn every_observation_schedule_short_circuits_in_order() {
        let steps = [Step::Cancellation, Step::Cloud, Step::Delete];
        for mask in 0..8 {
            let passed = [mask & 1 != 0, mask & 2 != 0, mask & 4 != 0];
            let mut calls = Vec::new();
            let result = run(|step| {
                calls.push(step);
                let index = match step {
                    Step::Cancellation => 0,
                    Step::Cloud => 1,
                    Step::Delete => 2,
                };
                if passed[index] { Ok(()) } else { Err(step) }
            });
            let first_failure = passed.iter().position(|pass| !pass);
            let expected_calls = first_failure.map_or(3, |index| index + 1);
            assert_eq!(calls, steps[..expected_calls], "schedule {mask}");
            assert_eq!(result, first_failure.map_or(Ok(()), |i| Err(steps[i])));
        }
    }
}
