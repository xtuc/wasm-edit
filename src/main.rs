use std::io::stdin;
use std::io::stdout;
use std::io::Read;
use std::io::Write;

use wasm_edit::{ast, parser, update_value};

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
            let (codes, _section_size) = get_code_section(&module).unwrap();
            for code in codes {
                visit_code(code, rewrite_memory_grow, &mut input)
            }
        }
    };

    stdout().write_all(&input)?;
    Ok(())
}

fn rewrite_memory_grow<'a>(instr: &'a ast::Value<ast::Instr>, input: &mut Vec<u8>) {
    if matches!(instr.value, ast::Instr::memory_grow(_)) {
        let new_instr = ast::Instr::call(66);
        let diff = update_value(input, &instr, new_instr);
        assert_eq!(diff, 0);
    }
}

fn get_main_memory<'a>(module: &'a ast::Module) -> Option<(&'a ast::Memory, &'a ast::Value<u32>)> {
    for section in &module.sections {
        match section {
            ast::Section::Memory((size, memories)) => {
                return Some((memories.first().unwrap(), size));
            }
            _ => {}
        }
    }

    None
}

fn get_code_section<'a>(module: &'a ast::Module) -> Option<(&'a [ast::Code], &'a ast::Value<u32>)> {
    for section in &module.sections {
        match section {
            ast::Section::Code((size, codes)) => {
                return Some((codes, size));
            }
            _ => {}
        }
    }

    None
}

type VisitorFn = fn(&ast::Value<ast::Instr>, &mut Vec<u8>);
fn visit_instr(instr: &ast::Value<ast::Instr>, visitor_fn: VisitorFn, input: &mut Vec<u8>) {
    match &instr.value {
        ast::Instr::Block(_, body) | ast::Instr::Loop(_, body) | ast::Instr::If(_, body) => {
            for instr in body {
                visit_instr(instr, visitor_fn, input);
            }
        }
        _ => visitor_fn(instr, input),
    }
}

fn visit_code(code: &ast::Code, visitor_fn: VisitorFn, input: &mut Vec<u8>) {
    for instr in &code.body {
        visit_instr(instr, visitor_fn, input);
    }
}
