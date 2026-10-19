fn main() {
    let result = (|| {
        let executable = std::env::current_exe()?;
        let runtime = ddi_danoni_importer::runtime_path(&executable)?;
        match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
            [] => ddi_danoni_importer::serve(std::io::stdin(), std::io::stdout(), runtime),
            [arg] if arg == "--stdio" => {
                ddi_danoni_importer::serve(std::io::stdin(), std::io::stdout(), runtime)
            }
            [arg] if arg == "--check-bundle" => ddi_danoni_importer::check_runtime(&runtime)
                .map_err(|e| std::io::Error::other(format!("{}: {}", e.code, e.message))),
            _ => Err(std::io::Error::other(
                "Usage: ddi-danoni-importer [--stdio|--check-bundle]",
            )),
        }
    })();
    if let Err(error) = result {
        eprintln!("DanOni importer: {error}");
        std::process::exit(1);
    }
}
