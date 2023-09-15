// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use simulation_import::mrsim_txt::parse;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parser() {
        let sample_yaml_path = "./tests/assets/StrainedShellBearing-15ps.mrsim-txt";
        let sample_yaml_content =
            fs::read_to_string(sample_yaml_path).expect("Failed to read the sample YAML file");

        let parsed = parse(&sample_yaml_content).unwrap();
        println!("{:#?}", parsed);
    }
}
