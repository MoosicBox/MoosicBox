-- Real-world ASIO compatibility transform
-- Removes features that transitively enable cpal/asio on Windows
-- This demonstrates dependency graph analysis that can't be done with static config
function transform(context, matrix)
  local result = {}

  -- Helper function to check if a feature transitively enables cpal/asio
  -- Uses proper cycle detection instead of arbitrary depth limits
  local function enables_cpal_asio(pkg_name, pkg, feature, os, visited)
    if os ~= "windows" then
      return false, nil
    end

    -- Initialize visited set for cycle detection
    visited = visited or {}
    local visit_key = pkg_name .. "/" .. feature
    if visited[visit_key] then
      return false, nil -- Already visited this path (cycle detected)
    end
    visited[visit_key] = true

    -- Get all dependencies activated by this feature
    local activated_deps = pkg:feature_activates_dependencies(feature)

    for _, dep in ipairs(activated_deps) do
      -- Check each feature being activated on this dependency
      for _, dep_feature in ipairs(dep.features) do
        -- THE CORE CHECK: Does this directly enable cpal/asio?
        -- Check both workspace members AND external dependencies
        if dep.name == "cpal" and dep_feature == "asio" then
          return true, string.format("%s/%s -> cpal/asio", pkg_name, feature)
        end

        -- Recursively check workspace dependencies only
        -- (external deps like cpal are checked above)
        if context:is_workspace_member(dep.name) then
          local dep_pkg = context:get_package(dep.name)
          local enables, chain = enables_cpal_asio(dep.name, dep_pkg, dep_feature, os, visited)
          if enables then
            -- Build the full dependency chain for debugging
            return true, string.format("%s/%s -> %s", pkg_name, feature, chain)
          end
        end
      end
    end

    return false, nil
  end

  -- Process each matrix entry
  for _, entry in ipairs(matrix) do
    local package = entry.package
    local os = entry.os

    if not package or not os then
      -- No package or OS info, keep as-is
      table.insert(result, entry)
    else
      local pkg = context:get_package(package)
      if not pkg then
        -- Package not found in workspace, keep as-is
        table.insert(result, entry)
      else
        local features = entry.features

        if features and type(features) == "table" then
          -- Handle features array (modern format)
          local filtered_features = {}
          local skipped_features = {}

          for _, feature in ipairs(features) do
            local enables, chain = enables_cpal_asio(package, pkg, feature, os)

            if enables then
              table.insert(skipped_features, { feature = feature, chain = chain })
            else
              table.insert(filtered_features, feature)
            end
          end

          -- Log what was filtered
          if #skipped_features > 0 then
            for _, skipped in ipairs(skipped_features) do
              context:warn(
                string.format(
                  "Removing feature '%s' from %s on %s (dependency chain: %s)",
                  skipped.feature,
                  package,
                  os,
                  skipped.chain
                )
              )
            end
          end

          -- Only include entry if there are remaining features
          if #filtered_features > 0 then
            entry.features = filtered_features
            table.insert(result, entry)
          else
            context:warn(
              string.format("Removing entire entry for %s on %s (all features filtered)", package, os)
            )
          end
        elseif entry.feature then
          -- Handle single feature (legacy format)
          local feature = entry.feature
          local enables, chain = enables_cpal_asio(package, pkg, feature, os)

          if enables then
            context:warn(
              string.format("Skipping %s/%s on %s (dependency chain: %s)", package, feature, os, chain)
            )
          else
            table.insert(result, entry)
          end
        else
          -- No features field, keep as-is
          table.insert(result, entry)
        end
      end
    end
  end

  return result
end
