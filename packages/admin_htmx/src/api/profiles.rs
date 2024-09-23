use actix_htmx::{Htmx, TriggerType};
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, Scope,
};
use maud::{html, Markup};
use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{
    config::ConfigDatabase,
    profiles::{api::ProfileName, PROFILES},
};
use serde::Deserialize;

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProfileQuery {
    value: Option<String>,
    bundled: Option<bool>,
}

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNewProfileForm {
    profile: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNewProfileQuery {
    bundled: Option<bool>,
}

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

pub fn new_profile_form(message: Option<String>, value: Option<String>, bundled: bool) -> Markup {
    html! {
        form hx-post={ "/admin/profiles/new?bundled="(bundled) } hx-swap="outerHTML" {
            (message.unwrap_or_default())
            input
                type="text"
                name="profile"
                placeholder="profile..."
                value={ (value.unwrap_or_else(|| if bundled { whoami::realname() } else { "".to_string() })) }
            ;
            button type="submit" { "Create" }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProfileQuery {
    profile: String,
}

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

#[route("", method = "GET")]
pub async fn list_profile_endpoint(
    _htmx: Htmx,
    db: ConfigDatabase,
) -> Result<Markup, actix_web::Error> {
    profiles(&db).await.map_err(ErrorInternalServerError)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectProfileForm {
    profile: String,
}

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
        &profiles.iter().map(|x| x.as_str()).collect::<Vec<_>>(),
        Some(form.profile.as_str()),
    ))
}

#[route("select", method = "GET")]
pub async fn select_endpoint(
    _htmx: Htmx,
    profile: Option<ProfileName>,
) -> Result<Markup, actix_web::Error> {
    let profiles = PROFILES.names();
    let profile = profile.map(|x| x.0).or_else(|| profiles.first().cloned());
    Ok(select(
        &profiles.iter().map(|x| x.as_str()).collect::<Vec<_>>(),
        profile.as_deref(),
    ))
}

pub fn select(profiles: &[&str], selected: Option<&str>) -> Markup {
    html! {
        form hx-post="/admin/profiles/select" hx-trigger="change" {
            select name="profile" {
                @for p in profiles.iter() {
                    option value=(p) selected[selected.is_some_and(|x| &x == p)] { (p) }
                }
            }
        }
    }
}

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

pub async fn profiles(db: &ConfigDatabase) -> Result<Markup, DbError> {
    let profiles = moosicbox_config::get_profiles(db).await?;

    Ok(html! {
        ul {
            @for p in &profiles {
                (profile(&p.name))
            }
        }
    })
}
