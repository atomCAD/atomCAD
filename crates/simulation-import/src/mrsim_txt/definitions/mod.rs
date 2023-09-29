// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use serde::Deserialize;
use std::collections::HashMap;

use crate::mrsim_txt::utils::deserialize_space_separated_ints;
use crate::mrsim_txt::utils::parse_coordinates;

pub struct ParsedData {
    pub data: MrSimTxt,
    pub diagnostics: Diagnostics,
    pub has_calculated_coordinates: bool,
}

#[derive(Debug, Deserialize)]
pub struct MrSimTxt {
    pub specification: Option<Vec<String>>,
    pub header: Header,
    pub metadata: Option<Metadata>,
    #[serde(skip)]
    pub clusters: HashMap<usize, FrameCluster>,
    #[serde(skip)]
    pub calculated: Calculated,
}

#[derive(Debug, Deserialize)]
pub struct Header {
    #[serde(rename = "frame time in femtoseconds")]
    pub frame_time: f64,
    #[serde(rename = "spatial resolution in approximate picometers")]
    pub spatial_resolution: f64,
    #[serde(rename = "uses checkpoints")]
    pub uses_checkpoints: bool,
    #[serde(rename = "frame count")]
    pub frame_count: usize,
    #[serde(rename = "frame cluster size")]
    pub frame_cluster_size: usize,
}

#[derive(Debug, Deserialize)]
pub struct Metadata {}

#[derive(Default, Debug)]
pub struct Calculated {
    pub x: HashMap<usize, Vec<i32>>,
    pub y: HashMap<usize, Vec<i32>>,
    pub z: HashMap<usize, Vec<i32>>,
    pub elements: Vec<i32>,
    pub flags: Vec<i32>,
}

#[derive(Debug, Deserialize)]
pub struct FrameCluster {
    #[serde(rename = "frame start")]
    pub frame_start: usize,
    #[serde(rename = "frame end")]
    pub frame_end: usize,
    pub metadata: Option<HashMap<String, Vec<f64>>>,
    pub atoms: Atoms,
}

#[derive(Deserialize)]
pub struct Atoms {
    #[serde(rename = "x coordinates", deserialize_with = "parse_coordinates")]
    pub x_coordinates: HashMap<usize, Vec<i32>>,
    #[serde(rename = "y coordinates", deserialize_with = "parse_coordinates")]
    pub y_coordinates: HashMap<usize, Vec<i32>>,
    #[serde(rename = "z coordinates", deserialize_with = "parse_coordinates")]
    pub z_coordinates: HashMap<usize, Vec<i32>>,
    #[serde(deserialize_with = "deserialize_space_separated_ints")]
    pub elements: Vec<i32>,
    #[serde(deserialize_with = "deserialize_space_separated_ints")]
    pub flags: Vec<i32>,
}

#[derive(Default)]
pub struct Diagnostics {
    pub messages: Vec<String>,
}
