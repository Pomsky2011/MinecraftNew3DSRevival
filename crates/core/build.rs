//! Fails the build unless exactly one release feature (`mc1_*`) is on (DESIGN.md §5.6).

use std::env;

fn main() {
    let releases: Vec<String> = env::vars()
        .filter_map(|(key, _)| {
            key.strip_prefix("CARGO_FEATURE_MC")
                .map(|rest| format!("mc{}", rest.to_lowercase()))
        })
        .collect();
    match releases.as_slice() {
        [_] => {}
        [] => panic!("no release feature on: build with exactly one, e.g. `--features mc1_21_11`"),
        many => panic!(
            "more than one release feature on ({}): build with exactly one",
            many.join(", ")
        ),
    }
}
