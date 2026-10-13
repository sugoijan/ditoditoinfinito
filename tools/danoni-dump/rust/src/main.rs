//! JSON CLI over the same API that a desktop host can call.
use ddi_danoni_export::{CompileRequest, Error, compile_chart};
use std::io::{self, Read, Write};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [command] if command == "compile-chart" => run().and_then(|result| {
            serde_json::to_value(result).map_err(|e| Error {
                code: "output-json",
                message: e.to_string(),
            })
        }),
        #[cfg(feature = "native")]
        [command, job, output] if command == "prepare-work" => prepare(job, output),
        #[cfg(feature = "native")]
        [command, registry, exports, output] if command == "rebuild-accepted" => {
            ddi_danoni_export::native::rebuild_accepted(
                std::path::Path::new(registry),
                std::path::Path::new(exports),
                std::path::Path::new(output),
                &mut |_| Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "artwork" => {
            ddi_danoni_export::native::generate_artwork(
                std::path::Path::new(job),
                std::path::Path::new(output),
            )
        }
        #[cfg(feature = "native")]
        [command, job, output] if command == "package-generation" => {
            ddi_danoni_export::package::package_job(
                std::path::Path::new(job),
                std::path::Path::new(output),
                &mut |_| Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, input] if command == "verify-generation" => {
            ddi_danoni_export::package::verify_generation(std::path::Path::new(input), &mut |_| {
                Ok(())
            })
        }
        #[cfg(feature = "native")]
        [command, stage, dest, history] if command == "promote" => {
            ddi_danoni_export::transaction::promote(
                std::path::Path::new(stage),
                std::path::Path::new(dest),
                std::path::Path::new(history),
                &mut |_| Ok(()),
            )
        }
        #[cfg(feature = "native")]
        [command, journal] if command == "recover" => {
            ddi_danoni_export::transaction::recover(std::path::Path::new(journal), &mut |_| Ok(()))
        }
        _ => {
            eprintln!(
                "Usage: ddi-danoni-export compile-chart < request.json\n       ddi-danoni-export prepare-work JOB.json NEW_OUTPUT\n       ddi-danoni-export rebuild-accepted REGISTRY.json EXPORTS NEW_OUTPUT\n       ddi-danoni-export artwork JOB.json NEW_OUTPUT\n       ddi-danoni-export package-generation JOB.json NEW_OUTPUT\n       ddi-danoni-export verify-generation GENERATION\n       ddi-danoni-export promote STAGE DESTINATION HISTORY\n       ddi-danoni-export recover JOURNAL.json"
            );
            std::process::exit(2);
        }
    };
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

#[cfg(feature = "native")]
fn prepare(job: &str, output: &str) -> Result<serde_json::Value, Error> {
    use ddi_danoni_export::{native, work};
    let (job, source) = native::load_job(std::path::Path::new(job))?;
    let prepared = work::prepare_work(&job.work, &source, &mut |_| Ok(()))?;
    native::write_prepared(&prepared, std::path::Path::new(output), &mut |_| Ok(()))?;
    Ok(
        serde_json::json!({"schema":prepared.manifest["schema"],"songs":prepared.manifest["songs"].as_array().unwrap().len(),"output":output}),
    )
}
