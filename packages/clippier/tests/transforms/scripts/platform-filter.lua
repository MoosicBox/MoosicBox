-- Filter out matrix entries where OS is windows and any feature is asio
-- Handles both single-feature and multi-feature matrix entries
function transform(context, matrix)
  local result = {}

  for _, entry in ipairs(matrix) do
    local os = entry.os
    local features = entry.features
    local feature = entry.feature

    if not os then
      -- No OS specified, keep as-is
      table.insert(result, entry)
    elseif features and type(features) == "table" then
      -- Handle features array
      local filtered_features = {}

      for _, feat in ipairs(features) do
        if not (os == "windows" and feat == "asio") then
          table.insert(filtered_features, feat)
        else
          context:log("Filtering out ASIO feature on Windows")
        end
      end

      -- Only include entry if there are remaining features
      if #filtered_features > 0 then
        entry.features = filtered_features
        table.insert(result, entry)
      end
    elseif feature then
      -- Handle single feature (legacy format)
      if os == "windows" and feature == "asio" then
        context:log("Filtering out ASIO feature on Windows")
        -- Don't include this entry
      else
        table.insert(result, entry)
      end
    else
      -- No features, keep as-is
      table.insert(result, entry)
    end
  end

  return result
end
