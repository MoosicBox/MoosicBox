-- Unit Test Coverage Filter Transform
-- Filters out packages that don't have testable source code
--
-- Problem: The unit-tests-checker workflow needs to add tests for code,
-- but some packages are purely organizational (parent packages with no src/)
-- or are metadata-only packages. This transform filters the matrix at
-- generation time rather than checking and skipping in each job.
--
-- This improves efficiency by:
-- 1. Reducing job overhead (no jobs spun up just to be skipped)
-- 2. Simplifying workflow logic (no conditional steps)
-- 3. Making logs cleaner (no skipped job clutter)

function transform(context, matrix)
	local result = {}
	local filtered_count = 0

	for _, entry in ipairs(matrix) do
		local package = entry.package

		if not package then
			-- No package info, keep as-is (shouldn't happen in practice)
			table.insert(result, entry)
		else
			-- Get package metadata
			local pkg = context:get_package(package)
			local pkg_path = pkg.path

			-- Check for lib.rs or main.rs using Lua's io library
			local lib_file = io.open(pkg_path .. "/src/lib.rs", "r")
			local main_file = io.open(pkg_path .. "/src/main.rs", "r")

			local has_lib = lib_file ~= nil
			local has_main = main_file ~= nil

			-- Close files if opened
			if lib_file then
				lib_file:close()
			end
			if main_file then
				main_file:close()
			end

			if has_lib or has_main then
				-- Package has testable source code
				table.insert(result, entry)
			else
				-- Package has no lib.rs or main.rs, filter it out
				filtered_count = filtered_count + 1
				context:warn(
					string.format(
						"Filtering %s from test coverage matrix (no src/lib.rs or src/main.rs found)",
						package
					)
				)
			end
		end
	end

	-- Log summary
	if filtered_count > 0 then
		context:log(string.format("Filtered %d package(s) without testable source code", filtered_count))
	else
		context:log("All packages have testable source code")
	end

	return result
end
