use core_wasm_ast as ast;
use log::info;
use std::io::stdin;
use std::io::stdout;
use std::io::Read;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

mod instrument;
mod wasi;
use wasm_edit::update_value;

pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync>;

use clap::{Parser, Subcommand};

#[derive(Subcommand, Debug)]
enum Commands {
    /// Edit the main memory
    EditMemory {
        /// Initial memory size, in pages (64Kib)
        #[arg(short, long)]
        initial_memory: Option<u32>,
    },

    /// Instrument memory operations (WIP)
    InstrumentMemory {},
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

fn main() -> Result<(), BoxError> {
    env_logger::init();

    let args = Args::parse();

    let mut input = Vec::new();
    stdin().read_to_end(&mut input)?;

    let now = Instant::now();
    let module = Arc::new(
        wasm_parser::parse(&input)
            .map_err(|err| format!("failed to parse Wasm module: {}", err))?,
    );
    let elapsed = now.elapsed();
    info!("decode: {:.2?}", elapsed);

    match args.cmd {
        Commands::EditMemory { initial_memory } => {
            if let Some(initial_memory) = initial_memory {
                if let Some((mem, section_size)) = get_main_memory(Arc::clone(&module)) {
                    // Update value in memory struct
                    let diff = update_value(&mut input, &mem.min, initial_memory);
                    if diff != 0 {
                        // Update section size with size diff
                        let new_size = section_size.value as isize + diff;
                        assert!(new_size > 0);
                        update_value(&mut input, &section_size, new_size as u32);
                    }
                    eprintln!("updated memory");
                }

                if let Some((import, section_size)) = get_memory_import(Arc::clone(&module)) {
                    // Update value in memory struct
                    let diff = update_value(&mut input, &import.min, initial_memory);
                    if diff != 0 {
                        // Update section size with size diff
                        let new_size = section_size.value as isize + diff;
                        assert!(new_size > 0);
                        update_value(&mut input, &section_size, new_size as u32);
                    }
                    eprintln!("updated imported memory");
                }
            }
        }

        Commands::InstrumentMemory { .. } => {
            instrument::transform(Arc::clone(&module));
            input = wasm_printer::wasm::print(&module)?;
        }
    };

    stdout().write_all(&input)?;
    Ok(())
}

fn get_main_memory(module: Arc<ast::Module>) -> Option<(ast::Memory, ast::Value<u32>)> {
    for section in module.sections.lock().unwrap().iter() {
        match &section.value {
            ast::Section::Memory((size, memories)) => {
                return Some((memories.first().unwrap().to_owned(), size.to_owned()));
            }
            _ => {}
        }
    }

    None
}

fn get_memory_import(module: Arc<ast::Module>) -> Option<(ast::Memory, ast::Value<u32>)> {
    for section in module.sections.lock().unwrap().iter() {
        match &section.value {
            ast::Section::Import((size, imports)) => {
                for import in imports.lock().unwrap().iter() {
                    match &import.import_type {
                        ast::ImportType::Memory(mem) => {
                            return Some((mem.clone(), size.to_owned()));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    None
}
