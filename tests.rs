#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_cargo_get_projects() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("Cargo.lock");
        let lock_content = r#"
version = 3

[[package]]
name = "serde"
version = "1.0.197"
dependencies = [
 "serde_derive",
]

[[package]]
name = "serde_derive"
version = "1.0.197"

[[package]]
name = "my_app"
version = "0.1.0"
dependencies = [
 "serde",
]
"#;
        std::fs::write(&lock_path, lock_content).unwrap();

        let projects = get_projects(dir.path()).unwrap();
        assert_eq!(projects.len(), 1);
        let proj = &projects[0];
        assert_eq!(proj.graph.nodes.len(), 3);
        
        let my_app_node = proj.graph.nodes.iter().find(|n| n.name == "my_app").unwrap();
        assert_eq!(my_app_node.dependencies.len(), 1);
        assert_eq!(my_app_node.dependencies[0].0, "serde");
    }
}
