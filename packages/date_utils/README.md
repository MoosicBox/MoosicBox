# MoosicBox Date Utilities

Date and time parsing utilities with flexible format support.

## Overview

The MoosicBox Date Utilities package provides:

- **Flexible Date Parsing**: Parse various date and time formats
- **Chrono Integration**: Built on the Chrono date/time library
- **Format Detection**: Automatic format detection and parsing
- **Error Handling**: Comprehensive parsing error management

## Features

### Date Format Support

- **Year Only**: "2023" → NaiveDateTime
- **Date Only**: "2023-12-25" → NaiveDateTime
- **ISO 8601**: "2023-12-25T15:30:45Z" → NaiveDateTime
- **With Timezone**: "2023-12-25T15:30:45+00:00" → NaiveDateTime
- **With Microseconds**: "2023-12-25T15:30:45.123456" → NaiveDateTime

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_date_utils = { path = "../date_utils" }
```

The `chrono` feature is enabled by default. To use without chrono:

```toml
[dependencies]
moosicbox_date_utils = { path = "../date_utils", default-features = false }
```

## Usage

### Basic Date Parsing

```rust
use moosicbox_date_utils::chrono::parse_date_time;

// Parse various formats
let year_only = parse_date_time("2023")?;
let date_only = parse_date_time("2023-12-25")?;
let iso_format = parse_date_time("2023-12-25T15:30:45Z")?;
let with_tz = parse_date_time("2023-12-25T15:30:45+00:00")?;
let with_micros = parse_date_time("2023-12-25T15:30:45.123456")?;
```

## Dependencies

- **Chrono**: Date and time library
- **Log**: Error logging
