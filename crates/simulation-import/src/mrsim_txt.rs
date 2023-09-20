// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use num_cpus;
use rayon;
use serde::{Deserialize, Deserializer};
use serde_yaml;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct MrSimTxt {
    specification: Option<Vec<String>>,
    header: Header,
    metadata: Option<Metadata>,
    #[serde(flatten, deserialize_with = "frame_clusters_deserializer")]
    clusters: HashMap<usize, FrameCluster>,
}

impl MrSimTxt {
    pub fn specification(&self) -> &Option<Vec<String>> {
        &self.specification
    }
    pub fn header(&self) -> &Header {
        &self.header
    }
    pub fn metadata(&self) -> &Option<Metadata> {
        &self.metadata
    }
    pub fn clusters(&self) -> &HashMap<usize, FrameCluster> {
        &self.clusters
    }
}

fn frame_clusters_deserializer<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<usize, FrameCluster>, D::Error> {
    let map: HashMap<String, FrameCluster> = HashMap::deserialize(deserializer)?;

    let mut ordered_map = HashMap::new();

    for (key, value) in map.into_iter() {
        if let Some(cluster_idx) = key.strip_prefix("frame cluster ") {
            if let Ok(idx) = cluster_idx.parse::<usize>() {
                ordered_map.insert(idx, value);
            } else {
                return Err(serde::de::Error::custom(format!(
                    "Unexpected frame cluster key: {}",
                    key
                )));
            }
        }
    }

    Ok(ordered_map)
}

#[derive(Debug, Deserialize)]
pub struct Header {
    #[serde(rename = "frame time in femtoseconds")]
    frame_time: f64,
    #[serde(rename = "spatial resolution in approximate picometers")]
    spatial_resolution: f64,
    #[serde(rename = "uses checkpoints")]
    uses_checkpoints: bool,
    #[serde(rename = "frame count")]
    frame_count: usize,
    #[serde(rename = "frame cluster size")]
    frame_cluster_size: usize,
}

impl Header {
    pub fn frame_time(&self) -> &f64 {
        &self.frame_time
    }
    pub fn spatial_resolution(&self) -> &f64 {
        &self.spatial_resolution
    }
    pub fn uses_checkpoints(&self) -> &bool {
        &self.uses_checkpoints
    }
    pub fn frame_count(&self) -> &usize {
        &self.frame_count
    }
    pub fn frame_cluster_size(&self) -> &usize {
        &self.frame_cluster_size
    }
}

#[derive(Debug, Deserialize)]
pub struct Metadata {}

#[derive(Debug, Deserialize)]
pub struct FrameCluster {
    #[serde(rename = "frame start")]
    frame_start: usize,
    #[serde(rename = "frame end")]
    frame_end: usize,
    metadata: Option<HashMap<String, Vec<f64>>>,
    atoms: Atoms,
}

impl FrameCluster {
    pub fn frame_start(&self) -> &usize {
        &self.frame_start
    }
    pub fn frame_end(&self) -> &usize {
        &self.frame_end
    }
    pub fn metadata(&self) -> &Option<HashMap<String, Vec<f64>>> {
        &self.metadata
    }
    pub fn atoms(&self) -> &Atoms {
        &self.atoms
    }
}

#[derive(Deserialize)]
pub struct Atoms {
    #[serde(rename = "x coordinates", deserialize_with = "parse_coordinates")]
    x_coordinates: HashMap<usize, Vec<i32>>,
    #[serde(rename = "y coordinates", deserialize_with = "parse_coordinates")]
    y_coordinates: HashMap<usize, Vec<i32>>,
    #[serde(rename = "z coordinates", deserialize_with = "parse_coordinates")]
    z_coordinates: HashMap<usize, Vec<i32>>,
    #[serde(deserialize_with = "deserialize_space_separated_ints")]
    elements: Vec<i32>,
    #[serde(deserialize_with = "deserialize_space_separated_ints")]
    flags: Vec<i32>,
}

impl Atoms {
    pub fn x_coordinates(&self) -> &HashMap<usize, Vec<i32>> {
        &self.x_coordinates
    }
    pub fn y_coordinates(&self) -> &HashMap<usize, Vec<i32>> {
        &self.y_coordinates
    }
    pub fn z_coordinates(&self) -> &HashMap<usize, Vec<i32>> {
        &self.z_coordinates
    }
    pub fn elements(&self) -> &Vec<i32> {
        &self.elements
    }
    pub fn flags(&self) -> &Vec<i32> {
        &self.flags
    }
}

impl std::fmt::Debug for Atoms {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Atoms")
            .field("x_coordinates count", &self.x_coordinates.len())
            .field("y_coordinates count", &self.y_coordinates.len())
            .field("z_coordinates count", &self.z_coordinates.len())
            .field("elements count", &self.elements.len())
            .field("flags count", &self.flags.len())
            .finish()
    }
}

fn parse_coordinates<'de, D>(deserializer: D) -> Result<HashMap<usize, Vec<i32>>, D::Error>
where
    D: Deserializer<'de>,
{
    let values: Vec<HashMap<usize, String>> = Deserialize::deserialize(deserializer)?;
    let mut coords_map = HashMap::new();
    for map in values {
        for (k, v) in map {
            let coords: Vec<i32> = parse_space_separated_ints(&v)
                .map_err(|e| serde::de::Error::custom(format!("{}", e)))?;
            coords_map.insert(k, coords);
        }
    }
    Ok(coords_map)
}

fn deserialize_space_separated_ints<'de, D>(deserializer: D) -> Result<Vec<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_space_separated_ints(&s).map_err(|e| serde::de::Error::custom(format!("{}", e)))
}

fn parse_space_separated_ints(s: &str) -> Result<Vec<i32>, std::num::ParseIntError> {
    s.split_whitespace()
        .map(|part| part.parse::<i32>())
        .collect()
}

pub fn parse(yaml: &str) -> Result<MrSimTxt, serde_yaml::Error> {
    let num_threads = num_cpus::get() / 2;
    println!("Using {} threads", num_threads);

    let thread_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .unwrap();

    let mr_sim_txt: Result<MrSimTxt, serde_yaml::Error> =
        thread_pool.install(|| serde_yaml::from_str(yaml));

    mr_sim_txt
}
