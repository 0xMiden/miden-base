#![cfg(feature = "build")]

#[test]
fn build_helper_and_derives_support_a_renamed_runtime_dependency() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // Do not reuse the parent's target directory: Cargo holds its build lock while testing.
    let target = manifest_dir.join("../../target/protobuf-renamed-consumer");
    let output = std::process::Command::new(env!("CARGO"))
        .args(["test", "--offline", "--quiet", "--manifest-path"])
        .arg(manifest_dir.join("tests/fixtures/renamed/Cargo.toml"))
        .arg("--target-dir")
        .arg(target)
        .env_remove("CARGO_TARGET_TMPDIR")
        .output()
        .expect("run renamed consumer fixture");
    assert!(
        output.status.success(),
        "renamed consumer failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
