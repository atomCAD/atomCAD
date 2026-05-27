// Debug test for diagnosing load errors on user-supplied .cnnd files.
// Not committed — temporary scaffold.

use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;

#[test]
#[ignore = "manual diagnostic; pass a path via DEBUG_CNND env var"]
fn debug_load_user_file() {
    let path = std::env::var("DEBUG_CNND").unwrap_or_else(|_| {
        "../demolib_with_proxygenerator_Si_2026-05-18_17-11-CET.cnnd".to_string()
    });
    let mut registry = NodeTypeRegistry::new();
    match load_node_networks_from_file(&mut registry, &path) {
        Ok(_) => println!("LOADED OK ({} networks)", registry.node_networks.len()),
        Err(e) => {
            eprintln!("LOAD FAILED: {}", e);
            panic!("{}", e);
        }
    }
}
