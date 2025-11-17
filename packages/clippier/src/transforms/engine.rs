//! Lua engine for executing transform scripts.

use std::path::Path;

use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;

use super::context::{DependencyInfo, PackageInfo, TransformContext};

/// Lua engine for running transform scripts
pub struct TransformEngine {
    lua: Lua,
}

impl TransformEngine {
    /// Create a new transform engine
    ///
    /// # Errors
    ///
    /// * Failed to create Lua engine
    /// * Failed to load workspace context
    /// * Failed to register API functions
    pub fn new(workspace_root: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let lua = Lua::new();
        let context = TransformContext::new(workspace_root)?;

        // Register helper functions
        register_helpers(&lua)?;

        // Register context API
        register_context_api(&lua, &context)?;

        Ok(Self { lua })
    }

    /// Apply a transform script to the matrix
    ///
    /// # Errors
    ///
    /// * Script fails to compile
    /// * Script encounters runtime error
    /// * Script doesn't return a valid matrix
    pub fn apply_transform(
        &self,
        matrix: &mut Vec<serde_json::Map<String, Value>>,
        script: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Convert Rust matrix to Lua value
        let lua_matrix = self.lua.to_value(matrix)?;

        // Load and execute the script
        self.lua.load(script).exec()?;

        // Get the transform function
        let transform_fn: Function = self.lua.globals().get("transform")?;

        // Get context table
        let context_table: Table = self.lua.globals().get("context")?;

        // Call transform(context, matrix) and get result
        let result: LuaValue = transform_fn.call((context_table, lua_matrix))?;

        // Convert result back to Rust
        *matrix = self.lua.from_value(result)?;

        Ok(())
    }
}

/// Register helper functions in Lua
fn register_helpers(lua: &Lua) -> Result<(), Box<dyn std::error::Error>> {
    lua.load(
        r"
        -- table.filter: filter elements based on predicate
        function table.filter(t, predicate)
            local result = {}
            for _, v in ipairs(t) do
                if predicate(v) then
                    table.insert(result, v)
                end
            end
            return result
        end

        -- table.map: transform elements
        function table.map(t, fn)
            local result = {}
            for _, v in ipairs(t) do
                table.insert(result, fn(v))
            end
            return result
        end

        -- table.contains: check if value exists
        function table.contains(t, value)
            for _, v in ipairs(t) do
                if v == value then
                    return true
                end
            end
            return false
        end

        -- table.find: find first element matching predicate
        function table.find(t, predicate)
            for _, v in ipairs(t) do
                if predicate(v) then
                    return v
                end
            end
            return nil
        end
    ",
    )
    .exec()?;

    Ok(())
}

/// Register context API functions
fn register_context_api(
    lua: &Lua,
    context: &TransformContext,
) -> Result<(), Box<dyn std::error::Error>> {
    let globals = lua.globals();

    // Create context table
    let context_table = lua.create_table()?;

    // Register get_package
    let ctx = context.clone();
    let get_package = lua.create_function(move |lua, name: String| {
        let Some(pkg) = ctx.get_package(&name) else {
            return Err(mlua::Error::RuntimeError(format!(
                "Package not found: {name}"
            )));
        };

        create_package_table(lua, pkg)
    })?;
    context_table.set("get_package", get_package)?;

    // Register is_workspace_member
    let ctx = context.clone();
    let is_workspace_member =
        lua.create_function(move |_lua, name: String| Ok(ctx.is_workspace_member(&name)))?;
    context_table.set("is_workspace_member", is_workspace_member)?;

    // Register get_all_packages
    let ctx = context.clone();
    let get_all_packages = lua.create_function(move |_lua, ()| Ok(ctx.get_all_packages()))?;
    context_table.set("get_all_packages", get_all_packages)?;

    // Register package_depends_on
    let ctx = context.clone();
    let package_depends_on = lua.create_function(move |_lua, (pkg, dep): (String, String)| {
        Ok(ctx.package_depends_on(&pkg, &dep))
    })?;
    context_table.set("package_depends_on", package_depends_on)?;

    // Register feature_exists
    let ctx = context.clone();
    let feature_exists = lua.create_function(move |_lua, (pkg, feat): (String, String)| {
        Ok(ctx.feature_exists(&pkg, &feat))
    })?;
    context_table.set("feature_exists", feature_exists)?;

    // Register log function
    let log_fn = lua.create_function(|_lua, message: String| {
        log::info!("[Transform] {message}");
        Ok(())
    })?;
    context_table.set("log", log_fn)?;

    // Register warn function
    let warn_fn = lua.create_function(|_lua, message: String| {
        log::warn!("[Transform] {message}");
        Ok(())
    })?;
    context_table.set("warn", warn_fn)?;

    // Register error function
    let error_fn = lua.create_function(|_lua, message: String| {
        log::error!("[Transform] {message}");
        Ok(())
    })?;
    context_table.set("error", error_fn)?;

    globals.set("context", context_table)?;

    Ok(())
}

/// Create a Lua table representing a package
fn create_package_table(lua: &Lua, pkg: &PackageInfo) -> mlua::Result<Table> {
    let pkg_table = lua.create_table()?;

    // Basic fields
    pkg_table.set("name", pkg.name.clone())?;
    pkg_table.set("path", pkg.path.to_string_lossy().to_string())?;

    // Add methods
    let pkg_clone = pkg.clone();
    let depends_on =
        lua.create_function(move |_lua, dep_name: String| Ok(pkg_clone.depends_on(&dep_name)))?;
    pkg_table.set("depends_on", depends_on)?;

    let pkg_clone = pkg.clone();
    let has_feature =
        lua.create_function(move |_lua, feature: String| Ok(pkg_clone.has_feature(&feature)))?;
    pkg_table.set("has_feature", has_feature)?;

    let pkg_clone = pkg.clone();
    let feature_definition = lua.create_function(move |_lua, feature: String| {
        Ok(pkg_clone.feature_definition(&feature).cloned())
    })?;
    pkg_table.set("feature_definition", feature_definition)?;

    let pkg_clone = pkg.clone();
    let feature_activates_dependencies = lua.create_function(move |lua, feature: String| {
        let deps = pkg_clone.feature_activates_dependencies(&feature);
        create_dependencies_table(lua, &deps)
    })?;
    pkg_table.set(
        "feature_activates_dependencies",
        feature_activates_dependencies,
    )?;

    let pkg_clone = pkg.clone();
    let skips_feature_on_os =
        lua.create_function(move |_lua, (feature, os): (String, String)| {
            Ok(pkg_clone.skips_feature_on_os(&feature, &os))
        })?;
    pkg_table.set("skips_feature_on_os", skips_feature_on_os)?;

    let pkg_clone = pkg.clone();
    let get_all_features = lua.create_function(move |_lua, ()| Ok(pkg_clone.get_all_features()))?;
    pkg_table.set("get_all_features", get_all_features)?;

    Ok(pkg_table)
}

/// Create a Lua table representing dependencies
fn create_dependencies_table(lua: &Lua, deps: &[DependencyInfo]) -> mlua::Result<Table> {
    let deps_table = lua.create_table()?;

    for (i, dep) in deps.iter().enumerate() {
        let dep_entry = lua.create_table()?;
        dep_entry.set("name", dep.name.clone())?;
        dep_entry.set("optional", dep.optional)?;
        dep_entry.set("workspace_member", dep.workspace_member)?;
        dep_entry.set("features", dep.features.clone())?;

        deps_table.set(i + 1, dep_entry)?;
    }

    Ok(deps_table)
}
