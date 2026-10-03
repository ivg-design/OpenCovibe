#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--room-mcp") {
        let value = |flag: &str| -> Result<String, String> {
            args.windows(2)
                .find(|pair| pair[0] == flag)
                .map(|pair| pair[1].clone())
                .ok_or_else(|| format!("missing {flag}"))
        };
        let result = (|| -> Result<(), String> {
            let data_dir = std::path::PathBuf::from(value("--data-dir")?);
            if !data_dir.is_absolute() {
                return Err("data directory must be absolute".into());
            }
            std::env::set_var("OPENCOVIBE_DATA_DIR", &data_dir);
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?
                .block_on(opencovibe_desktop_lib::rooms::mcp::serve(
                    data_dir,
                    value("--room-id")?,
                    value("--participant-id")?,
                ))
        })();
        if let Err(error) = result {
            eprintln!("room MCP: {error}");
            std::process::exit(1);
        }
        return;
    }
    opencovibe_desktop_lib::run();
}
