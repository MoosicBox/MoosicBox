pub use {
            other_dep::{
                star_mod::*,
                unnamed_import::{UnnamedTrait as _, other_mod},
                renamed_import as new_name,
                sub_import::{mod1, mod2},
            },
            futures as futures_renamed,
            reqwest,
        };
