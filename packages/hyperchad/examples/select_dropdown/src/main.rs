#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # `HyperChad` Select/Dropdown Example
//!
//! This example demonstrates select/option dropdown elements in
//! [`HyperChad`](https://github.com/MoosicBox/MoosicBox/tree/master/packages/hyperchad).
//!
//! ## Features Demonstrated
//!
//! * Basic select dropdown usage
//! * Default selected value with `selected` attribute
//! * Disabled placeholder option pattern
//! * Dynamic options using `@for` iteration
//! * Change event handling with `fx-change`
//! * Styled dropdowns with `HyperChad` attributes
//! * Form integration with `name` attribute
//! * Visual feedback on selection change
//!
//! ## Running the Example
//!
//! ```bash
//! cd packages/hyperchad/examples/select_dropdown
//! PORT=3133 cargo run -- serve
//! ```
//!
//! Then open your browser to: <http://localhost:3133>
//!
//! ## Key Points
//!
//! * `<select>` creates dropdown selection menus
//! * `<option>` elements must be direct children of `<select>`
//! * `selected` attribute on select sets the currently selected value
//! * `disabled` on option prevents selection (useful for placeholders)
//! * `fx-change` triggers actions when selection changes
//! * Native HTML functionality - works in all browsers
//!
//! ## Example Usage
//!
//! Basic select:
//! ```rust,ignore
//! select name="fruit" {
//!     option value="apple" { "Apple" }
//!     option value="banana" { "Banana" }
//!     option value="orange" { "Orange" }
//! }
//! ```
//!
//! With default selection:
//! ```rust,ignore
//! select name="size" selected="medium" {
//!     option value="small" { "Small" }
//!     option value="medium" { "Medium" }
//!     option value="large" { "Large" }
//! }
//! ```
//!
//! Placeholder pattern:
//! ```rust,ignore
//! select name="country" selected="" {
//!     option value="" disabled { "-- Select a country --" }
//!     option value="us" { "United States" }
//!     option value="uk" { "United Kingdom" }
//! }
//! ```

#[allow(unused_imports)]
use hyperchad::template as hyperchad_template;
use hyperchad::{
    app::AppBuilder,
    renderer::View,
    router::{RouteRequest, Router},
    template::{Containers, container},
};
use log::info;

#[cfg(feature = "assets")]
use std::sync::LazyLock;

/// Static assets served by the application.
///
/// Contains the vanilla JavaScript runtime required for `HyperChad`'s client-side interactivity.
#[cfg(feature = "assets")]
static ASSETS: LazyLock<Vec<hyperchad::renderer::assets::StaticAssetRoute>> = LazyLock::new(|| {
    vec![
        #[cfg(feature = "vanilla-js")]
        hyperchad::renderer::assets::StaticAssetRoute {
            route: format!(
                "js/{}",
                hyperchad::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()
            ),
            target: hyperchad::renderer::assets::AssetPathTarget::FileContents(
                hyperchad::renderer_vanilla_js::SCRIPT.as_bytes().into(),
            ),
        },
    ]
});

/// Creates a section demonstrating basic select usage.
///
/// Shows a simple dropdown with a few static options and no default selection.
#[must_use]
fn create_basic_select_section() -> Containers {
    container! {
        section
            class="basic-section"
            padding=24
            background=white
            border-radius=8
            gap=16
        {
            h2 { "Basic Select" }
            span color=#6b7280 { "Simple dropdown with static options" }

            div margin-top=12 gap=8 {
                span font-weight=bold color=#374151 { "Choose a fruit:" }
                select name="fruit" {
                    option value="apple" { "Apple" }
                    option value="banana" { "Banana" }
                    option value="orange" { "Orange" }
                    option value="grape" { "Grape" }
                    option value="mango" { "Mango" }
                }
            }
        }
    }
}

/// Creates a section demonstrating default selected value.
///
/// Shows how to use the `selected` attribute to pre-select an option.
#[must_use]
fn create_default_selected_section() -> Containers {
    container! {
        section
            class="default-selected-section"
            padding=24
            background=white
            border-radius=8
            gap=16
        {
            h2 { "Default Selected Value" }
            span color=#6b7280 { "Using `selected` attribute to pre-select an option" }

            div margin-top=12 gap=8 {
                span font-weight=bold color=#374151 { "Select size (Medium is pre-selected):" }
                select name="size" selected="medium" {
                    option value="xs" { "Extra Small" }
                    option value="small" { "Small" }
                    option value="medium" { "Medium" }
                    option value="large" { "Large" }
                    option value="xl" { "Extra Large" }
                }
            }
        }
    }
}

/// Creates a section demonstrating the disabled placeholder pattern.
///
/// Shows how to use a disabled option as placeholder text that cannot be selected.
#[must_use]
fn create_placeholder_section() -> Containers {
    container! {
        section
            class="placeholder-section"
            padding=24
            background=white
            border-radius=8
            gap=16
        {
            h2 { "Placeholder Pattern" }
            span color=#6b7280 { "Using disabled option as placeholder text" }

            div margin-top=12 gap=8 {
                span font-weight=bold color=#374151 { "Select a country:" }
                select name="country" selected="" {
                    option value="" disabled { "-- Select a country --" }
                    option value="us" { "United States" }
                    option value="uk" { "United Kingdom" }
                    option value="ca" { "Canada" }
                    option value="au" { "Australia" }
                    option value="de" { "Germany" }
                    option value="fr" { "France" }
                    option value="jp" { "Japan" }
                }
            }

            div
                padding=12
                background="#fef3c7"
                border-radius=6
                margin-top=8
            {
                span color="#92400e" {
                    "The placeholder option has `disabled` attribute, so it cannot be re-selected after choosing another option."
                }
            }
        }
    }
}

/// Creates a section demonstrating dynamic options with `@for`.
///
/// Shows how to generate options from a `Vec` using template iteration.
#[must_use]
fn create_dynamic_options_section() -> Containers {
    let colors = vec![
        ("red", "Red"),
        ("green", "Green"),
        ("blue", "Blue"),
        ("yellow", "Yellow"),
        ("purple", "Purple"),
        ("orange", "Orange"),
        ("pink", "Pink"),
        ("cyan", "Cyan"),
    ];

    container! {
        section
            class="dynamic-section"
            padding=24
            background=white
            border-radius=8
            gap=16
        {
            h2 { "Dynamic Options" }
            span color=#6b7280 { "Generating options from data using @for" }

            div margin-top=12 gap=8 {
                span font-weight=bold color=#374151 { "Select a color (Blue is default):" }
                select name="color" selected="blue" {
                    @for (value, label) in &colors {
                        option value=(value) { (label) }
                    }
                }
            }

            div
                padding=12
                background="#f3f4f6"
                border-radius=6
                margin-top=8
                font-family=monospace
                font-size=14
            {
                div { "let colors = vec![" }
                div padding-left=16 { "(\"red\", \"Red\")," }
                div padding-left=16 { "(\"green\", \"Green\")," }
                div padding-left=16 { "// ..." }
                div { "];" }
                div margin-top=8 { "@for (value, label) in &colors {" }
                div padding-left=16 { "option value=(value) { (label) }" }
                div { "}" }
            }
        }
    }
}

/// Creates the animal selection display component.
///
/// This is returned as a partial update when the selection changes.
#[must_use]
fn create_animal_display(animal: &str) -> Containers {
    container! {
        div
            id="animal-display"
            padding=16
            background="#eff6ff"
            border-radius=8
            color="#1e40af"
            font-weight=bold
            text-align=center
            font-size=18
        {
            span font-size=32 {
                @match animal {
                    "dog" => "🐕",
                    "cat" => "🐈",
                    "bird" => "🐦",
                    "fish" => "🐟",
                    "rabbit" => "🐇",
                    _ => "❓",
                }
            }
            div margin-top=8 {
                @match animal {
                    "dog" => "You selected: Dog",
                    "cat" => "You selected: Cat",
                    "bird" => "You selected: Bird",
                    "fish" => "You selected: Fish",
                    "rabbit" => "You selected: Rabbit",
                    _ => "No selection yet",
                }
            }
        }
    }
}

/// Creates a section demonstrating change event handling with visual feedback.
///
/// Shows how to use `hx-get` with change events to update the UI via partial updates.
#[must_use]
fn create_change_event_section() -> Containers {
    container! {
        section
            class="change-event-section"
            padding=24
            background=white
            border-radius=8
            gap=16
        {
            h2 { "Change Event Handler" }
            span color=#6b7280 { "Using hx-get with hx-trigger=\"change\" to update UI on selection" }

            div margin-top=12 gap=16 {
                div gap=8 {
                    span font-weight=bold color=#374151 { "Select an animal:" }
                    select
                        name="animal"
                        selected=""
                        hx-get="/api/animal-display"
                        hx-trigger="change"
                        hx-target="#animal-display"
                        hx-swap="outerHTML"
                    {
                        option value="" disabled { "Choose an animal..." }
                        option value="dog" { "Dog" }
                        option value="cat" { "Cat" }
                        option value="bird" { "Bird" }
                        option value="fish" { "Fish" }
                        option value="rabbit" { "Rabbit" }
                    }
                }

                (create_animal_display(""))

                div
                    padding=12
                    background="#f0fdf4"
                    border-radius=6
                {
                    span color="#15803d" {
                        "The display above updates automatically when you select an option via a partial page update!"
                    }
                }
            }
        }
    }
}

/// Creates a section demonstrating styled select dropdowns.
///
/// Shows how to apply custom styling to select elements.
#[must_use]
fn create_styled_select_section() -> Containers {
    container! {
        section
            class="styled-section"
            padding=24
            background=white
            border-radius=8
            gap=16
        {
            h2 { "Styled Selects" }
            span color=#6b7280 { "Custom appearance with HyperChad style attributes" }

            div margin-top=12 gap=16 {
                div gap=8 {
                    span font-weight=bold color=#374151 { "Primary Style:" }
                    select
                        name="priority"
                        selected="medium"
                        padding=12
                        border-radius=8
                    {
                        option value="low" { "Low Priority" }
                        option value="medium" { "Medium Priority" }
                        option value="high" { "High Priority" }
                        option value="urgent" { "Urgent" }
                    }
                }

                div gap=8 {
                    span font-weight=bold color=#374151 { "Status Selector:" }
                    select
                        name="status"
                        selected="active"
                        padding=12
                        border-radius=8
                    {
                        option value="active" { "Active" }
                        option value="pending" { "Pending" }
                        option value="completed" { "Completed" }
                        option value="archived" { "Archived" }
                    }
                }

                div gap=8 {
                    span font-weight=bold color=#374151 { "Category:" }
                    select
                        name="category"
                        selected=""
                        padding=12
                        border-radius=8
                    {
                        option value="" disabled { "-- Select Category --" }
                        option value="work" { "Work" }
                        option value="personal" { "Personal" }
                        option value="shopping" { "Shopping" }
                        option value="health" { "Health" }
                    }
                }
            }
        }
    }
}

/// Creates a section demonstrating form integration.
///
/// Shows how select elements work within a form with proper `name` attributes.
#[must_use]
fn create_form_section() -> Containers {
    container! {
        section
            class="form-section"
            padding=24
            background=white
            border-radius=8
            gap=16
        {
            h2 { "Form Integration" }
            span color=#6b7280 { "Select elements within a form with proper name attributes" }

            form
                margin-top=12
                gap=16
                padding=20
                background="#f9fafb"
                border-radius=8
            {
                h3 color=#374151 { "Product Order Form" }

                div gap=8 {
                    span font-weight=bold color=#374151 { "Product Name:" }
                    input
                        type=text
                        name="product"
                        placeholder="Enter product name"
                        padding=10
                        border-radius=6
                        width=100%;
                }

                div gap=8 {
                    span font-weight=bold color=#374151 { "Category:" }
                    select name="category" selected="" {
                        option value="" disabled { "Select category..." }
                        option value="electronics" { "Electronics" }
                        option value="clothing" { "Clothing" }
                        option value="books" { "Books" }
                        option value="home" { "Home & Garden" }
                        option value="sports" { "Sports & Outdoors" }
                    }
                }

                div direction=row gap=16 {
                    div gap=8 flex=1 {
                        span font-weight=bold color=#374151 { "Quantity:" }
                        select name="quantity" selected="1" {
                            option value="1" { "1" }
                            option value="2" { "2" }
                            option value="3" { "3" }
                            option value="5" { "5" }
                            option value="10" { "10" }
                            option value="25" { "25" }
                        }
                    }

                    div gap=8 flex=1 {
                        span font-weight=bold color=#374151 { "Shipping:" }
                        select name="shipping" selected="standard" {
                            option value="standard" { "Standard (5-7 days)" }
                            option value="express" { "Express (2-3 days)" }
                            option value="overnight" { "Overnight" }
                        }
                    }
                }

                button
                    type=submit
                    padding-y=12
                    padding-x=24
                    background="#3b82f6"
                    color=white
                    border-radius=6
                    cursor=pointer
                    margin-top=8
                {
                    "Submit Order"
                }
            }
        }
    }
}

/// Creates an info section with documentation about select/option elements.
///
/// Provides a reference for the key attributes and usage patterns.
#[must_use]
fn create_info_section() -> Containers {
    container! {
        section
            class="info-section"
            padding=24
            background="#eff6ff"
            border-radius=8
            gap=16
        {
            h3 { "Select/Option Element Info" }
            ul padding-left=20 gap=8 {
                li {
                    span font-weight=bold { "<select>" }
                    " creates dropdown selection menus"
                }
                li {
                    span font-weight=bold { "<option>" }
                    " defines individual choices within a select"
                }
                li {
                    span font-weight=bold { "selected" }
                    " attribute on select sets the currently selected value"
                }
                li {
                    span font-weight=bold { "value" }
                    " attribute on option defines the form submission value"
                }
                li {
                    span font-weight=bold { "disabled" }
                    " on option prevents selection (great for placeholders)"
                }
                li {
                    span font-weight=bold { "name" }
                    " on select identifies the field in form submissions"
                }
                li {
                    span font-weight=bold { "hx-trigger=\"change\"" }
                    " triggers HTTP requests when selection changes"
                }
                li {
                    "<option> must be a direct child of <select>"
                }
                li {
                    "Native HTML - works in all browsers without JavaScript"
                }
            }

            h3 margin-top=16 { "HTMX Integration" }
            ul padding-left=20 gap=8 {
                li {
                    span font-weight=bold font-family=monospace { "hx-get" }
                    " - URL to fetch when triggered"
                }
                li {
                    span font-weight=bold font-family=monospace { "hx-trigger=\"change\"" }
                    " - Trigger on selection change"
                }
                li {
                    span font-weight=bold font-family=monospace { "hx-target" }
                    " - Element to update with response"
                }
                li {
                    span font-weight=bold font-family=monospace { "hx-swap" }
                    " - How to swap content (innerHTML, outerHTML, etc.)"
                }
            }
        }
    }
}

/// Creates the main page demonstrating all select/option features.
///
/// Builds a comprehensive demonstration page showcasing various use cases for
/// `<select>` and `<option>` elements in `HyperChad`.
///
/// Returns a [`Containers`] instance containing the complete page structure.
#[must_use]
#[allow(clippy::too_many_lines)]
fn create_main_page() -> Containers {
    container! {
        div class="page" {
            header
                class="header"
                padding=24
                background=#1f2937
                color=white
                text-align=center
            {
                h1 { "HyperChad Select/Dropdown Demo" }
                span { "Demonstrating dropdown menus with <select> and <option>" }
            }

            div
                direction=row
                justify-content=center
                width=100%
            {
                main
                    class="main"
                    padding=24
                    max-width=800
                    width=100%
                    gap=32
                {
                    (create_basic_select_section())
                    (create_default_selected_section())
                    (create_placeholder_section())
                    (create_dynamic_options_section())
                    (create_change_event_section())
                    (create_styled_select_section())
                    (create_form_section())
                    (create_info_section())
                }
            }

            footer
                class="footer"
                padding=24
                text-align=center
                background=#f3f4f6
            {
                span { "Built with HyperChad - Select/Dropdown Demo" }
            }
        }
    }
}

/// Creates and configures the application router.
///
/// Sets up routes for the main page and the animal display partial update endpoint.
///
/// Returns a configured [`Router`] instance ready to handle requests.
#[must_use]
fn create_router() -> Router {
    Router::new()
        .with_route("/", |_req: RouteRequest| async move {
            View::builder().with_primary(create_main_page()).build()
        })
        .with_route("/api/animal-display", |req: RouteRequest| async move {
            // Get the selected animal from query params
            let animal = req.query.get("animal").map_or("", String::as_str);

            View::builder()
                .with_primary(create_animal_display(animal))
                .build()
        })
}

/// Application entry point.
///
/// Initializes logging, creates the async runtime, sets up the router with the demonstration
/// page, configures static assets (if enabled), and starts the web server.
///
/// The server listens on the port specified by the `PORT` environment variable, or defaults
/// to port 8080 if not set.
///
/// # Errors
///
/// Returns an error if:
/// * The async runtime fails to initialize
/// * Static asset routes are invalid or fail to register
/// * The web server fails to bind to the specified port
/// * The server encounters a fatal error during execution
fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Starting HyperChad Select/Dropdown Example");

    let runtime = switchy::unsync::runtime::Builder::new().build()?;

    let router = create_router();

    info!("Server running on http://localhost:8080");
    info!("Press Ctrl+C to stop");

    #[allow(unused_mut)]
    let mut app = AppBuilder::new()
        .with_router(router)
        .with_runtime_handle(runtime.handle())
        .with_title("HyperChad Select/Dropdown Demo".to_string())
        .with_description("Demonstrating select/option dropdown elements in HyperChad".to_string());

    #[cfg(feature = "assets")]
    for asset in ASSETS.iter().cloned() {
        app.static_asset_route_result(asset).unwrap();
    }

    app.build_default()?.run()?;

    Ok(())
}
