//! # Migration Discovery
//!
//! This module contains different strategies for discovering and loading migrations.
//! Each discovery method has different trade-offs in terms of flexibility, performance,
//! and deployment considerations.
//!
//! ## Available Discovery Methods
//!
//! * **[`embedded`]**: Migrations compiled into the binary (recommended for production)
//! * **[`directory`]**: Migrations loaded from filesystem (good for development)  
//! * **[`code`]**: Migrations defined programmatically in Rust code
//!
//! ## Choosing a Discovery Method
//!
//! ### Embedded Migrations (`feature = "embedded"`)
//!
//! **Best for**: Production deployments, distributed applications
//!
//! * ✅ No runtime filesystem dependencies
//! * ✅ Guaranteed availability of migrations
//! * ✅ Smaller deployment artifacts
//! * ❌ Requires recompilation to change migrations
//!
//! ### Directory Migrations (`feature = "directory"`)
//!
//! **Best for**: Development, dynamic environments
//!
//! * ✅ Easy to modify migrations without rebuilding
//! * ✅ Good for development and testing
//! * ✅ Human-readable SQL files
//! * ❌ Runtime filesystem dependency
//! * ❌ Files can be missing or modified
//!
//! ### Code Migrations (`feature = "code"`)
//!
//! **Best for**: Generated migrations, complex logic
//!
//! * ✅ Type-safe migration definitions
//! * ✅ Can use Rust logic and query builders
//! * ✅ Programmatic migration generation
//! * ❌ More complex to write and maintain
//! * ❌ Harder to review changes

#[cfg(feature = "embedded")]
pub mod embedded;

#[cfg(feature = "directory")]
pub mod directory;

#[cfg(feature = "code")]
pub mod code;
