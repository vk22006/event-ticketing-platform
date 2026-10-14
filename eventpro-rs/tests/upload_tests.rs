mod common;

use axum::{
    body::to_bytes,
    extract::Request,
    http::{header, StatusCode},
    Router,
};
use eventpro::{
    config::{resolve_public_dir, AppConfig},
    routes::create_router_with_store,
    upload::{self, AllowedImageFormat, UploadError, MAX_BANNER_SIZE},
    AppState,
};
use std::path::PathBuf;
use tower::ServiceExt;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

/// Isolated temporary upload directory guard that removes all test-created files on drop.
struct TestUploadDir {
    path: PathBuf,
}

impl TestUploadDir {
    fn new() -> Self {
        let unique_id = format!(
            "eventpro_test_uploads_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        );
        let path = std::env::temp_dir().join(unique_id);
        std::fs::create_dir_all(&path).expect("failed to create temporary upload dir");
        Self { path }
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TestUploadDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Helper to build standard HTTP multipart/form-data payloads in memory for testing.
struct MultipartBuilder {
    boundary: String,
    body: Vec<u8>,
}

impl MultipartBuilder {
    fn new() -> Self {
        let boundary = format!("----TestBoundary{}", rand::random::<u64>());
        Self {
            boundary,
            body: Vec::new(),
        }
    }

    fn text(mut self, name: &str, value: &str) -> Self {
        self.body
            .extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
        );
        self.body.extend_from_slice(value.as_bytes());
        self.body.extend_from_slice(b"\r\n");
        self
    }

    fn file(mut self, name: &str, filename: &str, mime: &str, data: &[u8]) -> Self {
        self.body
            .extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                name, filename
            )
            .as_bytes(),
        );
        self.body
            .extend_from_slice(format!("Content-Type: {}\r\n\r\n", mime).as_bytes());
        self.body.extend_from_slice(data);
        self.body.extend_from_slice(b"\r\n");
        self
    }

    fn build(mut self) -> (String, Vec<u8>) {
        self.body
            .extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        let content_type = format!("multipart/form-data; boundary={}", self.boundary);
        (content_type, self.body)
    }
}

fn create_test_app(state: AppState) -> Router {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("eventpro_sid")
        .with_secure(false)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));
    create_router_with_store(state, Some(session_layer))
}

fn extract_cookie(headers: &header::HeaderMap) -> Option<String> {
    headers.get_all(header::SET_COOKIE).iter().find_map(|v| {
        let s = v.to_str().ok()?;
        if s.starts_with("eventpro_sid=") {
            Some(s.split(';').next().unwrap().to_string())
        } else {
            None
        }
    })
}

fn extract_csrf_token(html: &str) -> Option<String> {
    let marker = "name=\"csrf_token\" value=\"";
    let start = html.find(marker)? + marker.len();
    let end = html[start..].find('"')? + start;
    Some(html[start..end].to_string())
}

async fn http_login(app: &Router, email: &str, password: &str) -> (String, String) {
    let login_page_req = Request::builder()
        .uri("/auth/login")
        .body(axum::body::Body::empty())
        .unwrap();
    let login_page_resp = app.clone().oneshot(login_page_req).await.unwrap();
    let cookie_1 = extract_cookie(login_page_resp.headers()).expect("Must receive session cookie");
    let body = to_bytes(login_page_resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let csrf_1 =
        extract_csrf_token(&String::from_utf8_lossy(&body)).expect("Must extract CSRF token");

    let form_body = format!(
        "email={}&password={}&csrf_token={}",
        percent_encoding::utf8_percent_encode(email, percent_encoding::NON_ALPHANUMERIC),
        percent_encoding::utf8_percent_encode(password, percent_encoding::NON_ALPHANUMERIC),
        percent_encoding::utf8_percent_encode(&csrf_1, percent_encoding::NON_ALPHANUMERIC),
    );
    let login_req = Request::builder()
        .method("POST")
        .uri("/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, &cookie_1)
        .body(axum::body::Body::from(form_body))
        .unwrap();

    let login_resp = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(
        login_resp.status(),
        StatusCode::SEE_OTHER,
        "Login should redirect"
    );
    let session_cookie = extract_cookie(login_resp.headers()).unwrap_or(cookie_1);

    let dash_req = Request::builder()
        .uri("/dashboard")
        .header(header::COOKIE, &session_cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let dash_resp = app.clone().oneshot(dash_req).await.unwrap();
    assert_eq!(dash_resp.status(), StatusCode::OK);
    let dash_body = to_bytes(dash_resp.into_body(), 128 * 1024).await.unwrap();
    let dash_csrf = extract_csrf_token(&String::from_utf8_lossy(&dash_body)).unwrap_or(csrf_1);

    (session_cookie, dash_csrf)
}

// ─── Unit & Format Tests ──────────────────────────────────────────────────────

#[test]
fn test_magic_bytes_detection_all_allowed_formats() {
    let valid_jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
    assert_eq!(
        AllowedImageFormat::detect(&valid_jpeg),
        Some(AllowedImageFormat::Jpeg)
    );
    assert_eq!(AllowedImageFormat::Jpeg.extension(), "jpg");
    assert_eq!(AllowedImageFormat::Jpeg.mime_type(), "image/jpeg");

    let valid_png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
    assert_eq!(
        AllowedImageFormat::detect(&valid_png),
        Some(AllowedImageFormat::Png)
    );
    assert_eq!(AllowedImageFormat::Png.extension(), "png");
    assert_eq!(AllowedImageFormat::Png.mime_type(), "image/png");

    let valid_gif87 = b"GIF87a\x01\x00\x01\x00";
    let valid_gif89 = b"GIF89a\x01\x00\x01\x00";
    assert_eq!(
        AllowedImageFormat::detect(valid_gif87),
        Some(AllowedImageFormat::Gif)
    );
    assert_eq!(
        AllowedImageFormat::detect(valid_gif89),
        Some(AllowedImageFormat::Gif)
    );
    assert_eq!(AllowedImageFormat::Gif.extension(), "gif");
    assert_eq!(AllowedImageFormat::Gif.mime_type(), "image/gif");

    let valid_webp = b"RIFF\x24\x00\x00\x00WEBPVP8 ";
    assert_eq!(
        AllowedImageFormat::detect(valid_webp),
        Some(AllowedImageFormat::WebP)
    );
    assert_eq!(AllowedImageFormat::WebP.extension(), "webp");
    assert_eq!(AllowedImageFormat::WebP.mime_type(), "image/webp");
}

#[test]
fn test_magic_bytes_rejection_disguised_or_unsupported() {
    // Malicious script masquerading as image
    let php_script = b"<?php echo 'malicious_code'; ?>";
    assert_eq!(AllowedImageFormat::detect(php_script), None);

    let html_payload = b"<!DOCTYPE html><html><body><script>alert(1)</script></body></html>";
    assert_eq!(AllowedImageFormat::detect(html_payload), None);

    let svg_xml = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><text>SVG</text></svg>";
    assert_eq!(AllowedImageFormat::detect(svg_xml), None);

    let elf_binary = b"\x7FELF\x02\x01\x01\x00\x00\x00\x00\x00";
    assert_eq!(AllowedImageFormat::detect(elf_binary), None);

    let random_junk = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
    assert_eq!(AllowedImageFormat::detect(&random_junk), None);

    let empty: [u8; 0] = [];
    assert_eq!(AllowedImageFormat::detect(&empty), None);
}

#[tokio::test]
async fn test_save_banner_upload_rejects_empty_and_oversized() {
    let test_dir = TestUploadDir::new();

    // 0 bytes -> EmptyFile
    let err = upload::save_banner_upload(test_dir.path(), &[])
        .await
        .unwrap_err();
    match err {
        UploadError::EmptyFile => (),
        other => panic!("Expected EmptyFile, got {:?}", other),
    }

    // > 5 MB -> FileTooLarge
    let oversized = vec![0u8; MAX_BANNER_SIZE + 1];
    let err = upload::save_banner_upload(test_dir.path(), &oversized)
        .await
        .unwrap_err();
    match err {
        UploadError::FileTooLarge => (),
        other => panic!("Expected FileTooLarge, got {:?}", other),
    }
}

#[tokio::test]
async fn test_save_banner_upload_success_and_random_filename() {
    let test_dir = TestUploadDir::new();
    let png_data = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52,
    ];

    let (rel_url1, path1) = upload::save_banner_upload(test_dir.path(), &png_data)
        .await
        .expect("save_banner_upload should succeed");
    let (rel_url2, path2) = upload::save_banner_upload(test_dir.path(), &png_data)
        .await
        .expect("save_banner_upload should succeed");

    assert_ne!(rel_url1, rel_url2, "Filenames must be uniquely generated");
    assert!(path1.exists());
    assert!(path2.exists());
    assert!(rel_url1.starts_with("/uploads/event-"));
    assert!(rel_url1.ends_with(".png"));
}

#[tokio::test]
async fn test_remove_banner_file_prevents_traversal_and_preserves_seed_assets() {
    let test_dir = TestUploadDir::new();

    // 1. Create a dummy dynamic banner file (event-{timestamp}-{rand}.png)
    let dynamic_name = "event-1728000000000-abcdef1234567890.png";
    let dynamic_path = test_dir.path().join(dynamic_name);
    tokio::fs::write(&dynamic_path, b"test").await.unwrap();
    assert!(dynamic_path.exists());

    // Safely remove it
    upload::remove_banner_file(test_dir.path(), &format!("/uploads/{}", dynamic_name)).await;
    assert!(!dynamic_path.exists(), "Dynamic file should be deleted");

    // 2. Protect seed assets: single-hyphen seed file format like event-1772530115548.jpg
    let seed_name = "event-1772530115548.jpg";
    let seed_path = test_dir.path().join(seed_name);
    tokio::fs::write(&seed_path, b"seed_asset_content")
        .await
        .unwrap();
    assert!(seed_path.exists());

    // Attempt deletion
    upload::remove_banner_file(test_dir.path(), &format!("/uploads/{}", seed_name)).await;
    assert!(
        seed_path.exists(),
        "Seed file must NOT be deleted by remove_banner_file"
    );

    // 3. Path traversal attacks
    let traversal_urls = [
        "/uploads/../secret.txt",
        "/uploads/..\\secret.txt",
        "/uploads/subdir/secret.txt",
        "/other/event-1728-abc.png",
    ];
    for url in traversal_urls {
        upload::remove_banner_file(test_dir.path(), url).await;
    }
}

// ─── Real MySQL Integration Tests ─────────────────────────────────────────────

#[tokio::test]
async fn test_real_mysql_event_creation_with_valid_banner() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    println!("REAL MYSQL DETECTED: Running valid banner upload integration test.");

    let public_dir = resolve_public_dir().expect("public dir");
    let test_upload_dir = TestUploadDir::new();

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "upload_integration_test_secret_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir: public_dir.clone(),
        upload_dir: test_upload_dir.path().clone(),
    };

    let state = AppState::new(pool.clone(), config);
    let app = create_test_app(state);

    // 1. Login as organizer Alice
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // 2. POST /events/create with valid PNG banner image
    let valid_png_bytes = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89,
    ];

    let unique_title = format!("Banner Upload Event {}", rand::random::<u32>());
    let (content_type, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", &unique_title)
        .text("description", "Testing banner uploads")
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "150")
        .text("ticket_price", "50.00")
        .text("status", "published")
        .file("banner_image", "banner.png", "image/png", &valid_png_bytes)
        .build();

    let post_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/create")
                .header(header::COOKIE, &auth_cookie)
                .header(header::CONTENT_TYPE, content_type)
                .body(axum::body::Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(post_create.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        post_create
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/dashboard"
    );

    // 3. Verify in MySQL database that banner_image was persisted
    let row: (i32, Option<String>) =
        sqlx::query_as("SELECT id, banner_image FROM events WHERE title = ?")
            .bind(&unique_title)
            .fetch_one(&pool)
            .await
            .expect("created event should exist in DB");

    let event_id = row.0;
    let banner_url = row.1.expect("banner_image should not be null");
    assert!(
        banner_url.starts_with("/uploads/event-"),
        "Relative URL must start with /uploads/event-, got: {}",
        banner_url
    );
    assert!(
        banner_url.ends_with(".png"),
        "Extension must match detected PNG format"
    );

    // 4. Verify the file actually exists in our test upload directory
    let filename = banner_url.strip_prefix("/uploads/").unwrap();
    let saved_file = test_upload_dir.path().join(filename);
    assert!(
        saved_file.exists(),
        "Saved banner image file must exist on disk"
    );

    // 5. Test GET /events/:id displays the banner image
    let get_detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/events/{}", event_id))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_detail.status(), StatusCode::OK);
    let detail_html =
        String::from_utf8_lossy(&to_bytes(get_detail.into_body(), usize::MAX).await.unwrap())
            .to_string();
    let escaped_url = banner_url.replace('/', "&#x2f;");
    assert!(
        detail_html.contains(&banner_url) || detail_html.contains(&escaped_url),
        "Event detail view must render banner image URL"
    );

    // Cleanup created event
    let _ = sqlx::query("DELETE FROM events WHERE id = ?")
        .bind(event_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn test_real_mysql_disguised_script_rejected_without_orphaning() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let public_dir = resolve_public_dir().expect("public dir");
    let test_upload_dir = TestUploadDir::new();

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "upload_integration_test_secret_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir: public_dir.clone(),
        upload_dir: test_upload_dir.path().clone(),
    };

    let state = AppState::new(pool.clone(), config);
    let app = create_test_app(state);

    // Login as organizer Alice
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // Submit disguised PHP script with .jpg extension and image/jpeg Content-Type
    let malicious_script = b"<?php system($_GET['cmd']); ?>";
    let unique_title = format!("Disguised Script Event {}", rand::random::<u32>());
    let (content_type, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", &unique_title)
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "100")
        .file(
            "banner_image",
            "innocent.jpg",
            "image/jpeg",
            malicious_script,
        )
        .build();

    let post_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/create")
                .header(header::COOKIE, &auth_cookie)
                .header(header::CONTENT_TYPE, content_type)
                .body(axum::body::Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    // Must be rejected with BAD_REQUEST
    assert_eq!(post_create.status(), StatusCode::BAD_REQUEST);
    let resp_html =
        String::from_utf8_lossy(&to_bytes(post_create.into_body(), usize::MAX).await.unwrap())
            .to_string();
    assert!(
        resp_html.contains("Invalid or unsupported image format"),
        "Must display safe user-facing validation error"
    );

    // Confirm nothing was written to database
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE title = ?")
        .bind(&unique_title)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "No event record should be created");

    // Confirm no orphaned files in upload directory
    let entries = std::fs::read_dir(test_upload_dir.path()).unwrap();
    assert_eq!(
        entries.count(),
        0,
        "No orphaned files should remain in upload directory"
    );
}

#[tokio::test]
async fn test_real_mysql_banner_replacement_lifecycle() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    println!("REAL MYSQL DETECTED: Running banner replacement & preservation test.");

    let public_dir = resolve_public_dir().expect("public dir");
    let test_upload_dir = TestUploadDir::new();

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "upload_integration_test_secret_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir: public_dir.clone(),
        upload_dir: test_upload_dir.path().clone(),
    };

    let state = AppState::new(pool.clone(), config);
    let app = create_test_app(state);

    // Login as organizer Alice
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // 1. Create event with initial PNG banner
    let initial_png = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52,
    ];
    let unique_title = format!("Replace Banner Event {}", rand::random::<u32>());
    let (ctype, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", &unique_title)
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "100")
        .file("banner_image", "init.png", "image/png", &initial_png)
        .build();

    let post_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/create")
                .header(header::COOKIE, &auth_cookie)
                .header(header::CONTENT_TYPE, ctype)
                .body(axum::body::Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post_create.status(), StatusCode::SEE_OTHER);

    let row: (i32, Option<String>) =
        sqlx::query_as("SELECT id, banner_image FROM events WHERE title = ?")
            .bind(&unique_title)
            .fetch_one(&pool)
            .await
            .unwrap();
    let event_id = row.0;
    let old_banner_url = row.1.unwrap();
    let old_filename = old_banner_url.strip_prefix("/uploads/").unwrap();
    let old_file_path = test_upload_dir.path().join(old_filename);
    assert!(old_file_path.exists(), "Initial banner file must exist");

    // 2. Edit event and replace with JPEG banner
    let replacement_jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
    let (ctype2, payload2) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", &unique_title)
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "100")
        .file(
            "banner_image",
            "replacement.jpg",
            "image/jpeg",
            &replacement_jpeg,
        )
        .build();

    let post_edit = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/events/{}/edit", event_id))
                .header(header::COOKIE, &auth_cookie)
                .header(header::CONTENT_TYPE, ctype2)
                .body(axum::body::Body::from(payload2))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post_edit.status(), StatusCode::SEE_OTHER);

    // Verify DB updated with new JPEG path
    let new_banner_url: String = sqlx::query_scalar("SELECT banner_image FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(new_banner_url, old_banner_url);
    assert!(new_banner_url.ends_with(".jpg"));

    // Verify old file was deleted from disk
    assert!(
        !old_file_path.exists(),
        "Replaced banner file must be safely removed from disk"
    );

    // Verify new file exists on disk
    let new_filename = new_banner_url.strip_prefix("/uploads/").unwrap();
    let new_file_path = test_upload_dir.path().join(new_filename);
    assert!(new_file_path.exists(), "New banner file must exist on disk");

    // 3. Edit event AGAIN with NO file upload (missing optional banner) -> Must PRESERVE existing banner
    let (ctype3, payload3) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", &format!("{} Updated", unique_title))
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "120")
        .build();

    let post_edit2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/events/{}/edit", event_id))
                .header(header::COOKIE, &auth_cookie)
                .header(header::CONTENT_TYPE, ctype3)
                .body(axum::body::Body::from(payload3))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post_edit2.status(), StatusCode::SEE_OTHER);

    // Verify banner was PRESERVED
    let preserved_url: String = sqlx::query_scalar("SELECT banner_image FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        preserved_url, new_banner_url,
        "Banner image must be preserved when no replacement uploaded"
    );
    assert!(
        new_file_path.exists(),
        "Preserved banner file must remain on disk"
    );

    // Cleanup
    let _ = sqlx::query("DELETE FROM events WHERE id = ?")
        .bind(event_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn test_real_mysql_invalid_csrf_cleans_up_upload() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let public_dir = resolve_public_dir().expect("public dir");
    let test_upload_dir = TestUploadDir::new();

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "upload_integration_test_secret_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir: public_dir.clone(),
        upload_dir: test_upload_dir.path().clone(),
    };

    let state = AppState::new(pool.clone(), config);
    let app = create_test_app(state);

    // Login as organizer Alice
    let (auth_cookie, _) = http_login(&app, "alice@events.com", "Organizer@1").await;

    let valid_png = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52,
    ];
    let (ctype, payload) = MultipartBuilder::new()
        .text("csrf_token", "invalid_forged_csrf_token_12345")
        .text("title", "Forged CSRF Event")
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "100")
        .file("banner_image", "test.png", "image/png", &valid_png)
        .build();

    let post_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/create")
                .header(header::COOKIE, &auth_cookie)
                .header(header::CONTENT_TYPE, ctype)
                .body(axum::body::Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(post_create.status(), StatusCode::FORBIDDEN);

    // Verify upload file was cleaned up on disk
    let count = std::fs::read_dir(test_upload_dir.path()).unwrap().count();
    assert_eq!(
        count, 0,
        "Uploaded file must be removed when CSRF verification fails"
    );
}

#[tokio::test]
async fn test_real_mysql_validation_failure_cleans_up_upload() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let public_dir = resolve_public_dir().expect("public dir");
    let test_upload_dir = TestUploadDir::new();

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "upload_integration_test_secret_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir: public_dir.clone(),
        upload_dir: test_upload_dir.path().clone(),
    };

    let state = AppState::new(pool.clone(), config);
    let app = create_test_app(state);

    // Login as organizer Alice
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // Valid image, but invalid total_seats (0 or negative)
    let valid_png = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52,
    ];
    let (ctype, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", "Invalid Seats Event")
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "-5")
        .file("banner_image", "test.png", "image/png", &valid_png)
        .build();

    let post_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/create")
                .header(header::COOKIE, &auth_cookie)
                .header(header::CONTENT_TYPE, ctype)
                .body(axum::body::Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(post_create.status(), StatusCode::BAD_REQUEST);

    // Verify upload file was cleaned up on disk
    let count = std::fs::read_dir(test_upload_dir.path()).unwrap().count();
    assert_eq!(
        count, 0,
        "Uploaded file must be removed when form validation fails"
    );
}

#[tokio::test]
async fn test_existing_urlencoded_creation_backward_compatibility() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    println!("REAL MYSQL DETECTED: Running URL-encoded backward compatibility test.");

    let public_dir = resolve_public_dir().expect("public dir");
    let test_upload_dir = TestUploadDir::new();

    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "upload_integration_test_secret_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir: public_dir.clone(),
        upload_dir: test_upload_dir.path().clone(),
    };

    let state = AppState::new(pool.clone(), config);
    let app = create_test_app(state);

    // Login as organizer Alice
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // Submit urlencoded form as older Phase 4 tests did
    let unique_title = format!("UrlEncoded Event {}", rand::random::<u32>());
    let form_body = format!(
        "title={}&event_date=2028-12-01T10:00&total_seats=80&ticket_price=10.00&csrf_token={}",
        percent_encoding::utf8_percent_encode(&unique_title, percent_encoding::NON_ALPHANUMERIC),
        percent_encoding::utf8_percent_encode(&create_token, percent_encoding::NON_ALPHANUMERIC),
    );

    let post_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/create")
                .header(header::COOKIE, &auth_cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(form_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(post_create.status(), StatusCode::SEE_OTHER);

    // Verify event in DB has banner_image = NULL
    let row: (i32, Option<String>) =
        sqlx::query_as("SELECT id, banner_image FROM events WHERE title = ?")
            .bind(&unique_title)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(row.1, None, "URL-encoded event must have NULL banner_image");

    // Cleanup
    let _ = sqlx::query("DELETE FROM events WHERE id = ?")
        .bind(row.0)
        .execute(&pool)
        .await;
}
