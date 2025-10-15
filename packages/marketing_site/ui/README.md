# MoosicBox Marketing Site UI

UI components and templates for the MoosicBox marketing website.

## Overview

The MoosicBox Marketing Site UI package provides:

- **Website Templates**: Complete page templates for marketing site
- **Responsive Design**: Mobile-first responsive layout components
- **HyperChad Integration**: Built with HyperChad UI framework
- **Brand Components**: Branded headers, navigation, and layout elements
- **Download Integration**: Download page components and functionality

## Components

### Page Templates

- **Home Page**: Landing page with hero section and showcase
- **Download Page**: Product download and installation pages
- **Try Now**: Free trial and demo pages
- **404 Page**: Not found error page template

### Layout Components

- **Header**: Site navigation with logo and menu items
- **Main**: Main content wrapper with flexible layout
- **Page**: Base page template with consistent styling

### Responsive Features

- **Mobile Navigation**: Responsive header and menu items
- **Adaptive Layout**: Flexible layouts that adapt to screen size
- **Image Optimization**: Responsive images with srcset support
- **Touch-Friendly**: Mobile-optimized interactions

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_marketing_site_ui = { path = "../marketing_site/ui" }
```

## Usage

### Page Templates

```rust
use moosicbox_marketing_site_ui::{home, try_now, not_found};
use moosicbox_marketing_site_ui::download;

// Generate home page
let home_page = home();

// Generate download page
let download_page = download::download();

// Generate try now page
let try_now_page = try_now();

// Generate 404 page
let not_found_page = not_found();
```

### Layout Components

```rust
use moosicbox_marketing_site_ui::{header, main, page};
use hyperchad::template::container;

// Create site header
let site_header = header();

// Create main content area
let content = container! {
    div { "Your content here" }
};
let main_content = main(&content);

// Create complete page
let complete_page = page(&content);
```

### Public Assets

```rust
use moosicbox_marketing_site_ui::public_img;

// Reference public images
let logo_src = public_img!("icon128.png");
let showcase_src = public_img!("showcase-1.webp");

// Macro expands to: "/public/img/icon128.png"
```

### Responsive Design

The components use HyperChad's responsive utilities:

```rust
use hyperchad::actions::logic::if_responsive;
use hyperchad::transformer::models::{LayoutDirection, TextAlign};

// Responsive layout direction
let direction = if_responsive("mobile-large")
    .then::<LayoutDirection>(LayoutDirection::Column)
    .or_else(LayoutDirection::Row);

// Responsive text alignment
let text_align = if_responsive("mobile-large")
    .then::<TextAlign>(TextAlign::Center)
    .or_else(TextAlign::End);
```

## Features

### Header Component

- **Logo**: MoosicBox branding with icon and text
- **Navigation**: Download, login, and trial links
- **Responsive**: Adaptive menu items for mobile
- **Styling**: Dark theme with branded colors

### Home Page

- **Hero Section**: Main value proposition and messaging
- **Showcase**: Product screenshots and demonstrations
- **Responsive Images**: Optimized images with multiple sizes
- **Call-to-Action**: Trial and download buttons

### Download Integration

- **Download Module**: Specialized download page components
- **Release Listings**: Display GitHub releases with version information
- **Asset Downloads**: Download links with file sizes and formats
- **Platform Headers**: Organized display by operating system (Windows, macOS, Linux, Android)

## Styling

### Color Scheme

- **Background**: Dark theme (#080a0b)
- **Text**: White text (#fff)
- **Accent**: Branded accent colors
- **Interactive**: Hover and focus states

### Typography

- **Font Stack**: Gordita, Roboto, system fonts
- **Hierarchy**: Consistent heading and text sizes
- **Responsive**: Adaptive font sizes for mobile

### Layout

- **Flexbox**: Modern flexbox-based layouts
- **Grid**: CSS Grid for complex layouts
- **Responsive**: Mobile-first responsive design
- **Spacing**: Consistent padding and margins

## Responsive Breakpoints

The components use these responsive breakpoints:

- **Mobile**: Small screens and phones
- **Mobile Large**: Large phones and small tablets
- **Desktop**: Tablets and desktop screens

## Dependencies

- **HyperChad**: UI framework and templating with features:
    - `actions`: Interactive behavior and logic
    - `actions-logic`: Conditional rendering logic
    - `actions-serde`: Serialization support
    - `color`: Color utilities
    - `renderer`: HTML rendering
    - `router`: Routing support
    - `template`: Template macros
    - `transformer`: Layout and styling types
- **bytesize**: File size formatting
- **chrono**: Date and time handling
- **log**: Logging support
- **regex**: Pattern matching for string formatting

## Integration

This package is designed for:

- **Marketing Websites**: Product marketing and landing pages
- **Static Site Generation**: Server-side rendered marketing sites
- **Brand Consistency**: Consistent MoosicBox branding
- **SEO Optimization**: Search engine optimized templates
- **Performance**: Fast-loading responsive components
