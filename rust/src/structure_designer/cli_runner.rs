use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::api::structure_designer::structure_designer_api_types::{CliConfig, BatchCliConfig};
use std::collections::HashMap;
use std::io::{self, Write};

/// Single run within a batch (local to cli_runner, not in API layer)
struct BatchRun {
  network_name: String,
  output_file: String,
  parameters: HashMap<String, String>,
}

/// Batch configuration data parsed from TOML file (local to cli_runner)
struct BatchConfig {
  cnnd_file: String,
  runs: Vec<BatchRun>,
}

/// Run atomCAD in CLI single-run mode
pub fn run_cli_single_mode(
  designer: &mut StructureDesigner,
  config: CliConfig
) -> Result<(), String> {
  println!("=== CLI Single Run Mode ===");
  println!("CNND file: {}", config.cnnd_file);
  println!("Network: {}", config.network_name);
  println!("Output file: {}", config.output_file);
  println!("Parameters: {:?}", config.parameters);
  
  // 1. Load .cnnd file
  println!("\n[Step 1] Loading .cnnd file...");
  designer.load_node_networks(&config.cnnd_file)
    .map_err(|e| format!("Failed to load .cnnd file '{}': {}", config.cnnd_file, e))?;
  println!("✓ Loaded successfully");
  
  // 2. Validate network exists
  println!("\n[Step 2] Validating network exists...");
  io::stdout().flush().ok();
  
  let available_networks = designer.node_type_registry.get_node_network_names();
  if !available_networks.contains(&config.network_name) {
    eprintln!("ERROR: Network '{}' not found in loaded file.", config.network_name);
    eprintln!("Available networks: {:?}", available_networks);
    return Err(format!("Network '{}' not found", config.network_name));
  }
  println!("✓ Network '{}' found", config.network_name);
  io::stdout().flush().ok();
  
  // 3. Set active network
  println!("\n[Step 3] Setting active network...");
  designer.active_node_network_name = Some(config.network_name.clone());
  println!("✓ Active network set to: {}", config.network_name);
  
  // 4. Apply CLI parameters (stubbed)
  println!("\n[Step 4] Applying parameters...");
  apply_cli_parameters(designer, &config.network_name, &config.parameters)?;
  println!("✓ Parameters applied");
  
  // 5. Evaluate network (stubbed)
  println!("\n[Step 5] Evaluating network...");
  println!("TODO: Mark refresh and evaluate network");
  println!("✓ Network evaluated (stubbed)");
  
  // 6. Export visible atomic structures (stubbed)
  println!("\n[Step 6] Exporting to {}...", config.output_file);
  println!("TODO: Export visible atomic structures to .mol file");
  println!("✓ Export complete (stubbed)");
  
  println!("\n=== CLI Run Complete ===\n");
  Ok(())
}

/// Run atomCAD in CLI batch mode
pub fn run_cli_batch_mode(
  designer: &mut StructureDesigner,
  config: BatchCliConfig
) -> Result<(), String> {
  println!("=== CLI Batch Mode ===");
  println!("Batch file: {}", config.batch_file);
  
  // 1. Parse batch file
  println!("\n[Step 1] Parsing batch configuration file...");
  let batch_config = parse_batch_file(&config.batch_file)?;
  println!("✓ Parsed successfully");
  println!("  - CNND file: {}", batch_config.cnnd_file);
  println!("  - Number of runs: {}", batch_config.runs.len());
  
  // 2. Load .cnnd file ONCE
  let cnnd_file = if !batch_config.cnnd_file.is_empty() {
    &batch_config.cnnd_file
  } else {
    &config.cnnd_file
  };
  
  println!("\n[Step 2] Loading .cnnd file: {}...", cnnd_file);
  designer.load_node_networks(cnnd_file)
    .map_err(|e| format!("Failed to load .cnnd file '{}': {}", cnnd_file, e))?;
  println!("✓ Loaded successfully");
  
  // 3. Run each batch configuration
  println!("\n[Step 3] Running batch configurations...");
  for (i, run) in batch_config.runs.iter().enumerate() {
    println!("\n--- Batch Run {}/{} ---", i + 1, batch_config.runs.len());
    println!("  Network: {}", run.network_name);
    println!("  Output: {}", run.output_file);
    println!("  Parameters: {:?}", run.parameters);
    
    // Validate network exists
    println!("  [3.1] Validating network exists...");
    io::stdout().flush().ok();
    
    let available_networks = designer.node_type_registry.get_node_network_names();
    if !available_networks.contains(&run.network_name) {
      eprintln!("ERROR: Network '{}' not found in loaded file.", run.network_name);
      eprintln!("Available networks: {:?}", available_networks);
      return Err(format!("Network '{}' not found", run.network_name));
    }
    println!("  ✓ Network '{}' found", run.network_name);
    io::stdout().flush().ok();
    
    // Set active network for this run
    println!("  [3.2] Setting active network...");
    designer.active_node_network_name = Some(run.network_name.clone());
    println!("  ✓ Active network set to: {}", run.network_name);
    
    // Apply parameters for this run
    println!("  [3.3] Applying parameters...");
    apply_cli_parameters(designer, &run.network_name, &run.parameters)?;
    println!("  ✓ Parameters applied");
    
    // Evaluate
    println!("  [3.4] Evaluating network...");
    println!("  TODO: Mark refresh and evaluate network");
    println!("  ✓ Network evaluated (stubbed)");
    
    // Export
    println!("  [3.5] Exporting to {}...", run.output_file);
    println!("  TODO: Export visible atomic structures to .mol file");
    println!("  ✓ Export complete (stubbed)");
  }
  
  println!("\n=== Batch Complete: {} runs finished ===\n", batch_config.runs.len());
  Ok(())
}

/// Apply CLI parameters by parsing and preparing values (stubbed for now)
fn apply_cli_parameters(
  designer: &mut StructureDesigner,
  network_name: &str,
  parameters: &HashMap<String, String>
) -> Result<(), String> {
  if parameters.is_empty() {
    println!("  No parameters to apply");
    return Ok(());
  }
  
  // Get the node type for this network (network existence already validated by caller)
  let node_type = designer.node_type_registry
    .get_node_type(network_name)
    .expect("Network should exist (already validated)");
  
  println!("  Found {} parameters defined for '{}'", node_type.parameters.len(), network_name);
  
  // For each CLI parameter, validate and parse it
  for (param_name, value_str) in parameters {
    // Find the parameter definition in the node type
    let param_def = node_type.parameters.iter()
      .find(|p| &p.name == param_name)
      .ok_or_else(|| format!("Parameter '{}' not found in network '{}'", param_name, network_name))?;
    
    // Parse the string value into NetworkResult
    let param_value = NetworkResult::from_string(value_str, &param_def.data_type)?;
    
    println!("  ✓ Parsed parameter '{}' = {} (type: {})", 
      param_name, 
      param_value.to_display_string(), 
      param_def.data_type.to_string()
    );
    
    println!("    TODO: Create constant node and wire to parameter consumers");
  }
  
  Ok(())
}

/// Parse batch file (TOML format)
fn parse_batch_file(batch_file: &str) -> Result<BatchConfig, String> {
  use std::fs;
  
  let content = fs::read_to_string(batch_file)
    .map_err(|e| format!("Failed to read batch file '{}': {}", batch_file, e))?;
  
  parse_batch_toml(&content)
}

/// Parse batch configuration from TOML
fn parse_batch_toml(content: &str) -> Result<BatchConfig, String> {
  use toml::Value;
  
  let value: Value = toml::from_str(content)
    .map_err(|e| format!("TOML parse error: {}", e))?;
  
  let table = value.as_table()
    .ok_or_else(|| "Expected TOML table".to_string())?;
  
  let cnnd_file = table.get("cnnd_file")
    .and_then(|v| v.as_str())
    .unwrap_or("")
    .to_string();
  
  let runs_array = table.get("run")
    .and_then(|v| v.as_array())
    .ok_or_else(|| "Missing 'run' array in batch file".to_string())?;
  
  let mut runs = Vec::new();
  for run_value in runs_array {
    let run_table = run_value.as_table()
      .ok_or_else(|| "Run must be a table".to_string())?;
    
    let network_name = run_table.get("network")
      .and_then(|v| v.as_str())
      .ok_or_else(|| "Missing 'network' in run".to_string())?
      .to_string();
    
    let output_file = run_table.get("output")
      .and_then(|v| v.as_str())
      .ok_or_else(|| "Missing 'output' in run".to_string())?
      .to_string();
    
    let params_table = run_table.get("params")
      .and_then(|v| v.as_table())
      .ok_or_else(|| "Missing 'params' in run".to_string())?;
    
    let mut parameters = HashMap::new();
    for (key, val) in params_table {
      let value_str = match val {
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        _ => return Err(format!("Unsupported parameter value type for '{}'", key)),
      };
      parameters.insert(key.clone(), value_str);
    }
    
    runs.push(BatchRun { network_name, output_file, parameters });
  }
  
  Ok(BatchConfig {
    cnnd_file,
    runs,
  })
}
