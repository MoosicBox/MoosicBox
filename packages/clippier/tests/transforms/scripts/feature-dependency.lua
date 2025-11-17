-- Add dependency info to matrix entries based on feature activation
function transform(context, matrix)
  return table.map(matrix, function(entry)
    local package = entry.package
    local feature = entry.feature

    if package and feature then
      local pkg = context:get_package(package)
      if pkg then
        local deps = pkg:feature_activates_dependencies(feature)
        entry.activated_deps = table.map(deps, function(dep)
          return dep.name
        end)
      end
    end

    return entry
  end)
end
