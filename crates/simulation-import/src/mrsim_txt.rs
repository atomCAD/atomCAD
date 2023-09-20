// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use serde::{Deserialize, Deserializer};
use serde_yaml;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Deserialize)]
pub struct MrSimTxt {
    specification: Option<Vec<String>>,
    header: Header,
    metadata: Option<Metadata>,
    #[serde(skip)]
    clusters: HashMap<usize, FrameCluster>,
}

impl MrSimTxt {
    pub fn specification(&self) -> Option<&Vec<String>> {
        self.specification.as_ref()
    }
    pub fn header(&self) -> &Header {
        &self.header
    }
    pub fn metadata(&self) -> Option<&Metadata> {
        self.metadata.as_ref()
    }
    pub fn clusters(&self) -> &HashMap<usize, FrameCluster> {
        &self.clusters
    }
}

fn frame_clusters_deserializer(
    map: HashMap<String, FrameCluster>,
) -> Result<HashMap<usize, FrameCluster>, serde_yaml::Error> {
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

#[derive(Default)]
pub struct Diagnostics {
    messages: Vec<String>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, message: String) {
        self.messages.push(message);
    }
    // Provide a method to get an iterator over the messages
    pub fn iter(&self) -> std::slice::Iter<'_, String> {
        self.messages.iter()
    }
    // Keeping this method in case you still want direct access to the Vec
    pub fn messages(&self) -> &Vec<String> {
        &self.messages
    }
}

fn split_yaml(yaml: &str) -> (String, Vec<String>) {
    let mut non_cluster = String::new();
    let mut clusters = Vec::new();
    let mut current_cluster = String::new();
    let mut is_inside_cluster = false;

    for line in yaml.lines() {
        if line.starts_with("frame cluster ") {
            if !current_cluster.is_empty() {
                clusters.push(current_cluster.clone());
                current_cluster.clear();
            }
            is_inside_cluster = true;
        }

        if is_inside_cluster {
            current_cluster.push_str(line);
            current_cluster.push('\n');
        } else {
            non_cluster.push_str(line);
            non_cluster.push('\n');
        }
    }

    if !current_cluster.is_empty() {
        clusters.push(current_cluster);
    }

    (non_cluster, clusters)
}

pub fn parse(yaml: &str) -> Result<(MrSimTxt, Diagnostics), serde_yaml::Error> {
    let mut diagnostics = Diagnostics::new();

    let start = Instant::now();

    let (non_cluster, clusters) = split_yaml(yaml);

    let preprocessed_duration = start.elapsed();

    diagnostics.add(format!(
        "Preprocessed text in: {}ms",
        preprocessed_duration.as_millis()
    ));

    let header_start = Instant::now();
    // Parse non-cluster part into a partial MrSimTxt structure
    let mut mr_sim_txt: MrSimTxt = serde_yaml::from_str(&non_cluster)?;
    let header_duration = header_start.elapsed();

    diagnostics.add(format!(
        "Parsed header in: {}ms",
        header_duration.as_millis()
    ));

    // This is where all parsed clusters would be stored
    let mut all_clusters: HashMap<usize, FrameCluster> = HashMap::new();

    let cluster_start = Instant::now();
    let clusters_data: Result<Vec<HashMap<String, FrameCluster>>, serde_yaml::Error> = clusters
        .into_par_iter()
        .map(|cluster_yaml| serde_yaml::from_str::<HashMap<String, FrameCluster>>(&cluster_yaml))
        .collect::<Result<Vec<_>, _>>();

    let cluster_duration = cluster_start.elapsed();

    diagnostics.add(format!(
        "Parsed clusters in: {} ms",
        cluster_duration.as_millis()
    ));

    let thread_count = rayon::current_num_threads();
    diagnostics.add(format!("Using {} threads", thread_count));

    match clusters_data {
        Ok(cluster_maps) => {
            for map in cluster_maps {
                // Convert each cluster map into the desired format using your deserializer logic
                let ordered_map = frame_clusters_deserializer(map)?;

                // Merge the ordered_map into the all_clusters map
                all_clusters.extend(ordered_map);
            }
        }
        Err(e) => return Err(e),
    }

    // Assign the combined clusters map to the main structure
    mr_sim_txt.clusters = all_clusters;

    Ok((mr_sim_txt, diagnostics))
}
