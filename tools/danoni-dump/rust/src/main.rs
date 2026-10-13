//! JSON CLI over the same API that a desktop host can call.
use ddi_danoni_export::{CompileRequest, Error, compile_chart};
use std::io::{self, Read, Write};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() != ["compile-chart"] {
        eprintln!("Usage: ddi-danoni-export compile-chart < request.json > result.json");
        std::process::exit(2);
    }
    let result = run();
    match result {
        Ok(result) => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            if serde_json::to_writer_pretty(&mut out, &result).is_err() || writeln!(out).is_err() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{}", serde_json::to_string(&error).unwrap());
            std::process::exit(1);
        }
    }
}
fn run() -> Result<ddi_danoni_export::PreparedChart, Error> {
    const MAX_JSON: u64 = 16 * 1024 * 1024;
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(MAX_JSON + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error {
            code: "input-read",
            message: e.to_string(),
        })?;
    if bytes.len() as u64 > MAX_JSON {
        return Err(Error {
            code: "input-limit",
            message: "JSON exceeds 16 MiB".into(),
        });
    }
    let request: CompileRequest = serde_json::from_slice(&bytes).map_err(|e| Error {
        code: "invalid-json",
        message: e.to_string(),
    })?;
    if request.schema != "ddi-danoni-compile-request/1" {
        return Err(Error {
            code: "unsupported-schema",
            message: request.schema,
        });
    }
    compile_chart(&request.chart, &request.metadata, &request.options)
}
