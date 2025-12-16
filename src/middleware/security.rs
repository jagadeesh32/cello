//! Security headers middleware for Cello (Helmet-like).
//!
//! Provides:
//! - Content Security Policy (CSP)
//! - HTTP Strict Transport Security (HSTS)
//! - X-Frame-Options
//! - X-Content-Type-Options
//! - Referrer-Policy
//! - Permissions-Policy
//! - Cross-Origin policies

use rand::Rng;
use std::collections::HashMap;

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// Content Security Policy
// ============================================================================

/// Content Security Policy builder.
#[derive(Clone, Default)]
pub struct ContentSecurityPolicy {
    directives: HashMap<String, Vec<String>>,
    report_uri: Option<String>,
    report_only: bool,
}

impl ContentSecurityPolicy {
    /// Create new CSP builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create strict CSP preset.
    pub fn strict() -> Self {
        Self::new()
            .default_src(vec!["'self'"])
            .script_src(vec!["'self'"])
            .style_src(vec!["'self'"])
            .img_src(vec!["'self'", "data:"])
            .font_src(vec!["'self'"])
            .connect_src(vec!["'self'"])
            .frame_ancestors(vec!["'none'"])
            .base_uri(vec!["'self'"])
            .form_action(vec!["'self'"])
    }

    /// Add directive.
    pub fn directive(mut self, name: &str, values: Vec<&str>) -> Self {
        self.directives
            .insert(name.to_string(), values.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Set default-src directive.
    pub fn default_src(self, values: Vec<&str>) -> Self {
        self.directive("default-src", values)
    }

    /// Set script-src directive.
    pub fn script_src(self, values: Vec<&str>) -> Self {
        self.directive("script-src", values)
    }

    /// Set script-src with nonce support.
    pub fn script_src_nonce(self, values: Vec<&str>) -> Self {
        let mut vals: Vec<&str> = values;
        vals.push("'nonce-{nonce}'");
        self.directive("script-src", vals)
    }

    /// Set style-src directive.
    pub fn style_src(self, values: Vec<&str>) -> Self {
        self.directive("style-src", values)
    }

    /// Set style-src with nonce support.
    pub fn style_src_nonce(self, values: Vec<&str>) -> Self {
        let mut vals: Vec<&str> = values;
        vals.push("'nonce-{nonce}'");
        self.directive("style-src", vals)
    }

    /// Set img-src directive.
    pub fn img_src(self, values: Vec<&str>) -> Self {
        self.directive("img-src", values)
    }

    /// Set font-src directive.
    pub fn font_src(self, values: Vec<&str>) -> Self {
        self.directive("font-src", values)
    }

    /// Set connect-src directive.
    pub fn connect_src(self, values: Vec<&str>) -> Self {
        self.directive("connect-src", values)
    }

    /// Set media-src directive.
    pub fn media_src(self, values: Vec<&str>) -> Self {
        self.directive("media-src", values)
    }

    /// Set object-src directive.
    pub fn object_src(self, values: Vec<&str>) -> Self {
        self.directive("object-src", values)
    }

    /// Set frame-src directive.
    pub fn frame_src(self, values: Vec<&str>) -> Self {
        self.directive("frame-src", values)
    }

    /// Set child-src directive.
    pub fn child_src(self, values: Vec<&str>) -> Self {
        self.directive("child-src", values)
    }

    /// Set worker-src directive.
    pub fn worker_src(self, values: Vec<&str>) -> Self {
        self.directive("worker-src", values)
    }

    /// Set frame-ancestors directive.
    pub fn frame_ancestors(self, values: Vec<&str>) -> Self {
        self.directive("frame-ancestors", values)
    }

    /// Set base-uri directive.
    pub fn base_uri(self, values: Vec<&str>) -> Self {
        self.directive("base-uri", values)
    }

    /// Set form-action directive.
    pub fn form_action(self, values: Vec<&str>) -> Self {
        self.directive("form-action", values)
    }

    /// Set upgrade-insecure-requests directive.
    pub fn upgrade_insecure_requests(mut self) -> Self {
        self.directives
            .insert("upgrade-insecure-requests".to_string(), vec![]);
        self
    }

    /// Set block-all-mixed-content directive.
    pub fn block_all_mixed_content(mut self) -> Self {
        self.directives
            .insert("block-all-mixed-content".to_string(), vec![]);
        self
    }

    /// Set report-uri directive.
    pub fn report_uri(mut self, uri: &str) -> Self {
        self.report_uri = Some(uri.to_string());
        self
    }

    /// Enable report-only mode.
    pub fn report_only(mut self) -> Self {
        self.report_only = true;
        self
    }

    /// Generate nonce value.
    pub fn generate_nonce() -> String {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.gen();
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
    }

    /// Build CSP header value.
    pub fn build(&self, nonce: Option<&str>) -> String {
        let mut parts: Vec<String> = self
            .directives
            .iter()
            .map(|(name, values)| {
                if values.is_empty() {
                    name.clone()
                } else {
                    let vals: Vec<String> = values
                        .iter()
                        .map(|v| {
                            if let Some(n) = nonce {
                                v.replace("{nonce}", n)
                            } else {
                                v.clone()
                            }
                        })
                        .collect();
                    format!("{} {}", name, vals.join(" "))
                }
            })
            .collect();

        if let Some(ref uri) = self.report_uri {
            parts.push(format!("report-uri {}", uri));
        }

        parts.join("; ")
    }

    /// Get header name based on report-only mode.
    pub fn header_name(&self) -> &'static str {
        if self.report_only {
            "Content-Security-Policy-Report-Only"
        } else {
            "Content-Security-Policy"
        }
    }
}

// ============================================================================
// HSTS Configuration
// ============================================================================

/// HTTP Strict Transport Security configuration.
#[derive(Clone)]
pub struct HstsConfig {
    /// Max age in seconds
    pub max_age: u64,
    /// Include subdomains
    pub include_subdomains: bool,
    /// Enable HSTS preload
    pub preload: bool,
}

impl HstsConfig {
    /// Create new HSTS config with max age.
    pub fn new(max_age: u64) -> Self {
        Self {
            max_age,
            include_subdomains: false,
            preload: false,
        }
    }

    /// Create HSTS config for 1 year.
    pub fn one_year() -> Self {
        Self::new(31536000)
    }

    /// Create HSTS config for 2 years (preload requirement).
    pub fn two_years() -> Self {
        Self::new(63072000)
    }

    /// Include subdomains.
    pub fn include_subdomains(mut self) -> Self {
        self.include_subdomains = true;
        self
    }

    /// Enable preload.
    pub fn preload(mut self) -> Self {
        self.preload = true;
        self.include_subdomains = true; // Required for preload
        self
    }

    /// Build HSTS header value.
    pub fn build(&self) -> String {
        let mut value = format!("max-age={}", self.max_age);
        if self.include_subdomains {
            value.push_str("; includeSubDomains");
        }
        if self.preload {
            value.push_str("; preload");
        }
        value
    }
}

impl Default for HstsConfig {
    fn default() -> Self {
        Self::one_year()
    }
}

// ============================================================================
// X-Frame-Options
// ============================================================================

/// X-Frame-Options configuration.
#[derive(Clone)]
pub enum XFrameOptions {
    /// Prevent all framing
    Deny,
    /// Allow same origin only
    SameOrigin,
    /// Allow specific origin (deprecated in browsers)
    AllowFrom(String),
}

impl XFrameOptions {
    /// Build header value.
    pub fn build(&self) -> String {
        match self {
            XFrameOptions::Deny => "DENY".to_string(),
            XFrameOptions::SameOrigin => "SAMEORIGIN".to_string(),
            XFrameOptions::AllowFrom(origin) => format!("ALLOW-FROM {}", origin),
        }
    }
}

impl Default for XFrameOptions {
    fn default() -> Self {
        XFrameOptions::SameOrigin
    }
}

// ============================================================================
// Referrer Policy
// ============================================================================

/// Referrer-Policy configuration.
#[derive(Clone)]
pub enum ReferrerPolicy {
    NoReferrer,
    NoReferrerWhenDowngrade,
    Origin,
    OriginWhenCrossOrigin,
    SameOrigin,
    StrictOrigin,
    StrictOriginWhenCrossOrigin,
    UnsafeUrl,
}

impl ReferrerPolicy {
    /// Build header value.
    pub fn build(&self) -> &'static str {
        match self {
            ReferrerPolicy::NoReferrer => "no-referrer",
            ReferrerPolicy::NoReferrerWhenDowngrade => "no-referrer-when-downgrade",
            ReferrerPolicy::Origin => "origin",
            ReferrerPolicy::OriginWhenCrossOrigin => "origin-when-cross-origin",
            ReferrerPolicy::SameOrigin => "same-origin",
            ReferrerPolicy::StrictOrigin => "strict-origin",
            ReferrerPolicy::StrictOriginWhenCrossOrigin => "strict-origin-when-cross-origin",
            ReferrerPolicy::UnsafeUrl => "unsafe-url",
        }
    }
}

impl Default for ReferrerPolicy {
    fn default() -> Self {
        ReferrerPolicy::StrictOriginWhenCrossOrigin
    }
}

// ============================================================================
// Permissions Policy
// ============================================================================

/// Permissions-Policy builder (formerly Feature-Policy).
#[derive(Clone, Default)]
pub struct PermissionsPolicy {
    directives: HashMap<String, Vec<String>>,
}

impl PermissionsPolicy {
    /// Create new Permissions-Policy builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create restrictive preset (disables most features).
    pub fn restrictive() -> Self {
        Self::new()
            .geolocation(vec![])
            .camera(vec![])
            .microphone(vec![])
            .payment(vec![])
            .usb(vec![])
    }

    /// Set directive.
    pub fn directive(mut self, name: &str, values: Vec<&str>) -> Self {
        self.directives
            .insert(name.to_string(), values.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Set geolocation permission.
    pub fn geolocation(self, values: Vec<&str>) -> Self {
        self.directive("geolocation", values)
    }

    /// Set camera permission.
    pub fn camera(self, values: Vec<&str>) -> Self {
        self.directive("camera", values)
    }

    /// Set microphone permission.
    pub fn microphone(self, values: Vec<&str>) -> Self {
        self.directive("microphone", values)
    }

    /// Set payment permission.
    pub fn payment(self, values: Vec<&str>) -> Self {
        self.directive("payment", values)
    }

    /// Set USB permission.
    pub fn usb(self, values: Vec<&str>) -> Self {
        self.directive("usb", values)
    }

    /// Set fullscreen permission.
    pub fn fullscreen(self, values: Vec<&str>) -> Self {
        self.directive("fullscreen", values)
    }

    /// Set autoplay permission.
    pub fn autoplay(self, values: Vec<&str>) -> Self {
        self.directive("autoplay", values)
    }

    /// Set picture-in-picture permission.
    pub fn picture_in_picture(self, values: Vec<&str>) -> Self {
        self.directive("picture-in-picture", values)
    }

    /// Set accelerometer permission.
    pub fn accelerometer(self, values: Vec<&str>) -> Self {
        self.directive("accelerometer", values)
    }

    /// Set gyroscope permission.
    pub fn gyroscope(self, values: Vec<&str>) -> Self {
        self.directive("gyroscope", values)
    }

    /// Build header value.
    pub fn build(&self) -> String {
        self.directives
            .iter()
            .map(|(name, values)| {
                if values.is_empty() {
                    format!("{}=()", name)
                } else {
                    format!("{}=({})", name, values.join(" "))
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// ============================================================================
// Cross-Origin Policies
// ============================================================================

/// Cross-Origin-Embedder-Policy.
#[derive(Clone)]
pub enum CrossOriginEmbedderPolicy {
    UnsafeNone,
    RequireCorp,
    Credentialless,
}

impl CrossOriginEmbedderPolicy {
    pub fn build(&self) -> &'static str {
        match self {
            CrossOriginEmbedderPolicy::UnsafeNone => "unsafe-none",
            CrossOriginEmbedderPolicy::RequireCorp => "require-corp",
            CrossOriginEmbedderPolicy::Credentialless => "credentialless",
        }
    }
}

/// Cross-Origin-Opener-Policy.
#[derive(Clone)]
pub enum CrossOriginOpenerPolicy {
    UnsafeNone,
    SameOrigin,
    SameOriginAllowPopups,
}

impl CrossOriginOpenerPolicy {
    pub fn build(&self) -> &'static str {
        match self {
            CrossOriginOpenerPolicy::UnsafeNone => "unsafe-none",
            CrossOriginOpenerPolicy::SameOrigin => "same-origin",
            CrossOriginOpenerPolicy::SameOriginAllowPopups => "same-origin-allow-popups",
        }
    }
}

/// Cross-Origin-Resource-Policy.
#[derive(Clone)]
pub enum CrossOriginResourcePolicy {
    SameSite,
    SameOrigin,
    CrossOrigin,
}

impl CrossOriginResourcePolicy {
    pub fn build(&self) -> &'static str {
        match self {
            CrossOriginResourcePolicy::SameSite => "same-site",
            CrossOriginResourcePolicy::SameOrigin => "same-origin",
            CrossOriginResourcePolicy::CrossOrigin => "cross-origin",
        }
    }
}

// ============================================================================
// Security Headers Middleware
// ============================================================================

/// Security headers middleware configuration.
pub struct SecurityHeadersMiddleware {
    /// Content Security Policy
    pub csp: Option<ContentSecurityPolicy>,
    /// Generate nonce for CSP
    pub csp_nonce: bool,
    /// HTTP Strict Transport Security
    pub hsts: Option<HstsConfig>,
    /// X-Frame-Options
    pub x_frame_options: Option<XFrameOptions>,
    /// X-Content-Type-Options: nosniff
    pub x_content_type_options: bool,
    /// X-XSS-Protection (deprecated but still useful)
    pub x_xss_protection: bool,
    /// Referrer-Policy
    pub referrer_policy: Option<ReferrerPolicy>,
    /// Permissions-Policy
    pub permissions_policy: Option<PermissionsPolicy>,
    /// Cross-Origin-Embedder-Policy
    pub coep: Option<CrossOriginEmbedderPolicy>,
    /// Cross-Origin-Opener-Policy
    pub coop: Option<CrossOriginOpenerPolicy>,
    /// Cross-Origin-Resource-Policy
    pub corp: Option<CrossOriginResourcePolicy>,
    /// Context key for storing nonce
    pub nonce_key: String,
}

impl SecurityHeadersMiddleware {
    /// Create new security headers middleware with sensible defaults.
    pub fn new() -> Self {
        Self {
            csp: None,
            csp_nonce: false,
            hsts: Some(HstsConfig::default()),
            x_frame_options: Some(XFrameOptions::default()),
            x_content_type_options: true,
            x_xss_protection: true,
            referrer_policy: Some(ReferrerPolicy::default()),
            permissions_policy: None,
            coep: None,
            coop: None,
            corp: None,
            nonce_key: "csp_nonce".to_string(),
        }
    }

    /// Create with strict settings.
    pub fn strict() -> Self {
        Self {
            csp: Some(ContentSecurityPolicy::strict()),
            csp_nonce: true,
            hsts: Some(HstsConfig::two_years().preload()),
            x_frame_options: Some(XFrameOptions::Deny),
            x_content_type_options: true,
            x_xss_protection: true,
            referrer_policy: Some(ReferrerPolicy::NoReferrer),
            permissions_policy: Some(PermissionsPolicy::restrictive()),
            coep: Some(CrossOriginEmbedderPolicy::RequireCorp),
            coop: Some(CrossOriginOpenerPolicy::SameOrigin),
            corp: Some(CrossOriginResourcePolicy::SameOrigin),
            nonce_key: "csp_nonce".to_string(),
        }
    }

    /// Set CSP.
    pub fn csp(mut self, csp: ContentSecurityPolicy) -> Self {
        self.csp = Some(csp);
        self
    }

    /// Enable CSP nonce generation.
    pub fn with_nonce(mut self) -> Self {
        self.csp_nonce = true;
        self
    }

    /// Set HSTS.
    pub fn hsts(mut self, hsts: HstsConfig) -> Self {
        self.hsts = Some(hsts);
        self
    }

    /// Disable HSTS.
    pub fn no_hsts(mut self) -> Self {
        self.hsts = None;
        self
    }

    /// Set X-Frame-Options.
    pub fn x_frame_options(mut self, options: XFrameOptions) -> Self {
        self.x_frame_options = Some(options);
        self
    }

    /// Disable X-Frame-Options.
    pub fn no_x_frame_options(mut self) -> Self {
        self.x_frame_options = None;
        self
    }

    /// Enable/disable X-Content-Type-Options.
    pub fn x_content_type_options(mut self, enabled: bool) -> Self {
        self.x_content_type_options = enabled;
        self
    }

    /// Set Referrer-Policy.
    pub fn referrer_policy(mut self, policy: ReferrerPolicy) -> Self {
        self.referrer_policy = Some(policy);
        self
    }

    /// Set Permissions-Policy.
    pub fn permissions_policy(mut self, policy: PermissionsPolicy) -> Self {
        self.permissions_policy = Some(policy);
        self
    }

    /// Set Cross-Origin-Embedder-Policy.
    pub fn coep(mut self, policy: CrossOriginEmbedderPolicy) -> Self {
        self.coep = Some(policy);
        self
    }

    /// Set Cross-Origin-Opener-Policy.
    pub fn coop(mut self, policy: CrossOriginOpenerPolicy) -> Self {
        self.coop = Some(policy);
        self
    }

    /// Set Cross-Origin-Resource-Policy.
    pub fn corp(mut self, policy: CrossOriginResourcePolicy) -> Self {
        self.corp = Some(policy);
        self
    }

    /// Set nonce context key.
    pub fn nonce_key(mut self, key: &str) -> Self {
        self.nonce_key = key.to_string();
        self
    }
}

impl Default for SecurityHeadersMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for SecurityHeadersMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Generate and store nonce if enabled
        if self.csp_nonce && self.csp.is_some() {
            let nonce = ContentSecurityPolicy::generate_nonce();
            request
                .context
                .insert(self.nonce_key.clone(), serde_json::Value::String(nonce));
        }
        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Get nonce from context if generated
        let nonce = request
            .context
            .get(&self.nonce_key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Content-Security-Policy
        if let Some(ref csp) = self.csp {
            response.set_header(csp.header_name(), &csp.build(nonce.as_deref()));
        }

        // HSTS
        if let Some(ref hsts) = self.hsts {
            response.set_header("Strict-Transport-Security", &hsts.build());
        }

        // X-Frame-Options
        if let Some(ref xfo) = self.x_frame_options {
            response.set_header("X-Frame-Options", &xfo.build());
        }

        // X-Content-Type-Options
        if self.x_content_type_options {
            response.set_header("X-Content-Type-Options", "nosniff");
        }

        // X-XSS-Protection
        if self.x_xss_protection {
            response.set_header("X-XSS-Protection", "1; mode=block");
        }

        // Referrer-Policy
        if let Some(ref policy) = self.referrer_policy {
            response.set_header("Referrer-Policy", policy.build());
        }

        // Permissions-Policy
        if let Some(ref policy) = self.permissions_policy {
            response.set_header("Permissions-Policy", &policy.build());
        }

        // Cross-Origin-Embedder-Policy
        if let Some(ref coep) = self.coep {
            response.set_header("Cross-Origin-Embedder-Policy", coep.build());
        }

        // Cross-Origin-Opener-Policy
        if let Some(ref coop) = self.coop {
            response.set_header("Cross-Origin-Opener-Policy", coop.build());
        }

        // Cross-Origin-Resource-Policy
        if let Some(ref corp) = self.corp {
            response.set_header("Cross-Origin-Resource-Policy", corp.build());
        }

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        90 // Run late, after most processing
    }

    fn name(&self) -> &str {
        "security_headers"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csp_builder() {
        let csp = ContentSecurityPolicy::new()
            .default_src(vec!["'self'"])
            .script_src(vec!["'self'", "'unsafe-inline'"])
            .img_src(vec!["'self'", "data:", "https:"]);

        let header = csp.build(None);
        assert!(header.contains("default-src 'self'"));
        assert!(header.contains("script-src 'self' 'unsafe-inline'"));
    }

    #[test]
    fn test_csp_with_nonce() {
        let csp = ContentSecurityPolicy::new()
            .script_src_nonce(vec!["'self'"]);

        let header = csp.build(Some("abc123"));
        assert!(header.contains("'nonce-abc123'"));
    }

    #[test]
    fn test_hsts_config() {
        let hsts = HstsConfig::two_years().preload();
        assert_eq!(
            hsts.build(),
            "max-age=63072000; includeSubDomains; preload"
        );
    }

    #[test]
    fn test_permissions_policy() {
        let pp = PermissionsPolicy::new()
            .geolocation(vec!["'self'"])
            .camera(vec![]);

        let header = pp.build();
        assert!(header.contains("geolocation=('self')"));
        assert!(header.contains("camera=()"));
    }

    #[test]
    fn test_referrer_policy() {
        assert_eq!(
            ReferrerPolicy::StrictOriginWhenCrossOrigin.build(),
            "strict-origin-when-cross-origin"
        );
    }
}
