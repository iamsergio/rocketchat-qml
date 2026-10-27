// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The renderer the app was asked to draw with (`RC_SLINT_RENDERER`), so that the performance runs
//! compare the renderers one by one. Unset, Slint picks as it always did (`SLINT_BACKEND`, else
//! the default).
//!
//! Slint's own selection falls back to its default renderer, with a line on stderr, when the name
//! isn't one the build has, which would make a run measure the wrong renderer. So the names this
//! build has are listed here, and any other is an error.

/// The environment variable that names the renderer.
pub const ENV: &str = "RC_SLINT_RENDERER";

/// The renderers of this build, each with the name Slint's winit backend knows it by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renderer {
    Femtovg,
    Software,
    // Skia comes with its `rc_slint` feature (`#[cfg(feature = "skia")]`): the variant, its name
    // here and in `NAMES`, and Slint's `renderer-skia` feature behind it.
}

/// What `RC_SLINT_RENDERER` accepts, in the order the error lists them.
const NAMES: [(&str, Renderer); 2] = [
    ("femtovg", Renderer::Femtovg),
    ("software", Renderer::Software),
];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RendererError {
    #[error("{ENV}={0:?} is not a renderer of this build: {known}", known = names())]
    Unknown(String),
    #[error("can't select the {renderer} renderer: {message}")]
    Select {
        renderer: &'static str,
        message: String,
    },
}

fn names() -> String {
    NAMES
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

impl Renderer {
    /// The renderer a value of `RC_SLINT_RENDERER` names.
    ///
    /// # Errors
    ///
    /// For a name this build doesn't have, which includes an empty one.
    pub fn parse(name: &str) -> Result<Self, RendererError> {
        NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, renderer)| *renderer)
            .ok_or_else(|| RendererError::Unknown(name.to_owned()))
    }

    /// The renderer `RC_SLINT_RENDERER` asks for, if it is set.
    ///
    /// # Errors
    ///
    /// For a value that isn't a renderer of this build.
    pub fn from_env() -> Result<Option<Self>, RendererError> {
        std::env::var(ENV)
            .ok()
            .map(|name| Self::parse(&name))
            .transpose()
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        NAMES
            .iter()
            .find(|(_, renderer)| *renderer == self)
            .map_or("unknown", |(name, _)| name)
    }

    /// Makes Slint's winit backend with this renderer the platform. Call it before the first
    /// window. It takes precedence over `SLINT_BACKEND`.
    ///
    /// # Errors
    ///
    /// If the winit backend can't start, for example without a display.
    pub fn select(self) -> Result<(), RendererError> {
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name(self.name().into())
            .select()
            .map_err(|error| RendererError::Select {
                renderer: self.name(),
                message: error.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // APP-001
    #[test]
    fn the_renderers_of_this_build_are_parsed_by_name() {
        assert_eq!(Renderer::parse("femtovg"), Ok(Renderer::Femtovg));
        assert_eq!(Renderer::parse("software"), Ok(Renderer::Software));
        assert_eq!(Renderer::Femtovg.name(), "femtovg");
        assert_eq!(Renderer::Software.name(), "software");
    }

    // APP-001
    #[test]
    fn an_unknown_renderer_is_an_error_that_lists_the_known_ones() {
        for name in ["skia", "", "Software", "gl"] {
            assert_eq!(
                Renderer::parse(name),
                Err(RendererError::Unknown(name.to_owned()))
            );
        }
        let message = Renderer::parse("vulkan").unwrap_err().to_string();
        assert!(message.contains("vulkan") && message.contains("femtovg, software"));
    }
}
