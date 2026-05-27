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
        Ok(_) => {
            // Count closure nodes synthesised across all networks.
            let mut closure_count = 0usize;
            let mut networks_with_closure = Vec::new();
            for (name, network) in &registry.node_networks {
                let n = network
                    .nodes
                    .values()
                    .filter(|n| n.node_type_name == "closure")
                    .count();
                if n > 0 {
                    closure_count += n;
                    networks_with_closure.push((name.clone(), n));
                }
            }
            println!(
                "LOADED OK ({} networks, {} closures synthesised)",
                registry.node_networks.len(),
                closure_count
            );
            for (name, n) in &networks_with_closure {
                println!("  {}: {} closure(s)", name, n);
            }
        }
        Err(e) => {
            eprintln!("LOAD FAILED: {}", e);
            panic!("{}", e);
        }
    }
}
