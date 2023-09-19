// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use anyhow::Result;
use colored::*;
use rand::seq::SliceRandom;
use simulation_import::mrsim_txt::parse;
use std::env;
use std::fs;
use std::time::Instant;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut args_iter = args.iter();

    if args.len() < 2 {
        eprintln!("Usage: mrsim_txt_benchmark(.exe) <file_path> [--frames=frame1,frame2,...] [--atoms=atom1,atom2,...]");
        std::process::exit(1);
    }

    let _program_name = args_iter.next().unwrap(); // Skip program name
    let file_path = args_iter.next().unwrap();
    println!("{}", format!("Loading file: {}...", file_path).green());
    println!();

    // Fetch optional frame and atom arguments
    let frames_arg = args_iter.find(|&arg| arg.starts_with("--frames="));
    let atoms_arg = args_iter.find(|&arg| arg.starts_with("--atoms="));

    let start_load = Instant::now();
    let content = fs::read_to_string(file_path)?;
    let duration_load = start_load.elapsed();

    let start_parse = Instant::now();
    let parsed_result = parse(&content)?;
    let duration_parse = start_parse.elapsed();

    println!(
        "{}",
        format!("Loaded file in: {:?}", duration_load).yellow()
    );
    println!(
        "{}",
        format!("Total decoding time: {:?}", duration_parse).yellow()
    );

    let cluster_size = parsed_result.header().frame_cluster_size();
    let spatial_resolution = *parsed_result.header().spatial_resolution() as f64;

    // Calculate the total number of available frames and atoms
    let max_frames = parsed_result.clusters().len() * cluster_size; // assuming all clusters are full
    let max_atoms = if let Some(first_cluster) = parsed_result.clusters().values().next() {
        first_cluster.atoms().x_coordinates().len()
    } else {
        0
    };

    let predefined_frames: Vec<usize> = frames_arg
        .map(|frames| {
            frames[9..]
                .split(',')
                .filter_map(|s| s.parse().ok())
                .collect()
        })
        .unwrap_or_else(|| generate_random_indices(10, 0, max_frames));

    let predefined_atoms: Vec<usize> = atoms_arg
        .map(|atoms| {
            atoms[8..]
                .split(',')
                .filter_map(|s| s.parse().ok())
                .collect()
        })
        .unwrap_or_else(|| generate_random_indices(5, 0, max_atoms));

    for &frame in &predefined_frames {
        // Calculate the cluster index and relative frame index based on the provided frame number and cluster size.
        let cluster_idx = frame / cluster_size;
        let relative_frame_idx = frame % cluster_size;

        if let Some(cluster) = parsed_result.clusters().get(&cluster_idx) {
            let atoms = &cluster.atoms();
            println!();
            println!("Frame {}", frame);
            println!(
                "- timestamp: {:.3} ps",
                frame as f64 * parsed_result.header().frame_time() * 1e-3
            );

            for &atom_idx in &predefined_atoms {
                if let Some(x) = atoms.x_coordinates().get(&atom_idx) {
                    if let Some(y) = atoms.y_coordinates().get(&atom_idx) {
                        if let Some(z) = atoms.z_coordinates().get(&atom_idx) {
                            let element = atoms.elements()[atom_idx as usize];
                            let flag = atoms.flags()[atom_idx as usize];

                            // Convert the coordinates using the spatial resolution
                            let x_pos = x[relative_frame_idx] as f64 * spatial_resolution / 1000.0;
                            let y_pos = y[relative_frame_idx] as f64 * spatial_resolution / 1000.0;
                            let z_pos = z[relative_frame_idx] as f64 * spatial_resolution / 1000.0;

                            println!(
                                " - atom {}: {:.3} {:.3} {:.3} {} {}",
                                atom_idx, x_pos, y_pos, z_pos, element, flag
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn generate_random_indices(n: usize, min: usize, max: usize) -> Vec<usize> {
    let range: Vec<_> = (min..max).collect();
    let mut rng = rand::thread_rng();
    range.choose_multiple(&mut rng, n).cloned().collect()
}
