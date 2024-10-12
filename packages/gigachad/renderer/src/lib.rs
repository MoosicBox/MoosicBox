#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use async_trait::async_trait;
use gigachad_transformer::ContainerElement;

pub trait RenderRunner: Send + Sync {
    /// # Errors
    ///
    /// Will error if fails to run
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;
}

#[async_trait]
pub trait Renderer: Send + Sync {
    /// # Errors
    ///
    /// Will error if `Renderer` implementation app fails to start
    async fn init(
        &mut self,
        width: u16,
        height: u16,
        x: Option<i32>,
        y: Option<i32>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to run.
    async fn to_runner(
        &mut self,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send + 'static>>;

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to render the elements.
    fn render(
        &mut self,
        elements: ContainerElement,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;
}
