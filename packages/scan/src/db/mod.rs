use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{where_eq, Database, DatabaseValue};
use moosicbox_json_utils::ToValueType;

use crate::ScanOrigin;

use self::models::ScanLocation;

pub mod models;

#[cfg(feature = "local")]
pub async fn add_scan_path(db: &Box<dyn Database>, path: &str) -> Result<(), DbError> {
    db.upsert(
        "scan_locations",
        &[
            (
                "origin",
                DatabaseValue::String(ScanOrigin::Local.as_ref().to_string()),
            ),
            ("path", DatabaseValue::String(path.to_string())),
        ],
        Some(&[
            where_eq(
                "origin",
                DatabaseValue::String(ScanOrigin::Local.as_ref().to_string()),
            ),
            where_eq("path", DatabaseValue::String(path.to_string())),
        ]),
        None,
    )
    .await?;

    Ok(())
}

#[cfg(feature = "local")]
pub async fn remove_scan_path(db: &Box<dyn Database>, path: &str) -> Result<(), DbError> {
    db.delete(
        "scan_locations",
        Some(&[
            where_eq(
                "origin",
                DatabaseValue::String(ScanOrigin::Local.as_ref().to_string()),
            ),
            where_eq("path", DatabaseValue::String(path.to_string())),
        ]),
        None,
    )
    .await?;

    Ok(())
}

pub async fn enable_scan_origin(db: &Box<dyn Database>, origin: ScanOrigin) -> Result<(), DbError> {
    db.upsert(
        "scan_locations",
        &[
            ("origin", DatabaseValue::String(origin.as_ref().to_string())),
            ("path", DatabaseValue::StringOpt(None)),
        ],
        Some(&[
            where_eq("origin", DatabaseValue::String(origin.as_ref().to_string())),
            where_eq("path", DatabaseValue::StringOpt(None)),
        ]),
        None,
    )
    .await?;

    Ok(())
}

pub async fn disable_scan_origin(
    db: &Box<dyn Database>,
    origin: ScanOrigin,
) -> Result<(), DbError> {
    db.delete(
        "scan_locations",
        Some(&[
            where_eq("origin", DatabaseValue::String(origin.as_ref().to_string())),
            where_eq("path", DatabaseValue::StringOpt(None)),
        ]),
        None,
    )
    .await?;

    Ok(())
}

pub async fn get_enabled_scan_origins(db: &Box<dyn Database>) -> Result<Vec<ScanOrigin>, DbError> {
    Ok(db
        .select_distinct("scan_locations", &["origin"], None, None, None)
        .await?
        .iter()
        .map(|x| x.get("origin").unwrap().to_value_type())
        .collect::<Result<Vec<_>, _>>()?)
}

pub async fn get_scan_locations(db: &Box<dyn Database>) -> Result<Vec<ScanLocation>, DbError> {
    Ok(db
        .select("scan_locations", &["*"], None, None, None)
        .await?
        .to_value_type()?)
}
pub async fn get_scan_locations_for_origin(
    db: &Box<dyn Database>,
    origin: ScanOrigin,
) -> Result<Vec<ScanLocation>, DbError> {
    Ok(db
        .select(
            "scan_locations",
            &["*"],
            Some(&[where_eq(
                "origin",
                DatabaseValue::String(origin.as_ref().to_string()),
            )]),
            None,
            None,
        )
        .await?
        .to_value_type()?)
}
