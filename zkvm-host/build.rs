use std::process::Command;

use sp1_helper::{build_program_with_args, BuildArgs};

fn main() {
    // Build the crates with the release-client-lto profile for native execution.
    let binaries = ["validity-client"];
    // let binaries = ["zkvm-client", "validity-client"];
    for binary in binaries.iter() {
        let status = Command::new("cargo")
            .args(&[
                "build",
                "--workspace",
                "--bin",
                binary,
                "--profile",
                "release-client-lto",
            ])
            .status()
            .expect("Failed to execute cargo build command");

        if !status.success() {
            panic!("Failed to build {} with release-client-lto profile", binary);
        }

        println!(
            "cargo:warning={} built with release-client-lto profile",
            binary
        );
    }

    // build_program_with_args(
    //     "../zkvm-client",
    //     BuildArgs {
    //         ignore_rust_version: true,
    //         elf_name: "riscv32im-succinct-zkvm-elf".to_string(),
    //         ..Default::default()
    //     },
    // );

    build_program_with_args(
        "../validity-client",
        BuildArgs {
            ignore_rust_version: true,
            elf_name: "riscv32im-succinct-multiblock-elf".to_string(),
            ..Default::default()
        },
    );
}
