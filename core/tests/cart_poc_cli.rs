#![cfg(feature = "host-poc")]

use std::process::Command;

#[test]
fn synthetic_scenarios_have_distinct_results() {
    for (scenario, code, message) in [
        ("normal", 0, "26-byte header OK"),
        ("checksum", 5, "Checksum"),
        ("type", 5, "UnexpectedType"),
        ("rom-size", 5, "UnexpectedRomSize"),
        ("ram-size", 5, "UnexpectedRamSize"),
        ("read-failure", 4, "stopped at 0x0140"),
        ("gate-refusal", 3, "GateClosed"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_cart_header_poc"))
            .args(["--synthetic", scenario])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(code), "{scenario}");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(combined.contains(message), "{combined}");
    }
}

#[test]
fn hardware_unknown_missing_and_extra_options_are_refused() {
    for args in [
        vec![],
        vec!["--hardware"],
        vec!["--synthetic", "unknown"],
        vec!["--synthetic", "normal", "--hardware"],
        vec!["--synthetic"],
        vec!["/dev/ttyACM0"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_cart_header_poc"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("hardware is unsupported"));
    }
}
