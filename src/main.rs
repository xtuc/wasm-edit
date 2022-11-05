use std::cell::RefCell;
use std::io::stdin;
use std::io::stdout;
use std::io::Read;
use std::io::Write;
use std::rc::Rc;

mod instrument;
use wasm_edit::{ast, parser, printer, traverse, update_value};

type BoxError = Box<dyn std::error::Error>;

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

    let module =
        parser::decode(&input).map_err(|err| format!("failed to parse Wasm module: {}", err))?;

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
            let text_offset = {
                let visitor = Rc::new(instrument::MemoryInstrumentAddData {
                    text_offset: RefCell::new(0),
                    text: "called memory.grow\n",
                });
                traverse::traverse(&module, Rc::clone(&visitor) as Rc<dyn traverse::Visitor>);

                let v = visitor.text_offset.borrow();
                *v
            };

            let funcidx = {
                let visitor = Rc::new(instrument::MemoryInstrumentAddRuntime {
                    text_offset,
                    fd_write: RefCell::new(0),
                    typeidx: RefCell::new(0),
                    funcidx: RefCell::new(0),
                });
                traverse::traverse(&module, Rc::clone(&visitor) as Rc<dyn traverse::Visitor>);

                let v = visitor.funcidx.borrow();
                *v
            };

            let visitor = instrument::MemoryInstrument { funcidx };
            traverse::traverse(&module, Rc::new(visitor));
            input = printer::print(&module)?;
        }
    };

    stdout().write_all(&input)?;
    Ok(())
}

fn get_main_memory<'a>(module: &'a ast::Module) -> Option<(ast::Memory, ast::Value<u32>)> {
    for section in module.sections.borrow().iter() {
        match section {
            ast::Section::Memory((size, memories)) => {
                return Some((memories.first().unwrap().to_owned(), size.to_owned()));
            }
            _ => {}
        }
    }

    None
}
