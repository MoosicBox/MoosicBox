#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub use moosicbox_http_models;

use moosicbox_http_models::Method;

/// An enum signifying that some of type `T` is allowed, or `All` (anything is allowed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllOrSome<T> {
    /// Everything is allowed. Usually equivalent to the `*` value.
    All,

    /// Only some of `T` is allowed
    Some(T),
}

/// Default as `AllOrSome::All`.
impl<T> Default for AllOrSome<T> {
    fn default() -> Self {
        Self::All
    }
}

impl<T> AllOrSome<T> {
    /// Returns whether this is an `All` variant.
    pub const fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    /// Returns whether this is a `Some` variant.
    pub const fn is_some(&self) -> bool {
        !self.is_all()
    }

    /// Provides a shared reference to `T` if variant is `Some`.
    pub const fn as_ref(&self) -> Option<&T> {
        match *self {
            Self::All => None,
            Self::Some(ref t) => Some(t),
        }
    }

    /// Provides a mutable reference to `T` if variant is `Some`.
    pub const fn as_mut(&mut self) -> Option<&mut T> {
        match *self {
            Self::All => None,
            Self::Some(ref mut t) => Some(t),
        }
    }
}

#[cfg(test)]
#[test]
fn tests() {
    assert!(AllOrSome::<()>::All.is_all());
    assert!(!AllOrSome::<()>::All.is_some());

    assert!(!AllOrSome::Some(()).is_all());
    assert!(AllOrSome::Some(()).is_some());
}

#[derive(Debug, Clone)]
pub struct Cors {
    pub allowed_origins: AllOrSome<Vec<String>>,
    pub allowed_methods: AllOrSome<Vec<Method>>,
    pub allowed_headers: AllOrSome<Vec<String>>,
    pub expose_headers: AllOrSome<Vec<String>>,
    pub supports_credentials: bool,
    pub max_age: Option<u32>,
}

#[allow(clippy::derivable_impls)]
impl Default for Cors {
    fn default() -> Self {
        Self {
            allowed_origins: AllOrSome::Some(vec![]),
            allowed_methods: AllOrSome::Some(vec![]),
            allowed_headers: AllOrSome::Some(vec![]),
            expose_headers: AllOrSome::Some(vec![]),
            supports_credentials: false,
            max_age: None,
        }
    }
}

impl Cors {
    #[must_use]
    pub fn allow_any_origin(mut self) -> Self {
        self.allowed_origins = AllOrSome::All;
        self
    }

    #[must_use]
    pub fn allow_origin<T: Into<String>>(mut self, origin: T) -> Self {
        match &mut self.allowed_origins {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.push(origin.into());
            }
        }

        self
    }

    #[must_use]
    pub fn allowed_origins<T: Into<String>, I: IntoIterator<Item = T>>(
        mut self,
        origins: I,
    ) -> Self {
        match &mut self.allowed_origins {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.extend(origins.into_iter().map(Into::into));
            }
        }

        self
    }

    #[must_use]
    pub fn allow_any_method(mut self) -> Self {
        self.allowed_methods = AllOrSome::All;
        self
    }

    #[must_use]
    pub fn allow_method<T: Into<Method>>(mut self, method: T) -> Self {
        match &mut self.allowed_methods {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.push(method.into());
            }
        }

        self
    }

    #[must_use]
    pub fn allowed_methods<T: Into<Method>, I: IntoIterator<Item = T>>(
        mut self,
        methods: I,
    ) -> Self {
        match &mut self.allowed_methods {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.extend(methods.into_iter().map(Into::into));
            }
        }

        self
    }

    #[must_use]
    pub fn allow_any_header(mut self) -> Self {
        self.allowed_headers = AllOrSome::All;
        self
    }

    #[must_use]
    pub fn allow_header<T: Into<String>>(mut self, header: T) -> Self {
        match &mut self.allowed_headers {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.push(header.into());
            }
        }

        self
    }

    #[must_use]
    pub fn allowed_headers<T: Into<String>, I: IntoIterator<Item = T>>(
        mut self,
        headers: I,
    ) -> Self {
        match &mut self.allowed_headers {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.extend(headers.into_iter().map(Into::into));
            }
        }

        self
    }

    #[must_use]
    pub fn expose_any_header(mut self) -> Self {
        self.expose_headers = AllOrSome::All;
        self
    }

    #[must_use]
    pub fn expose_header<T: Into<String>>(mut self, header: T) -> Self {
        match &mut self.expose_headers {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.push(header.into());
            }
        }

        self
    }

    #[must_use]
    pub fn expose_headers<T: Into<String>, I: IntoIterator<Item = T>>(
        mut self,
        headers: I,
    ) -> Self {
        match &mut self.expose_headers {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.extend(headers.into_iter().map(Into::into));
            }
        }

        self
    }

    #[must_use]
    pub const fn support_credentials(mut self) -> Self {
        self.supports_credentials = true;
        self
    }

    #[must_use]
    pub fn max_age(mut self, max_age: impl Into<Option<u32>>) -> Self {
        self.max_age = max_age.into();
        self
    }
}
