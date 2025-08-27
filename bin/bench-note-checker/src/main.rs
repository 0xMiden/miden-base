use anyhow::Result;

fn main() -> Result<()> {
    println!("Use either of the following commands to run the benchmarks:");
    println!("make bench-note-checker");
    println!("cargo bench --bin bench-note-checker --bench benches");
    Ok(())
}
