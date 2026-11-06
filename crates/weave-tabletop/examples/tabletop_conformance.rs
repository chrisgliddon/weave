//! Run the registry-driven tabletop adapter conformance suite and print one line per adapter.
//!
//! ```bash
//! cargo run -p weave-tabletop --example tabletop_conformance
//! ```

use std::process::ExitCode;

use weave_tabletop::conformance::standard_suite;

fn main() -> ExitCode {
    let report = standard_suite().run();
    println!("{}", report.render());
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
