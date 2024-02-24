use clap::Parser;
use requirements;
use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Debug)]
struct Info {
    version: String,
}

#[derive(Deserialize, Debug)]
struct Response {
    info: Info,
}

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
    // println!("Checking file {}", args.file_name);

    let contents =
        fs::read_to_string(args.file_name).expect("Should have been able to read the file");
    let reqs = requirements::parse_str(&contents).unwrap();

    for req in reqs.into_iter() {
        if req.name.is_some() {
            let name = req.name.expect("Not a valid string").to_string();
            let old_version = &req.specs[0].1;
            let url = format!("https://pypi.org/pypi/{}/json", name);
            let body = reqwest::blocking::get(url)
                .expect("REASON")
                .json::<Response>();
            let new_version = body.unwrap().info.version;
            if &new_version != old_version {
                println!("{} {} {}", name, old_version, new_version);
            }
        }
    }
}
