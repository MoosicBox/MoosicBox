#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use flume::unbounded;
use hyperchad_actions::logic::Value;
use hyperchad_renderer::{Color, Renderer, ToRenderRunner, View};
use hyperchad_renderer_fltk::FltkRenderer;
use hyperchad_template::container;
use log::{error, info};

/// Creates a simple welcome page with navigation
fn create_home_page() -> hyperchad_template::Container {
    container! {
        div
            width="100%"
            height="100%"
            direction="column"
            background="#f5f5f5"
        {
            // Header section
            header
                width="100%"
                height=60
                background="#2c3e50"
                direction="row"
                padding=10
                gap=20
                align-items="center"
            {
                h1
                    color="#ecf0f1"
                    font-size=24
                {
                    "HyperChad Desktop"
                }

                div
                    flex=1
                {}

                anchor
                    href="/about"
                    color="#3498db"
                    padding=10
                {
                    div { "About" }
                }

                anchor
                    href="/gallery"
                    color="#3498db"
                    padding=10
                {
                    div { "Gallery" }
                }
            }

            // Main content area
            main
                flex=1
                direction="column"
                padding=30
                gap=20
                overflow-y="auto"
            {
                // Hero section
                div
                    direction="column"
                    gap=15
                    align-items="center"
                    padding=40
                    background="#ffffff"
                    border-radius=8
                {
                    h1
                        font-size=32
                        color="#2c3e50"
                        text-align="center"
                    {
                        "Welcome to HyperChad FLTK!"
                    }

                    span
                        font-size=16
                        color="#7f8c8d"
                        text-align="center"
                    {
                        "A lightweight desktop GUI framework built with Rust and FLTK"
                    }
                }

                // Features section
                div
                    direction="column"
                    gap=15
                    padding=20
                {
                    h2
                        font-size=24
                        color="#34495e"
                    {
                        "Features"
                    }

                    div
                        direction="row"
                        gap=15
                        flex-wrap="wrap"
                    {
                        // Feature card 1
                        div
                            width=250
                            padding=20
                            background="#ffffff"
                            border="1px solid #e0e0e0"
                            border-radius=8
                            direction="column"
                            gap=10
                        {
                            h3
                                font-size=18
                                color="#2c3e50"
                            {
                                "🚀 Fast & Lightweight"
                            }

                            span
                                font-size=14
                                color="#7f8c8d"
                            {
                                "Built with FLTK for minimal resource usage and quick startup"
                            }
                        }

                        // Feature card 2
                        div
                            width=250
                            padding=20
                            background="#ffffff"
                            border="1px solid #e0e0e0"
                            border-radius=8
                            direction="column"
                            gap=10
                        {
                            h3
                                font-size=18
                                color="#2c3e50"
                            {
                                "🎨 Native Look"
                            }

                            span
                                font-size=14
                                color="#7f8c8d"
                            {
                                "Platform-native appearance and behavior on all systems"
                            }
                        }

                        // Feature card 3
                        div
                            width=250
                            padding=20
                            background="#ffffff"
                            border="1px solid #e0e0e0"
                            border-radius=8
                            direction="column"
                            gap=10
                        {
                            h3
                                font-size=18
                                color="#2c3e50"
                            {
                                "📱 Cross-Platform"
                            }

                            span
                                font-size=14
                                color="#7f8c8d"
                            {
                                "Works seamlessly on Windows, macOS, and Linux"
                            }
                        }
                    }
                }

                // Interactive section
                div
                    direction="column"
                    gap=15
                    padding=20
                    background="#ffffff"
                    border-radius=8
                {
                    h2
                        font-size=20
                        color="#34495e"
                    {
                        "Try Interactive Elements"
                    }

                    div
                        direction="row"
                        gap=10
                        align-items="center"
                    {
                        button
                            width=120
                            height=40
                            background="#3498db"
                            color="#ffffff"
                            padding=10
                            border-radius=4
                            fx-click=fx { request_action("show_message", "Hello from FLTK!") }
                        {
                            "Show Message"
                        }

                        button
                            width=120
                            height=40
                            background="#2ecc71"
                            color="#ffffff"
                            padding=10
                            border-radius=4
                            fx-click=fx { request_action("increment_counter", "") }
                        {
                            "Count Click"
                        }

                        div
                            str_id="counter"
                            padding=10
                            font-size=16
                            color="#2c3e50"
                        {
                            "Clicks: 0"
                        }
                    }
                }
            }

            // Footer
            footer
                width="100%"
                height=40
                background="#34495e"
                direction="row"
                justify-content="center"
                align-items="center"
            {
                span
                    color="#ecf0f1"
                    font-size=14
                {
                    "Built with ❤️ using HyperChad FLTK"
                }
            }
        }
    }
    .into()
}

/// Creates an about page
fn create_about_page() -> hyperchad_template::Container {
    container! {
        div
            width="100%"
            height="100%"
            direction="column"
            background="#f5f5f5"
        {
            // Header
            header
                width="100%"
                height=60
                background="#2c3e50"
                direction="row"
                padding=10
                gap=20
                align-items="center"
            {
                h1
                    color="#ecf0f1"
                    font-size=24
                {
                    "HyperChad Desktop"
                }

                div flex=1 {}

                anchor
                    href="/"
                    color="#3498db"
                    padding=10
                {
                    div { "Home" }
                }

                anchor
                    href="/gallery"
                    color="#3498db"
                    padding=10
                {
                    div { "Gallery" }
                }
            }

            // Main content
            main
                flex=1
                direction="column"
                padding=30
                gap=20
                overflow-y="auto"
            {
                div
                    direction="column"
                    gap=20
                    padding=30
                    background="#ffffff"
                    border-radius=8
                {
                    h1
                        font-size=28
                        color="#2c3e50"
                    {
                        "About HyperChad FLTK"
                    }

                    span
                        font-size=16
                        color="#34495e"
                        line-height=1.6
                    {
                        "HyperChad FLTK is a lightweight desktop GUI renderer that provides native desktop application capabilities using the Fast Light Toolkit (FLTK)."
                    }

                    h2
                        font-size=22
                        color="#2c3e50"
                    {
                        "Key Features"
                    }

                    ul {
                        li { "Type-safe UI component generation" }
                        li { "Flexbox-based layout system" }
                        li { "Async image loading with caching" }
                        li { "Scrollable containers and viewports" }
                        li { "Event handling and navigation" }
                        li { "Low resource usage" }
                    }

                    h2
                        font-size=22
                        color="#2c3e50"
                    {
                        "Technology Stack"
                    }

                    ul {
                        li { "Rust - Systems programming language" }
                        li { "FLTK - Fast Light Toolkit for native widgets" }
                        li { "Tokio - Async runtime for concurrency" }
                        li { "HyperChad - Type-safe UI framework" }
                    }
                }
            }

            // Footer
            footer
                width="100%"
                height=40
                background="#34495e"
                direction="row"
                justify-content="center"
                align-items="center"
            {
                span
                    color="#ecf0f1"
                    font-size=14
                {
                    "Built with ❤️ using HyperChad FLTK"
                }
            }
        }
    }
    .into()
}

/// Creates an image gallery page demonstrating async image loading
fn create_gallery_page() -> hyperchad_template::Container {
    container! {
        div
            width="100%"
            height="100%"
            direction="column"
            background="#f5f5f5"
        {
            // Header
            header
                width="100%"
                height=60
                background="#2c3e50"
                direction="row"
                padding=10
                gap=20
                align-items="center"
            {
                h1
                    color="#ecf0f1"
                    font-size=24
                {
                    "HyperChad Desktop"
                }

                div flex=1 {}

                anchor
                    href="/"
                    color="#3498db"
                    padding=10
                {
                    div { "Home" }
                }

                anchor
                    href="/about"
                    color="#3498db"
                    padding=10
                {
                    div { "About" }
                }
            }

            // Main content
            main
                flex=1
                direction="column"
                padding=30
                gap=20
                overflow-y="auto"
            {
                h1
                    font-size=28
                    color="#2c3e50"
                {
                    "Image Gallery"
                }

                span
                    font-size=14
                    color="#7f8c8d"
                {
                    "Images load asynchronously with caching"
                }

                div
                    direction="row"
                    gap=15
                    flex-wrap="wrap"
                    padding=20
                {
                    // Note: In a real app, these would be actual image URLs
                    div
                        width=200
                        height=200
                        background="#e0e0e0"
                        border-radius=8
                        direction="column"
                        justify-content="center"
                        align-items="center"
                    {
                        span
                            color="#7f8c8d"
                            font-size=14
                        {
                            "Image 1"
                        }
                    }

                    div
                        width=200
                        height=200
                        background="#e0e0e0"
                        border-radius=8
                        direction="column"
                        justify-content="center"
                        align-items="center"
                    {
                        span
                            color="#7f8c8d"
                            font-size=14
                        {
                            "Image 2"
                        }
                    }

                    div
                        width=200
                        height=200
                        background="#e0e0e0"
                        border-radius=8
                        direction="column"
                        justify-content="center"
                        align-items="center"
                    {
                        span
                            color="#7f8c8d"
                            font-size=14
                        {
                            "Image 3"
                        }
                    }
                }

                span
                    font-size=12
                    color="#95a5a6"
                    font-style="italic"
                {
                    "Note: Replace placeholder divs with img elements and actual URLs for real images"
                }
            }

            // Footer
            footer
                width="100%"
                height=40
                background="#34495e"
                direction="row"
                justify-content="center"
                align-items="center"
            {
                span
                    color="#ecf0f1"
                    font-size=14
                {
                    "Built with ❤️ using HyperChad FLTK"
                }
            }
        }
    }
    .into()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("Starting HyperChad FLTK Desktop Application Example");

    // Create action channel for handling button clicks and other events
    let (action_tx, action_rx) = unbounded::<(String, Option<Value>)>();

    // Create FLTK renderer
    let mut renderer = FltkRenderer::new(action_tx.clone());

    // Initialize the window with size, position, and styling
    renderer
        .init(
            900.0,                                  // width
            700.0,                                  // height
            Some(100),                              // x position
            Some(100),                              // y position
            Some(Color::from_hex("#f5f5f5")),       // background color
            Some("HyperChad FLTK Example"),         // window title
            Some("Basic Desktop Application Demo"), // description
            None,                                   // viewport
        )
        .await?;

    info!("Window initialized successfully");

    // Render the initial home page
    renderer.render(View::from(create_home_page())).await?;

    info!("Initial view rendered");

    // Spawn a task to handle navigation events
    let renderer_clone = renderer.clone();
    tokio::spawn(async move {
        while let Some(href) = renderer_clone.wait_for_navigation().await {
            info!("Navigation event: {}", href);

            // Handle navigation based on href
            let new_view = match href.as_str() {
                "/" => create_home_page(),
                "/about" => create_about_page(),
                "/gallery" => create_gallery_page(),
                _ => {
                    info!("Unknown route: {}, showing home page", href);
                    create_home_page()
                }
            };

            // Render the new view
            if let Err(e) = renderer_clone.render(View::from(new_view)).await {
                error!("Failed to render view for route {}: {:?}", href, e);
            }
        }
    });

    // Spawn a task to handle action events (button clicks, etc.)
    let renderer_for_actions = renderer.clone();
    let mut click_count = 0;
    tokio::spawn(async move {
        while let Ok((action_name, value)) = action_rx.recv_async().await {
            info!("Action received: {} = {:?}", action_name, value);

            match action_name.as_str() {
                "show_message" => {
                    if let Some(Value::String(message)) = value {
                        info!("Message: {}", message);
                        // In a real app, you might update the UI to show this message
                    }
                }
                "increment_counter" => {
                    click_count += 1;
                    info!("Click count: {}", click_count);
                    // In a real app, you would update the counter display
                    // For now, we'll just log it
                }
                "app_exit" => {
                    info!("Exit action received");
                    std::process::exit(0);
                }
                _ => {
                    info!("Unknown action: {}", action_name);
                }
            }
        }
    });

    // Convert renderer to runner and start the FLTK event loop
    // This will block until the window is closed
    info!("Starting FLTK event loop");
    let runner = renderer.to_runner(hyperchad_renderer::Handle::current())?;
    runner.run()?;

    info!("Application exited");

    Ok(())
}
