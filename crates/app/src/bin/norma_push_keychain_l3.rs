fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        args.len() == 2,
        "Expected probe step and isolated AppDB path"
    );
    let step = args[0]
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid probe step"))?;
    app::run_push_keychain_probe(std::path::Path::new(&args[1]), step)?;
    println!("native-keychain-step-ok {step}");
    Ok(())
}
