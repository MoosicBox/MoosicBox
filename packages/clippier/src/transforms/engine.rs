//! Lua engine for executing transform scripts.

use std::path::Path;

use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;

use super::context::{DependencyInfo, PackageInfo, TransformContext};

/// Lua engine for running transform scripts
pub struct TransformEngine {
    lua: Lua,
    trace_mode: bool,
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
        Self::with_trace(workspace_root, false)
    }

    /// Create a new transform engine with optional trace mode
    ///
    /// # Errors
    ///
    /// * Failed to create Lua engine
    /// * Failed to load workspace context
    /// * Failed to register API functions
    pub fn with_trace(
        workspace_root: &Path,
        trace_mode: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let lua = Lua::new();
        let context = TransformContext::new(workspace_root)?;

        // Register helper functions
        register_helpers(&lua)?;

        // Register context API
        register_context_api(&lua, &context, trace_mode)?;

        Ok(Self { lua, trace_mode })
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
        let original_len = matrix.len();
        let original_matrix = if self.trace_mode {
            Some(matrix.clone())
        } else {
            None
        };

        if self.trace_mode {
            log::info!("[Transform] Input matrix: {original_len} entries");
        }

        // Convert Rust matrix to Lua value
        let lua_matrix = self.lua.to_value(matrix).map_err(|e| {
            format!(
                "Failed to convert matrix to Lua value: {e}\nMatrix: {}",
                serde_json::to_string_pretty(matrix).unwrap_or_default()
            )
        })?;

        // Load and execute the script
        self.lua
            .load(script)
            .exec()
            .map_err(|e| format!("Failed to load transform script: {e}"))?;

        // Get the transform function
        let transform_fn: Function = self
            .lua
            .globals()
            .get("transform")
            .map_err(|e| format!("Transform script must define a 'transform' function: {e}"))?;

        // Get context table
        let context_table: Table = self.lua.globals().get("context")?;

        // Call transform(context, matrix) and get result
        let result: LuaValue = transform_fn
            .call((context_table, lua_matrix))
            .map_err(|e| {
                let mut err_msg = format!("Transform function failed: {e}");
                if let Some(orig) = &original_matrix {
                    use std::fmt::Write;
                    write!(
                        &mut err_msg,
                        "\n\nMatrix before transform:\n{}",
                        serde_json::to_string_pretty(orig).unwrap_or_default()
                    )
                    .unwrap();
                }
                err_msg
            })?;

        // Convert result back to Rust
        *matrix = self.lua.from_value(result).map_err(|e| {
            format!(
                "Transform function must return a valid matrix array: {e}\n\
                 Make sure your transform function returns the matrix (or modified copy)"
            )
        })?;

        if self.trace_mode {
            let new_len = matrix.len();
            let delta_str = if new_len >= original_len {
                format!("+{}", new_len - original_len)
            } else {
                format!("-{}", original_len - new_len)
            };
            log::info!("[Transform] Output matrix: {new_len} entries ({delta_str} change)");

            if new_len != original_len
                && let Some(orig) = original_matrix
            {
                log::debug!("[Transform] Matrix diff:");
                log::debug!("  Removed: {}", original_len.saturating_sub(new_len));
                log::debug!(
                    "  Before: {}",
                    serde_json::to_string(&orig).unwrap_or_default()
                );
                log::debug!(
                    "  After: {}",
                    serde_json::to_string(matrix).unwrap_or_default()
                );
            }
        }

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
#[allow(clippy::too_many_lines)]
fn register_context_api(
    lua: &Lua,
    context: &TransformContext,
    trace_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let globals = lua.globals();

    // Create context table
    let context_table = lua.create_table()?;

    // Register get_package
    let ctx = context.clone();
    context_table.set(
        "get_package",
        lua.create_function(move |lua, (_self, name): (mlua::Value, String)| {
            let Some(pkg) = ctx.get_package(&name) else {
                return Err(mlua::Error::RuntimeError(format!(
                    "Package not found: {name}"
                )));
            };

            create_package_table(lua, pkg)
        })?,
    )?;

    // Register is_workspace_member
    let ctx = context.clone();
    context_table.set(
        "is_workspace_member",
        lua.create_function(move |_lua, (_self, name): (mlua::Value, String)| {
            Ok(ctx.is_workspace_member(&name))
        })?,
    )?;

    // Register get_all_packages
    let ctx = context.clone();
    context_table.set(
        "get_all_packages",
        lua.create_function(move |_lua, _self: mlua::Value| Ok(ctx.get_all_packages()))?,
    )?;

    // Register package_depends_on
    let ctx = context.clone();
    context_table.set(
        "package_depends_on",
        lua.create_function(
            move |_lua, (_self, pkg, dep): (mlua::Value, String, String)| {
                Ok(ctx.package_depends_on(&pkg, &dep))
            },
        )?,
    )?;

    // Register feature_exists
    let ctx = context.clone();
    context_table.set(
        "feature_exists",
        lua.create_function(
            move |_lua, (_self, pkg, feat): (mlua::Value, String, String)| {
                Ok(ctx.feature_exists(&pkg, &feat))
            },
        )?,
    )?;

    // Register log function
    context_table.set(
        "log",
        lua.create_function(|_lua, (_self, message): (mlua::Value, String)| {
            log::info!("[Transform] {message}");
            Ok(())
        })?,
    )?;

    // Register warn function
    context_table.set(
        "warn",
        lua.create_function(|_lua, (_self, message): (mlua::Value, String)| {
            log::warn!("[Transform] {message}");
            Ok(())
        })?,
    )?;

    // Register error function
    context_table.set(
        "error",
        lua.create_function(|_lua, (_self, message): (mlua::Value, String)| {
            log::error!("[Transform] {message}");
            Ok(())
        })?,
    )?;

    // Register debug function for structured logging
    context_table.set(
        "debug",
        lua.create_function(move |lua, (_self, data): (mlua::Value, mlua::Value)| {
            // Try to convert to JSON for pretty printing
            let debug_str = lua
                .from_value::<serde_json::Value>(data.clone())
                .map_or_else(
                    |_| format!("{data:?}"),
                    |json_val| {
                        serde_json::to_string_pretty(&json_val)
                            .unwrap_or_else(|_| format!("{data:?}"))
                    },
                );

            if trace_mode {
                log::debug!("[Transform Debug]\n{debug_str}");
            } else {
                log::trace!("[Transform Debug]\n{debug_str}");
            }
            Ok(())
        })?,
    )?;

    // Register inspect function for dependency visualization
    let ctx = context.clone();
    context_table.set(
        "inspect",
        lua.create_function(
            move |_lua, (_self, pkg_name, feature): (mlua::Value, String, String)| {
                let Some(pkg) = ctx.get_package(&pkg_name) else {
                    return Ok(format!("Package '{pkg_name}' not found"));
                };

                let mut output = format!("Inspecting {pkg_name}:{feature}\n");

                if !pkg.has_feature(&feature) {
                    use std::fmt::Write;
                    writeln!(&mut output, "  ⚠ Feature '{feature}' does not exist").unwrap();
                    return Ok(output);
                }

                let deps = pkg.feature_activates_dependencies(&feature);
                if deps.is_empty() {
                    output.push_str("  No dependencies activated\n");
                } else {
                    output.push_str("  Activates dependencies:\n");
                    for dep in deps {
                        use std::fmt::Write;
                        writeln!(
                            &mut output,
                            "    → {}{}",
                            dep.name,
                            if dep.features.is_empty() {
                                String::new()
                            } else {
                                format!("/{}", dep.features.join(","))
                            }
                        )
                        .unwrap();
                    }
                }

                Ok(output)
            },
        )?,
    )?;

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
    pkg_table.set(
        "depends_on",
        lua.create_function(move |_lua, (_self, dep_name): (mlua::Value, String)| {
            Ok(pkg_clone.depends_on(&dep_name))
        })?,
    )?;

    let pkg_clone = pkg.clone();
    pkg_table.set(
        "has_feature",
        lua.create_function(move |_lua, (_self, feature): (mlua::Value, String)| {
            Ok(pkg_clone.has_feature(&feature))
        })?,
    )?;

    let pkg_clone = pkg.clone();
    pkg_table.set(
        "feature_definition",
        lua.create_function(move |_lua, (_self, feature): (mlua::Value, String)| {
            Ok(pkg_clone.feature_definition(&feature).cloned())
        })?,
    )?;

    let pkg_clone = pkg.clone();
    pkg_table.set(
        "feature_activates_dependencies",
        lua.create_function(move |lua, (_self, feature): (mlua::Value, String)| {
            let deps = pkg_clone.feature_activates_dependencies(&feature);
            create_dependencies_table(lua, &deps)
        })?,
    )?;

    let pkg_clone = pkg.clone();
    pkg_table.set(
        "skips_feature_on_os",
        lua.create_function(
            move |_lua, (_self, feature, os): (mlua::Value, String, String)| {
                Ok(pkg_clone.skips_feature_on_os(&feature, &os))
            },
        )?,
    )?;

    let pkg_clone = pkg.clone();
    pkg_table.set(
        "get_all_features",
        lua.create_function(move |_lua, _self: mlua::Value| Ok(pkg_clone.get_all_features()))?,
    )?;

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
