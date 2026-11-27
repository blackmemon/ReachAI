//! Provides the shared authentication and security helpers used by the application.
//! This includes password hashing, secure random tokens, signed values, CSRF
//! protection, browser cookies, and the server-side user identity called `Actor`.
//! It does not contain database code. Backend repositories store sessions and
//! call these helpers to keep sensitive values such as passwords and session
//! tokens protected. The functions use server-side secrets and operating-system
//! randomness so browser-supplied values cannot be trusted without verification.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Describes who is making a request and which tenant and role they belong to.
/// A value is normally built from a server-side session, not from data supplied
/// by the browser in the request body. This keeps user identity and permissions
/// tied to information that the server has verified.
#[derive(Debug, Clone, PartialEq)]
pub struct Actor {
    /// The unique identifier for the current user. This value should come from
    /// a verified server-side session rather than a browser-controlled field.
    pub user_id: String,
    /// The tenant that owns the user's current session and data. Callers can use
    /// this value to keep one customer's records separate from another customer's.
    pub tenant_id: String,
    /// The role used to decide whether the user may perform an action. Permission
    /// checks are handled by the shared role rules instead of being hard-coded here.
    pub role: String,
    /// The user's display name, when it is available. This is presentation data
    /// and should not be used as proof of identity or authorization.
    pub name: String,
    /// The user's account email address. This identifies the account for
    /// application features but is not used as permission proof by itself.
    pub email: String,
}

impl Actor {
    /// Check whether this actor's role allows a named permission.
    /// The permission name is shared by the server and other application
    /// components, so callers can make consistent access decisions. Callers
    /// should use this check before allowing an action or returning protected data.
    pub fn can(&self, permission: &str) -> bool {
        shared::permissions::allows(&self.role, permission)
    }
}

/// Convert a password into a secure Argon2id hash that can be stored safely.
/// A fresh random salt is generated for each password. This means that two
/// accounts with the same password receive different stored values. The result
/// includes both the hashing settings and the salt, so it can be verified later
/// without storing the original password.
pub fn hash_password(password: &str) -> Result<String, String> {
    // A 16-byte random value gives each password hash a unique salt.
    let mut raw = [0u8; 16];
    OsRng.fill_bytes(&mut raw);
    let salt = SaltString::encode_b64(&raw).map_err(|e| e.to_string())?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// Check a password against a stored Argon2 password hash.
/// It returns true only when the password produces the matching hash. Invalid
/// or malformed hash text is treated as a failed verification instead of an
/// application error. This keeps normal login code from leaking details about
/// whether stored account data is valid.
pub fn verify_password(hash: &str, password: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// Perform a password verification for a user who could not be found.
/// Unknown usernames should not respond much faster than known usernames,
/// otherwise an attacker could use response timing to guess valid accounts.
/// This function runs an Argon2 check against a fixed dummy hash so the normal
/// and missing-account login paths perform similar work.
pub fn dummy_verify(password: &str) {
    // Reuse one dummy hash so the timing check is stable across login attempts.
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let h = DUMMY.get_or_init(|| hash_password("dummy-password-for-timing").unwrap_or_default());
    let _ = verify_password(h, password);
}

/// Create a new random opaque token for session or access purposes.
/// The token is generated with operating-system randomness and returned in
/// hexadecimal form so it can be carried safely in cookies, links, or headers.
/// The 32 random bytes provide enough randomness for a token that should not
/// be possible to guess in practice.
pub fn new_token() -> String {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    hex::encode(b)
}

/// Produce a one-way SHA-256 representation of a token.
/// The original token should not be stored in application data because knowing
/// it could allow someone to reuse the session or access link. Storing this
/// digest lets the application look up a token without keeping the reusable value.
pub fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

type HmacSha256 = Hmac<Sha256>;

/// Create a signed pre-login CSRF token in the form `nonce.signature`.
/// The same value is placed in a cookie and returned to the application, then
/// sent back in the `X-CSRF-Token` header. The signature lets the server reject
/// a token that was changed or created by an unauthorized website. The random
/// nonce makes each generated token distinct, even when the server secret is reused.
pub fn make_pre_login_csrf(secret: &str) -> String {
    let mut n = [0u8; 16];
    OsRng.fill_bytes(&mut n);
    let nonce = hex::encode(n);
    format!("{}.{}", nonce, sign(secret, &nonce))
}

/// Sign a text value with a server secret.
/// The signature can later prove that the value is unchanged and was created by
/// the application rather than supplied directly by an external caller.
pub fn sign_value(secret: &str, data: &str) -> String {
    sign(secret, data)
}

/// Verify that a signature belongs to the supplied text and server secret.
/// A constant-time comparison is used to avoid revealing timing differences
/// that could help an attacker test possible signature values. It regenerates
/// the expected signature from the server secret and the supplied data, then
/// checks it against the signature the caller provided.
pub fn verify_signed(secret: &str, data: &str, sig: &str) -> bool {
    constant_eq(sign(secret, data).as_bytes(), sig.as_bytes())
}

// Calculate an HMAC-SHA256 signature for internal callers. HMAC is keyed with
// the application secret, so the signature cannot be reproduced by someone who
// does not know that secret. It provides both change detection and server-origin proof.
fn sign(secret: &str, data: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Verify a pre-login CSRF token received through both the cookie and header.
/// The cookie and header values must match, and the cookie signature must be
/// valid. This check helps ensure that a state-changing request came from the
/// actual application and not from an unauthorized website. Both parts are
/// checked as untrusted input: an empty cookie, a mismatched header, or a malformed
/// token is rejected before the application continues.
pub fn verify_pre_login_csrf(secret: &str, cookie: &str, header: &str) -> bool {
    if cookie.is_empty() || !constant_eq(cookie.as_bytes(), header.as_bytes()) {
        return false;
    }
    match cookie.split_once('.') {
        Some((nonce, sig)) => constant_eq(sign(secret, nonce).as_bytes(), sig.as_bytes()),
        None => false,
    }
}

/// Compare two byte sequences without normal early-exit behavior.
/// The same-time comparison helps prevent timing attacks when secrets or
/// signatures are being checked. Different lengths are rejected because they
/// cannot match. For equal-length input, every byte is examined and the results
/// are combined instead of returning at the first difference.
pub fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // This fold combines every byte difference into one result. It avoids an
    // early return that could reveal which byte position caused a mismatch.
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Find a named cookie in a browser `Cookie` header.
/// It reads each semicolon-separated cookie pair and returns the value for the
/// requested name, or `None` when that cookie is not present. Extra spaces around
/// each pair are ignored, and malformed pairs without an equals sign are skipped.
pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    // Split once per pair so the first equals sign separates the cookie name
    // from its value and later equals signs remain part of the value.
    header
        .split(';')
        .filter_map(|p| p.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

/// Choose the session cookie name based on the site's secure-connection mode.
/// The `__Host-` prefix is only used for secure connections because that prefix
/// requires browser support for the Secure attribute. The plain name is kept for
/// local HTTP development, where secure browser cookies cannot be served.
pub fn session_cookie_name(secure: bool) -> &'static str {
    // Plain HTTP is supported for local development with the shorter plain name.
    if secure {
        "__Host-session"
    } else {
        "session"
    }
}

/// Choose the pre-login CSRF cookie name based on the secure-connection mode.
/// Secure deployments use the protected `__Host-` prefix, while local HTTP
/// development uses a plain cookie name. The returned name must match the name
/// used when the browser cookie is written so later requests can find it again.
pub fn pre_csrf_cookie_name(secure: bool) -> &'static str {
    if secure {
        "__Host-csrf"
    } else {
        "csrf"
    }
}

/// Build an HTTP cookie header for the application.
/// The cookie is restricted to the site's root path, is not readable by browser
/// JavaScript, and uses Lax same-site behavior. HttpOnly prevents script access,
/// SameSite limits cross-site sending, and Secure restricts HTTPS deployments
/// from sending the cookie over a plain HTTP connection.
pub fn set_cookie(name: &str, value: &str, secure: bool, max_age: i64) -> String {
    let mut c = format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}");
    if secure {
        c.push_str("; Secure");
    }
    c
}

/// Create a cookie header that removes a named cookie from the browser.
/// Setting an empty value with a maximum age of zero expires the cookie while
/// preserving the normal security attributes for secure deployments. Callers
/// should use the same name and secure mode that were used to create the cookie.
pub fn clear_cookie(name: &str, secure: bool) -> String {
    set_cookie(name, "", secure, 0)
}

/// Check whether a password meets the application's minimum password rules.
/// A password shorter than 12 characters or made only from ASCII digits is
/// returned with user-readable help text. The character count allows non-ASCII
/// characters to count as password characters instead of as individual bytes.
pub fn password_problem(p: &str) -> Option<&'static str> {
    if p.chars().count() < 12 {
        Some("Use at least 12 characters.")
    } else if p.chars().all(|c| c.is_ascii_digit()) {
        Some("Do not use only numbers.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    //! Exercises the authentication helpers to confirm that security checks,
    //! cookie behavior, password rules, and actor permissions remain predictable.
    use super::*;

    // Check the complete password-hash flow: successful Argon2 output, a correct
    // password, a wrong password, and malformed hash text should behave differently.
    #[test]
    fn password_round_trip() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password(&h, "correct horse battery"));
        assert!(!verify_password(&h, "wrong"));
        assert!(!verify_password("not a hash", "x"));
    }

    // Check that session tokens use a 64-character hexadecimal representation,
    // do not expose the original token through its hash, and hash consistently.
    #[test]
    fn tokens_are_long_and_hashed() {
        let t = new_token();
        assert_eq!(t.len(), 64);
        assert_ne!(t, token_hash(&t));
        assert_eq!(token_hash(&t), token_hash(&t));
    }

    // Check that the pre-login CSRF token is accepted only when the cookie and
    // header agree and the correct server secret is used to verify the signature.
    #[test]
    fn pre_login_csrf_needs_matching_cookie_and_header() {
        let t = make_pre_login_csrf("a-long-enough-secret");
        assert!(verify_pre_login_csrf("a-long-enough-secret", &t, &t));
        assert!(!verify_pre_login_csrf("a-long-enough-secret", &t, "other"));
        assert!(!verify_pre_login_csrf("another-secret-value-1", &t, &t));
        assert!(!verify_pre_login_csrf("a-long-enough-secret", "", ""));
    }

    // Check that signed values are accepted with the original secret and exact data,
    // but rejected if either value is changed or the wrong secret is supplied.
    #[test]
    fn signed_values_verify_only_with_the_right_secret_and_data() {
        let sig = sign_value("a-long-enough-secret", "unsub:abc");
        assert!(verify_signed("a-long-enough-secret", "unsub:abc", &sig));
        assert!(!verify_signed("a-long-enough-secret", "unsub:abd", &sig));
        assert!(!verify_signed("another-secret-value-1", "unsub:abc", &sig));
    }

    // Check cookie parsing and browser security flags, including the rule that
    // Secure is included only for HTTPS cookies and missing names return no value.
    #[test]
    fn cookie_parsing_and_flags() {
        assert_eq!(
            cookie_value("a=1; session=abc; b=2", "session"),
            Some("abc")
        );
        assert_eq!(cookie_value("a=1", "session"), None);
        let c = set_cookie("__Host-session", "x", true, 60);
        assert!(c.contains("HttpOnly") && c.contains("Secure") && c.contains("SameSite=Lax"));
        assert!(!set_cookie("session", "x", false, 60).contains("Secure"));
    }

    // Check the password help rules: short passwords and passwords made only of
    // digits receive a warning, while a sufficiently long mixed passphrase passes.
    #[test]
    fn password_rules() {
        assert!(password_problem("short").is_some());
        assert!(password_problem("123456789012").is_some());
        assert!(password_problem("a good long passphrase").is_none());
    }

    // Check that role-based permissions follow the shared permission rules:
    // a viewer may read reports but may not write records.
    #[test]
    fn actor_permissions_follow_role() {
        let a = Actor {
            user_id: "1".into(),
            tenant_id: "t".into(),
            role: "viewer".into(),
            name: "n".into(),
            email: "e".into(),
        };
        assert!(a.can("report.read"));
        assert!(!a.can("record.write"));
    }
}
