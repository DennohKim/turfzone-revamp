use serde::Serialize;
use umbral::auth::{Authentication, Identity};
use umbral::web::{HeaderMap, IntoResponse, Json, Response, StatusCode};
use umbral_auth::BearerAuthentication;
use umbral_rest::{Action, Permission, PermissionError};

use crate::models::{UserProfile, UserRole, user_profile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePolicy {
    Player,
    Manager,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationError {
    InvalidIdentity,
    Forbidden,
    SubjectMismatch,
}

pub fn authorize(
    policy: RoutePolicy,
    identity: &Identity,
    profile_role: Option<UserRole>,
    subject_user_id: Option<i64>,
) -> Result<i64, AuthorizationError> {
    let user_id = identity
        .user_pk::<i64>()
        .map_err(|_| AuthorizationError::InvalidIdentity)?;

    if identity.is_superuser {
        return Ok(user_id);
    }

    let role = profile_role.unwrap_or(UserRole::Player);
    let has_role = match policy {
        RoutePolicy::Player => role == UserRole::Player,
        RoutePolicy::Manager => matches!(role, UserRole::Manager | UserRole::ManagerStaff),
        RoutePolicy::Admin => false,
    };

    if !has_role {
        return Err(AuthorizationError::Forbidden);
    }
    if subject_user_id.is_some_and(|subject| subject != user_id) {
        return Err(AuthorizationError::SubjectMismatch);
    }

    Ok(user_id)
}

pub async fn require_policy(
    headers: &HeaderMap,
    policy: RoutePolicy,
    subject_user_id: Option<i64>,
) -> Result<i64, Response> {
    let identity = BearerAuthentication::default()
        .authenticate(headers)
        .await
        .ok_or_else(unauthenticated_response)?;
    let user_id = identity
        .user_pk::<i64>()
        .map_err(|_| bad_identity_response())?;
    let profile_role = UserProfile::objects()
        .filter(user_profile::USER.eq(user_id))
        .first()
        .await
        .map_err(|error| {
            tracing::error!(error = %error, user_id, "authorization profile lookup failed");
            internal_error_response()
        })?
        .map(|profile| profile.role);

    authorize(policy, &identity, profile_role, subject_user_id).map_err(|error| match error {
        AuthorizationError::InvalidIdentity => bad_identity_response(),
        AuthorizationError::Forbidden => forbidden_response("insufficient role"),
        AuthorizationError::SubjectMismatch => {
            forbidden_response("cannot act on behalf of another user")
        }
    })
}

#[derive(Debug, Clone, Copy)]
pub struct StaffOrSuperuserReadOnly;

impl Permission for StaffOrSuperuserReadOnly {
    fn check(&self, action: &Action, identity: Option<&Identity>) -> Result<(), PermissionError> {
        let identity = identity.ok_or(PermissionError::Unauthenticated)?;
        if !identity.is_staff && !identity.is_superuser {
            return Err(PermissionError::Forbidden);
        }
        if !action.is_read() {
            return Err(PermissionError::Forbidden);
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct AuthorizationErrorBody<'a> {
    error: &'a str,
    code: &'a str,
}

fn authorization_response(status: StatusCode, error: &'static str, code: &'static str) -> Response {
    (status, Json(AuthorizationErrorBody { error, code })).into_response()
}

fn unauthenticated_response() -> Response {
    authorization_response(
        StatusCode::UNAUTHORIZED,
        "bearer authentication required",
        "unauthenticated",
    )
}

fn forbidden_response(error: &'static str) -> Response {
    authorization_response(StatusCode::FORBIDDEN, error, "forbidden")
}

fn bad_identity_response() -> Response {
    authorization_response(
        StatusCode::BAD_REQUEST,
        "invalid authenticated user id",
        "bad_identity",
    )
}

fn internal_error_response() -> Response {
    authorization_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "authorization check failed",
        "internal_error",
    )
}

#[cfg(test)]
mod tests {
    use umbral::auth::Identity;
    use umbral_rest::{Action, Permission, PermissionError};

    use crate::models::UserRole;

    use super::{AuthorizationError, RoutePolicy, StaffOrSuperuserReadOnly, authorize};

    #[test]
    fn new_authenticated_users_default_to_player_access() {
        let identity = Identity::user(7);

        assert_eq!(
            authorize(RoutePolicy::Player, &identity, None, Some(7)),
            Ok(7)
        );
    }

    #[test]
    fn player_cannot_act_for_another_user() {
        let identity = Identity::user(7);

        assert_eq!(
            authorize(
                RoutePolicy::Player,
                &identity,
                Some(UserRole::Player),
                Some(8),
            ),
            Err(AuthorizationError::SubjectMismatch)
        );
    }

    #[test]
    fn manager_routes_accept_managers_and_manager_staff_only() {
        let identity = Identity::user(7);

        for role in [UserRole::Manager, UserRole::ManagerStaff] {
            assert_eq!(
                authorize(RoutePolicy::Manager, &identity, Some(role), None),
                Ok(7)
            );
        }
        assert_eq!(
            authorize(
                RoutePolicy::Manager,
                &identity,
                Some(UserRole::Player),
                None,
            ),
            Err(AuthorizationError::Forbidden)
        );
    }

    #[test]
    fn admin_routes_require_the_auth_user_superuser_flag() {
        let profile_admin = Identity::user(7);
        let superuser = Identity::user(8).with_superuser(true);

        assert_eq!(
            authorize(
                RoutePolicy::Admin,
                &profile_admin,
                Some(UserRole::Admin),
                None,
            ),
            Err(AuthorizationError::Forbidden)
        );
        assert_eq!(authorize(RoutePolicy::Admin, &superuser, None, None), Ok(8));
    }

    #[test]
    fn generated_rest_is_privileged_read_only() {
        let permission = StaffOrSuperuserReadOnly;
        let user = Identity::user(1);
        let staff = Identity::user(2).with_staff(true);
        let superuser = Identity::user(3).with_superuser(true);

        assert_eq!(
            permission.check(&Action::List, None),
            Err(PermissionError::Unauthenticated)
        );
        assert_eq!(
            permission.check(&Action::List, Some(&user)),
            Err(PermissionError::Forbidden)
        );
        assert!(permission.check(&Action::List, Some(&staff)).is_ok());
        assert!(
            permission
                .check(&Action::Retrieve, Some(&superuser))
                .is_ok()
        );
        assert_eq!(
            permission.check(&Action::Create, Some(&staff)),
            Err(PermissionError::Forbidden)
        );
        assert_eq!(
            permission.check(&Action::Delete, Some(&superuser)),
            Err(PermissionError::Forbidden)
        );
    }
}
