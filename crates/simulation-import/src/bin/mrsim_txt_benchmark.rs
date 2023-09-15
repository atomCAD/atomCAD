// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use anyhow::Result;
use simulation_import::mrsim_txt::parse;
use std::env;
use std::fs;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: mrsim_txt_benchmark(.exe) <file_path>");
        std::process::exit(1);
    }

    let file_path = &args[1];
    let content = fs::read_to_string(file_path)?;

    let start_time = std::time::Instant::now();

    let parsed_result = parse(&content)?;

    let duration = start_time.elapsed();
    println!("Parsing took: {:?}", duration);

    // Optional: Print the parsed result
    println!("{:#?}", parsed_result);

    Ok(())
}
