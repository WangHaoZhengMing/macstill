//! Inspect one real window capture without connecting to the desktop shell.
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let address = args.next().expect("window ADDRESS OUTPUT.png");
    let output = args.next().expect("window ADDRESS OUTPUT.png");
    let mut capture = macstill_capture::Capture::connect()?;
    capture
        .capture(&address, std::time::Duration::from_secs(2))?
        .save(output)?;
    Ok(())
}
