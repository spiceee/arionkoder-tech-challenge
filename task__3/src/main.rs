use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

// Define a trait representing a common behavior
trait Plugin {
    fn name(&self) -> &'static str;
    fn execute(&self) -> Result<String, String>;
    fn version(&self) -> &'static str {
        "1.0.0"
    }
}

// Global registry for plugins
struct PluginRegistry {
    plugins: HashMap<&'static str, Box<dyn Plugin + Send + Sync>>,
}

impl PluginRegistry {
    fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    fn register<T: Plugin + Send + Sync + 'static>(&mut self, plugin: T) -> Result<(), String> {
        let name = plugin.name();

        // Runtime check: ensure name is not empty
        if name.is_empty() {
            return Err("🚨 Plugin name cannot be empty".to_string());
        }

        // Runtime check: ensure execute method works
        match plugin.execute() {
            Ok(_) => {}
            Err(e) => return Err(format!("🚨 Plugin '{name}' execute method failed: {e}")),
        }

        if self.plugins.contains_key(name) {
            return Err(format!("🚨 Plugin '{name}' is already registered"));
        }

        self.plugins.insert(name, Box::new(plugin));
        println!(
            "➡️ Registered plugin: {} (version: {})",
            name,
            self.plugins[name].version()
        );
        Ok(())
    }

    fn get(&self, name: &str) -> Option<&(dyn Plugin + Send + Sync)> {
        self.plugins.get(name).map(|boxed| boxed.as_ref())
    }

    fn list_plugins(&self) -> Vec<&str> {
        self.plugins.keys().copied().collect()
    }
}

// Using OnceLock since static_mut_refs is now DENY
static REGISTRY: OnceLock<Mutex<PluginRegistry>> = OnceLock::new();

fn get_registry() -> &'static Mutex<PluginRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(PluginRegistry::new()))
}

// Macro to automatically register plugins
macro_rules! register_plugin {
    ($plugin_type:ty) => {{
        let plugin = <$plugin_type>::default();
        let registry = get_registry();
        if let Ok(mut reg) = registry.lock() {
            if let Err(e) = reg.register(plugin) {
                eprintln!(
                    "🚨 Failed to register plugin {}: {}",
                    stringify!($plugin_type),
                    e
                );
            }
        }
    }};
}

// Example plugin implementations
#[derive(Default)]
struct DatabasePlugin;

impl Plugin for DatabasePlugin {
    fn name(&self) -> &'static str {
        "database"
    }

    fn execute(&self) -> Result<String, String> {
        Ok("Database connection established".to_string())
    }

    fn version(&self) -> &'static str {
        "2.1.0"
    }
}

#[derive(Default)]
struct LoggingPlugin;

impl Plugin for LoggingPlugin {
    fn name(&self) -> &'static str {
        "logger"
    }

    fn execute(&self) -> Result<String, String> {
        Ok("🛠️ Logging system initialized".to_string())
    }
}

#[derive(Default)]
struct CachePlugin;

impl Plugin for CachePlugin {
    fn name(&self) -> &'static str {
        "cache"
    }

    fn execute(&self) -> Result<String, String> {
        Ok("🚀 Cache system ready".to_string())
    }

    fn version(&self) -> &'static str {
        "1.5.2"
    }
}

// Example of a plugin that would fail runtime checks
#[derive(Default)]
struct BrokenPlugin;

impl Plugin for BrokenPlugin {
    fn name(&self) -> &'static str {
        "broken"
    }

    fn execute(&self) -> Result<String, String> {
        Err("💣 This plugin is left intentionally broken".to_string())
    }
}

// Auto-registration function called at startup
fn auto_register_plugins() {
    println!("💅 Auto-registering plugins...");
    register_plugin!(DatabasePlugin);
    register_plugin!(LoggingPlugin);
    register_plugin!(CachePlugin);
    register_plugin!(BrokenPlugin); // This will fail the runtime check
}

fn main() {
    auto_register_plugins();

    let registry = get_registry();
    if let Ok(reg) = registry.lock() {
        let plugins = reg.list_plugins();
        println!("✌️ Registered plugins: {plugins:?}");
    }

    if let Ok(reg) = registry.lock() {
        let result = reg.get("database");
        match result {
            Some(plugin) => println!(
                "ℹ️ Database plugin: {} (version: {})",
                plugin.name(),
                plugin.version()
            ),
            None => println!("🚨 Database plugin: Not found"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_registry_new() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.list_plugins().len(), 0);
    }

    #[test]
    fn test_register_valid_plugin() {
        let mut registry = PluginRegistry::new();
        let plugin = DatabasePlugin;

        let result = registry.register(plugin);
        assert!(result.is_ok());
        assert_eq!(registry.list_plugins().len(), 1);
        assert!(registry.list_plugins().contains(&"database"));
    }

    #[test]
    fn test_register_duplicate_plugin() {
        let mut registry = PluginRegistry::new();
        let plugin1 = DatabasePlugin;
        let plugin2 = DatabasePlugin;

        registry.register(plugin1).unwrap();
        let result = registry.register(plugin2);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already registered"));
    }

    #[test]
    fn test_register_broken_plugin() {
        let mut registry = PluginRegistry::new();
        let plugin = BrokenPlugin;

        let result = registry.register(plugin);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("execute method failed"));
    }

    #[test]
    fn test_get_existing_plugin() {
        let mut registry = PluginRegistry::new();
        let plugin = LoggingPlugin;
        registry.register(plugin).unwrap();

        let retrieved = registry.get("logger");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "logger");
    }

    #[test]
    fn test_get_nonexistent_plugin() {
        let registry = PluginRegistry::new();
        let retrieved = registry.get("nonexistent");
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_plugin_versions() {
        let db_plugin = DatabasePlugin;
        let logging_plugin = LoggingPlugin;
        let cache_plugin = CachePlugin;

        assert_eq!(db_plugin.version(), "2.1.0");
        assert_eq!(logging_plugin.version(), "1.0.0"); // default version
        assert_eq!(cache_plugin.version(), "1.5.2");
    }

    #[test]
    fn test_plugin_execute() {
        let db_plugin = DatabasePlugin;
        let result = db_plugin.execute();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Database connection established");
    }

    #[test]
    fn test_list_multiple_plugins() {
        let mut registry = PluginRegistry::new();
        registry.register(DatabasePlugin).unwrap();
        registry.register(LoggingPlugin).unwrap();
        registry.register(CachePlugin).unwrap();

        let plugins = registry.list_plugins();
        assert_eq!(plugins.len(), 3);
        assert!(plugins.contains(&"database"));
        assert!(plugins.contains(&"logger"));
        assert!(plugins.contains(&"cache"));
    }

    #[derive(Default)]
    struct EmptyNamePlugin;

    impl Plugin for EmptyNamePlugin {
        fn name(&self) -> &'static str {
            ""
        }

        fn execute(&self) -> Result<String, String> {
            Ok("test".to_string())
        }
    }

    #[test]
    fn test_register_empty_name_plugin() {
        let mut registry = PluginRegistry::new();
        let plugin = EmptyNamePlugin;

        let result = registry.register(plugin);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("name cannot be empty"));
    }
}
