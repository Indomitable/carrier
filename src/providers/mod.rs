pub mod cargo;
pub mod nuget;

use crate::core::provider::ProviderRegistry;

/// Register all available providers into the registry.
///
/// To add a new provider:
/// 1. Create a new module under `providers/`
/// 2. Implement the `Provider` trait
/// 3. Add one line here to register it
pub fn register_all_providers(registry: &mut ProviderRegistry) {
    registry.register(Box::new(nuget::NuGetProvider::new()));
    registry.register(Box::new(cargo::CargoProvider::new()));
    // Future:
    // registry.register(Box::new(npm::NpmProvider::new()));
    // registry.register(Box::new(pip::PipProvider::new()));
    // registry.register(Box::new(go::GoProvider::new()));
}
