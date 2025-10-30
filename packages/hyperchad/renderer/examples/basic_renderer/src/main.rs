#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic renderer implementation example.
//!
//! This example demonstrates how to implement the core `Renderer` trait from
//! `hyperchad_renderer` to create a simple console-based renderer that outputs
//! HTML content.

use async_trait::async_trait;
use hyperchad_color::Color;
use hyperchad_renderer::{
    Content, Handle, RenderRunner, Renderer, RendererEvent, ReplaceContainer, ToRenderRunner, View,
    transformer::{Container, Element, Number, ResponsiveTrigger},
};
use log::info;
use std::error::Error;
use switchy_async::Builder;

/// A simple console-based renderer that outputs HTML to stdout.
///
/// This renderer demonstrates the basic implementation of the `Renderer` trait.
/// It stores window properties and outputs generated content as HTML.
struct ConsoleRenderer {
    /// Window width in logical pixels
    width: f32,
    /// Window height in logical pixels
    height: f32,
    /// Window title
    title: String,
    /// Window description
    description: String,
    /// Background color
    background: Option<Color>,
    /// Viewport meta tag value
    viewport: Option<String>,
}

impl ConsoleRenderer {
    /// Create a new console renderer with default settings.
    fn new() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
            title: "Console Renderer".to_string(),
            description: "A basic HyperChad renderer example".to_string(),
            background: None,
            viewport: None,
        }
    }

    /// Render a container to HTML string.
    #[allow(clippy::unused_self)]
    fn render_container(&self, container: &Container) -> String {
        // Use the transformer's Display implementation to generate HTML
        container.to_string()
    }

    /// Render a complete view with primary content and fragments.
    fn render_view(&self, view: &View) -> String {
        let mut output = String::new();

        // Render primary content if present
        if let Some(primary) = &view.primary {
            output.push_str("<div id=\"primary-content\">\n");
            output.push_str(&self.render_container(primary));
            output.push_str("\n</div>\n");
        }

        // Render fragments
        if !view.fragments.is_empty() {
            output.push_str("\n<!-- Fragments -->\n");
            for fragment in &view.fragments {
                use std::fmt::Write;
                let _ = writeln!(
                    &mut output,
                    "<div id=\"fragment\" data-selector=\"{:?}\">",
                    fragment.selector
                );
                output.push_str(&self.render_container(&fragment.container));
                output.push_str("\n</div>\n");
            }
        }

        // Show delete selectors if present
        if !view.delete_selectors.is_empty() {
            output.push_str("\n<!-- Delete Selectors -->\n");
            for selector in &view.delete_selectors {
                use std::fmt::Write;
                let _ = writeln!(&mut output, "<!-- DELETE: {selector:?} -->");
            }
        }

        output
    }
}

/// Simple runner that executes the renderer synchronously.
struct ConsoleRenderRunner {
    renderer: ConsoleRenderer,
}

impl RenderRunner for ConsoleRenderRunner {
    fn run(&mut self) -> Result<(), Box<dyn Error + Send + 'static>> {
        info!("ConsoleRenderRunner started");
        info!(
            "Window: {}x{} - {}",
            self.renderer.width, self.renderer.height, self.renderer.title
        );
        Ok(())
    }
}

impl ToRenderRunner for ConsoleRenderer {
    fn to_runner(self, _handle: Handle) -> Result<Box<dyn RenderRunner>, Box<dyn Error + Send>> {
        Ok(Box::new(ConsoleRenderRunner { renderer: self }))
    }
}

#[async_trait]
impl Renderer for ConsoleRenderer {
    async fn init(
        &mut self,
        width: f32,
        height: f32,
        _x: Option<i32>,
        _y: Option<i32>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
        viewport: Option<&str>,
    ) -> Result<(), Box<dyn Error + Send + 'static>> {
        // Store initialization parameters
        self.width = width;
        self.height = height;
        self.background = background;

        if let Some(t) = title {
            self.title = t.to_string();
        }

        if let Some(d) = description {
            self.description = d.to_string();
        }

        if let Some(v) = viewport {
            self.viewport = Some(v.to_string());
        }

        info!("Renderer initialized:");
        info!("  Size: {}x{}", self.width, self.height);
        info!("  Title: {}", self.title);
        info!("  Description: {}", self.description);
        if let Some(bg) = &self.background {
            info!("  Background: {bg:?}");
        }
        if let Some(vp) = &self.viewport {
            info!("  Viewport: {vp}");
        }

        Ok(())
    }

    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        info!("Added responsive trigger '{name}': {trigger:?}");
    }

    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn Error + Send + 'static>> {
        info!(
            "Event emitted: {} = {:?}",
            event_name,
            event_value.as_deref().unwrap_or("(no value)")
        );
        Ok(())
    }

    async fn render(&self, view: View) -> Result<(), Box<dyn Error + Send + 'static>> {
        info!("=== Rendering View ===");

        let html = self.render_view(&view);

        println!("\n{}", "=".repeat(80));
        println!("RENDERED OUTPUT:");
        println!("{}", "=".repeat(80));
        println!("{html}");
        println!("{}", "=".repeat(80));

        Ok(())
    }
}

fn main() -> Result<(), Box<dyn Error + Send>> {
    // Initialize logging
    env_logger::init();

    info!("Starting Basic Renderer Example");
    println!("\n=== HyperChad Basic Renderer Example ===\n");

    // Create async runtime using switchy_async
    let runtime = Builder::new()
        .build()
        .map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;

    // Run async code within the runtime
    runtime.block_on(async { run_examples().await })
}

#[allow(clippy::too_many_lines)]
async fn run_examples() -> Result<(), Box<dyn Error + Send>> {
    // Create a new renderer instance
    let mut renderer = ConsoleRenderer::new();

    // Initialize the renderer with window properties
    renderer
        .init(
            1024.0,
            768.0,
            Some(100),
            Some(100),
            Some(Color::from_hex("#f0f0f0")),
            Some("Basic Renderer Demo"),
            Some("Demonstrates implementing the Renderer trait"),
            Some("width=device-width, initial-scale=1.0"),
        )
        .await?;

    // Example 1: Simple view with primary content
    info!("\n--- Example 1: Simple Primary Content ---");
    let simple_view = View::builder()
        .with_primary(Container {
            element: Element::Raw {
                value: "Hello from HyperChad!".to_string(),
            },
            ..Default::default()
        })
        .build();

    renderer.render(simple_view).await?;

    // Example 2: View with fragments (targeted updates)
    info!("\n--- Example 2: View with Fragments ---");
    let header_container = Container {
        element: Element::Raw {
            value: "Header Content".to_string(),
        },
        str_id: Some("header".to_string()),
        ..Default::default()
    };

    let footer_container = Container {
        element: Element::Raw {
            value: "Footer Content".to_string(),
        },
        str_id: Some("footer".to_string()),
        ..Default::default()
    };

    let view_with_fragments = View::builder()
        .with_primary(Container {
            element: Element::Raw {
                value: "Main Content".to_string(),
            },
            ..Default::default()
        })
        .with_fragment(ReplaceContainer::from(header_container))
        .with_fragment(ReplaceContainer::from(footer_container))
        .build();

    renderer.render(view_with_fragments).await?;

    // Example 3: Using the Content builder
    info!("\n--- Example 3: Using Content Builder ---");
    let content = Content::builder()
        .with_primary(Container {
            element: Element::Raw {
                value: "Built with Content builder".to_string(),
            },
            ..Default::default()
        })
        .build();

    if let Content::View(view) = content {
        renderer.render(*view).await?;
    }

    // Example 4: Emitting custom events
    info!("\n--- Example 4: Custom Events ---");
    renderer
        .emit_event(
            "user_action".to_string(),
            Some("button_clicked".to_string()),
        )
        .await?;
    renderer.emit_event("page_loaded".to_string(), None).await?;

    // Example 5: Adding responsive triggers
    info!("\n--- Example 5: Responsive Triggers ---");
    renderer.add_responsive_trigger(
        "mobile".to_string(),
        ResponsiveTrigger::MaxWidth(Number::from(768)),
    );
    renderer.add_responsive_trigger(
        "tablet".to_string(),
        ResponsiveTrigger::MaxWidth(Number::from(1024)),
    );

    // Example 6: Handling renderer events
    info!("\n--- Example 6: Renderer Events ---");
    let events = vec![
        RendererEvent::View(Box::new(
            View::builder()
                .with_primary(Container {
                    element: Element::Raw {
                        value: "Event-driven content".to_string(),
                    },
                    ..Default::default()
                })
                .build(),
        )),
        RendererEvent::Event {
            name: "custom_event".to_string(),
            value: Some("event_data".to_string()),
        },
    ];

    for event in events {
        match event {
            RendererEvent::View(view) => {
                info!("Processing View event");
                renderer.render(*view).await?;
            }
            RendererEvent::Event { name, value } => {
                info!("Processing custom event: {name} = {value:?}");
                renderer.emit_event(name, value).await?;
            }
        }
    }

    println!("\n=== Example completed successfully ===\n");

    Ok(())
}
