use std::collections::BTreeMap;
use std::path::PathBuf;

use colored::Colorize;

use crate::core::models::OutdatedDependency;

/// Print the outdated dependencies as a simple list, grouped by ecosystem.
pub fn print_outdated_table(outdated: &[OutdatedDependency]) {
    if outdated.is_empty() {
        println!(
            "\n{}",
            "  ✅ All dependencies are up to date!".green().bold()
        );
        return;
    }

    println!("\n{}\n", "📦 Carrier — Outdated Dependencies".bold());

    // Group by (ecosystem, source_file) for organized output.
    let mut groups: BTreeMap<(String, PathBuf), Vec<&OutdatedDependency>> = BTreeMap::new();
    for dep in outdated {
        let key = (dep.ecosystem.to_string(), dep.source_file.clone());
        groups.entry(key).or_default().push(dep);
    }

    for ((ecosystem, source_file), deps) in &groups {
        let file_display = source_file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        println!(
            "  {} {} ({})",
            ecosystem_icon(ecosystem),
            ecosystem.cyan().bold(),
            file_display.dimmed()
        );

        for dep in deps {
            println!(
                "    - {}: {} -> {}",
                dep.name,
                dep.current_version,
                dep.latest_version.yellow()
            );
        }

        println!();
    }

    let total = outdated.len();
    println!(
        "  {} {total} outdated {} found.\n",
        "⚠".yellow(),
        if total == 1 {
            "dependency"
        } else {
            "dependencies"
        }
    );
}

/// Get an icon for each ecosystem.
fn ecosystem_icon(ecosystem: &str) -> &'static str {
    match ecosystem {
        "NuGet" => "🔷",
        "npm" => "📗",
        "Cargo" => "🦀",
        "pip" => "🐍",
        "Go" => "🐹",
        _ => "📦",
    }
}
