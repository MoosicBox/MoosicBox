//! Profile management endpoints for the admin interface.
//!
//! Provides endpoints for creating, selecting, deleting, and listing `MoosicBox` profiles.
//! Each profile represents a separate configuration and music library.

use actix_htmx::{Htmx, TriggerType};
use actix_web::{
    Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web,
};
use maud::{Markup, html};
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_profiles::api::ProfileName;
use serde::Deserialize;
use switchy_database::{config::ConfigDatabase, profiles::PROFILES};

/// Binds profile management endpoints to the provided Actix web scope.
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope.service(
        Scope::new("profiles")
            .service(new_profile_endpoint)
            .service(create_new_profile_endpoint)
            .service(delete_profile_endpoint)
            .service(list_profile_endpoint)
            .service(post_select_endpoint)
            .service(select_endpoint),
    )
}

/// Query parameters for the new profile form endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProfileQuery {
    /// Pre-filled value for the profile name field.
    value: Option<String>,
    /// Whether the profile is bundled (affects default name behavior).
    bundled: Option<bool>,
}

/// Endpoint that renders the new profile creation form.
///
/// # Errors
///
/// This endpoint does not return errors currently.
#[route("new", method = "GET")]
pub async fn new_profile_endpoint(
    _htmx: Htmx,
    query: web::Query<NewProfileQuery>,
) -> Result<Markup, actix_web::Error> {
    Ok(new_profile_form(
        None,
        query.value.clone(),
        query.bundled.unwrap_or_default(),
    ))
}

/// Form data for creating a new profile.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNewProfileForm {
    /// The name of the profile to create.
    profile: String,
}

/// Query parameters for creating a new profile.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNewProfileQuery {
    /// Whether the profile is bundled.
    bundled: Option<bool>,
}

/// Endpoint that handles creating a new profile.
///
/// # Errors
///
/// This endpoint does not return errors; failures are rendered as HTML with error messages.
#[route("new", method = "POST")]
pub async fn create_new_profile_endpoint(
    htmx: Htmx,
    query: web::Query<CreateNewProfileQuery>,
    form: web::Form<CreateNewProfileForm>,
    db: ConfigDatabase,
) -> Result<Markup, actix_web::Error> {
    let result = moosicbox_config::upsert_profile(&db, &form.profile).await;

    Ok(match result {
        Ok(_) => {
            htmx.trigger_event(
                "create-moosicbox-profile".to_string(),
                Some(
                    serde_json::json!({
                        "level": "info",
                        "message": "Successfully created profile",
                        "success": true,
                        "profile": &form.profile,
                    })
                    .to_string(),
                ),
                Some(TriggerType::Standard),
            );
            htmx.trigger_event(
                "create-moosicbox-profile-success".to_string(),
                Some(
                    serde_json::json!({
                        "profile": &form.profile,
                    })
                    .to_string(),
                ),
                Some(TriggerType::Standard),
            );

            new_profile_form(None, None, query.bundled.unwrap_or_default())
        }
        Err(e) => {
            htmx.trigger_event(
                "create-moosicbox-profile".to_string(),
                Some(
                    serde_json::json!({
                        "level": "info",
                        "message": "Failed to create profile",
                        "success": false,
                        "profile": &form.profile,
                    })
                    .to_string(),
                ),
                Some(TriggerType::Standard),
            );
            htmx.trigger_event(
                "create-moosicbox-profile-failure".to_string(),
                Some(
                    serde_json::json!({
                        "profile": &form.profile,
                    })
                    .to_string(),
                ),
                Some(TriggerType::Standard),
            );

            new_profile_form(
                Some(format!("Failed to create profile: {e:?}")),
                Some(form.profile.clone()),
                query.bundled.unwrap_or_default(),
            )
        }
    })
}

/// Renders a form for creating a new profile.
#[must_use]
pub fn new_profile_form(message: Option<String>, value: Option<String>, bundled: bool) -> Markup {
    html! {
        form hx-post={ "/admin/profiles/new?bundled="(bundled) } hx-swap="outerHTML" {
            (message.unwrap_or_default())
            input
                type="text"
                name="profile"
                placeholder="profile..."
                value={ (value.unwrap_or_else(|| if bundled { whoami::realname() } else { String::new() })) }
            ;
            button type="submit" { "Create" }
        }
    }
}

/// Query parameters for deleting a profile.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProfileQuery {
    /// The name of the profile to delete.
    profile: String,
}

/// Endpoint that handles deleting a profile.
///
/// # Errors
///
/// This endpoint does not return errors; failures are rendered as HTML.
#[route("", method = "DELETE")]
pub async fn delete_profile_endpoint(
    htmx: Htmx,
    query: web::Query<DeleteProfileQuery>,
    current_profile: Option<ProfileName>,
    db: ConfigDatabase,
) -> Result<Markup, actix_web::Error> {
    let result = moosicbox_config::delete_profile(&db, &query.profile).await;

    Ok(match result {
        Ok(_) => {
            htmx.trigger_event(
                "delete-moosicbox-profile-success".to_string(),
                None,
                Some(TriggerType::Standard),
            );
            if current_profile.is_some_and(|x| x.0 == query.profile) {
                htmx.trigger_event(
                    "delete-current-moosicbox-profile-success".to_string(),
                    None,
                    Some(TriggerType::Standard),
                );
            }

            html! {}
        }
        Err(e) => {
            htmx.trigger_event(
                "delete-moosicbox-profile-failure".to_string(),
                Some(serde_json::json!({"error": e.to_string()}).to_string()),
                Some(TriggerType::Standard),
            );

            profile(&query.profile)
        }
    })
}

/// Endpoint that renders the list of all profiles.
///
/// # Errors
///
/// * If fails to fetch the profiles from the database
#[route("", method = "GET")]
pub async fn list_profile_endpoint(
    _htmx: Htmx,
    db: ConfigDatabase,
) -> Result<Markup, actix_web::Error> {
    profiles(&db).await.map_err(ErrorInternalServerError)
}

/// Form data for selecting a profile.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectProfileForm {
    /// The name of the profile to select.
    profile: String,
}

/// Endpoint that handles profile selection via POST.
///
/// # Errors
///
/// This endpoint does not return errors currently.
#[route("select", method = "POST")]
pub async fn post_select_endpoint(
    htmx: Htmx,
    form: web::Form<SelectProfileForm>,
) -> Result<Markup, actix_web::Error> {
    htmx.trigger_event(
        "select-moosicbox-profile".to_string(),
        Some(
            serde_json::json!({
                "profile": &form.profile,
            })
            .to_string(),
        ),
        Some(TriggerType::Standard),
    );

    let profiles = PROFILES.names();
    Ok(select(
        &profiles.iter().map(String::as_str).collect::<Vec<_>>(),
        Some(form.profile.as_str()),
    ))
}

/// Endpoint that renders the profile selection form.
///
/// # Errors
///
/// This endpoint does not return errors currently.
#[route("select", method = "GET")]
pub async fn select_endpoint(
    _htmx: Htmx,
    profile: Option<ProfileName>,
) -> Result<Markup, actix_web::Error> {
    let profiles = PROFILES.names();
    let profile = profile.map(|x| x.0).or_else(|| profiles.first().cloned());
    Ok(select_form(
        &profiles.iter().map(String::as_str).collect::<Vec<_>>(),
        profile.as_deref(),
        None,
    ))
}

/// Renders a profile selection form with the given profiles and selected profile.
#[must_use]
pub fn select_form(profiles: &[&str], selected: Option<&str>, trigger: Option<&str>) -> Markup {
    html! {
        form hx-post="/admin/profiles/select" hx-trigger={"change"(trigger.map(|x| format!(", {x}")).unwrap_or_default())} {
            (select(profiles, selected))
        }
    }
}

/// Renders a profile selection dropdown.
#[must_use]
pub fn select(profiles: &[&str], selected: Option<&str>) -> Markup {
    html! {
        select name="profile" {
            @for p in profiles {
                option value=(p) selected[selected.is_some_and(|x| &x == p)] { (p) }
            }
        }
    }
}

/// Renders a single profile item with delete button.
#[must_use]
pub fn profile(profile: &str) -> Markup {
    html! {
        li {
            form hx-delete="/admin/profiles" hx-target="closest li" hx-swap="outerHTML" {
                span { (profile) }
                input type="hidden" name="profile" value=(profile) {}
                button type="submit" { "Delete" }
            }
        }
    }
}

/// Renders a list of all configured profiles with delete buttons.
///
/// # Errors
///
/// * If fails to fetch the profiles from the database
pub async fn profiles(db: &ConfigDatabase) -> Result<Markup, DatabaseFetchError> {
    let profiles = moosicbox_config::get_profiles(db).await?;

    Ok(html! {
        ul {
            @for p in &profiles {
                (profile(&p.name))
            }
        }
    })
}
