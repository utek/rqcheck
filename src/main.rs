use clap::Parser;
use requirements;
use std::fs;

/// Simple check for new versions of python packages (pypi.org)
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Requirements file name
    #[arg(short, long)]
    file_name: String,
}

fn main() {
    let args = Args::parse();
    println!("Checking file {}", args.file_name);

    let contents =
        fs::read_to_string(args.file_name).expect("Should have been able to read the file");
    let reqs = requirements::parse_str(&contents).unwrap();

    for req in reqs.into_iter() {
        println!("{:?}", req);
    }
}
