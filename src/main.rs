use log::info;
use std::io::stdin;
use std::io::stdout;
use std::io::Read;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

mod instrument;
mod wasi;
use wasm_edit::{ast, parser, printer, traverse, update_value};

pub(crate) type BoxError = Box<dyn std::error::Error>;

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
        parser::decode(&input).map_err(|err| format!("failed to parse Wasm module: {}", err))?,
    );
    let elapsed = now.elapsed();
    info!("decode: {:.2?}", elapsed);

    match args.cmd {
        Commands::EditMemory { initial_memory } => {
            if let Some(initial_memory) = initial_memory {
                let (mem, section_size) = get_main_memory(&module).unwrap();

                // Update value in memory struct
                let diff = update_value(&mut input, &mem.initial_memory, initial_memory);
                if diff != 0 {
                    // Update section size with size diff
                    update_value(&mut input, &section_size, section_size.value + diff as u32);
                }
            }
        }

        Commands::InstrumentMemory { .. } => {
            instrument::transform(Arc::clone(&module));
            input = printer::print(&module)?;
        }
    };

    stdout().write_all(&input)?;
    Ok(())
}

fn get_main_memory<'a>(module: &'a ast::Module) -> Option<(ast::Memory, ast::Value<u32>)> {
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
