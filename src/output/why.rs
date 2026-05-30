use crate::core::models::WhyProjectResult;

pub fn print_why_results(results: &[WhyProjectResult]) {
    for project in results {
        if project.targets.is_empty() {
            continue;
        }

        println!("Project: {}", project.project_name);

        for target in &project.targets {
            println!("{}: {}", target.target_name, target.target_version);

            for path in &target.paths {
                println!("{}", path.join(" -> "));
            }
            println!();
        }
    }
}
