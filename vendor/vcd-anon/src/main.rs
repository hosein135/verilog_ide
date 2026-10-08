use clap::Parser;
use vcd_anon::anonymizer::VCDAnon;

/// VCD Anonymizer that anonymizes variable names and flattens hierarchy
#[derive(Parser, Debug)]
#[command(name = "vcd-anon")]
#[command(about = "Anonymizes and flattens VCD hierarchy into a single top-level module", long_about = None)]
struct Args {
    /// Input VCD file to anonymize
    #[arg(value_name = "INPUT")]
    input: String,

    /// Output VCD file path
    #[arg(value_name = "OUTPUT")]
    output: String,

    /// Mapping file path
    #[arg(short, long, value_name = "MAPPING", default_value = "mapping.txt")]
    mapping: String,

    /// Keep parameter variables in the output
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    parameters_kept: bool,

    /// Keep variable types as is (do not convert to smaller subset)
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    types_kept: bool,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let input_path = &args.input;
    let output_path = &args.output;
    let mapping_path = &args.mapping;

    println!("Anonymizing and flattening VCD file...");
    println!("Input:  {}", input_path);
    println!("Output: {}", output_path);

    let mut anonymizer = VCDAnon::new(args.parameters_kept, args.types_kept);

    match anonymizer.anonymize_file(input_path, output_path) {
        Ok(_) => {
            println!("✓ VCD file anonymized and flattened successfully");

            anonymizer.save_mapping(mapping_path)?;
            println!("✓ Mapping saved to: {}", mapping_path);
            if !args.parameters_kept {
                println!("✓ Parameter variables are dropped from the output");
            }
            if !args.types_kept {
                println!("✓ Bit/bit-vector variable types are converted to wires");
            }

            let (original_modules, signals, parameters_removed) = anonymizer.get_stats();
            println!("\nStatistics:");
            println!("  Original hierarchy levels: {}", original_modules);
            println!("  Variables anonymized: {}", signals);
            if !args.parameters_kept {
            println!("  Parameters removed: {}", parameters_removed);
            }
        }
        Err(e) => {
            eprintln!("✗ Error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
