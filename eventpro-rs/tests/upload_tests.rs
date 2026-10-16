mod common;

use axum::{
    extract::Request,
    http::{header, StatusCode},
    Router,
};
use common::{
    create_test_app, get_response, http_login, post_multipart_response, MultipartBuilder,
    TestUploadDir,
};
use eventpro::{
    config::{resolve_public_dir, AppConfig},
    upload::{self, AllowedImageFormat, UploadError, MAX_BANNER_SIZE},
    AppState,
};
use std::path::Path;
use tower::ServiceExt;

const SAMPLE_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89,
];

fn setup_upload_app(pool: &sqlx::MySqlPool, upload_dir: &Path) -> Router {
    let public_dir = resolve_public_dir().expect("public dir");
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        port: 3001,
        database_url: "mysql://[REDACTED]".to_string(),
        session_secret: "upload_integration_test_secret_key_12345678".to_string(),
        app_env: "test".to_string(),
        public_dir: public_dir.clone(),
        upload_dir: upload_dir.to_path_buf(),
    };
    create_test_app(AppState::new(pool.clone(), config))
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

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());

    // 1. Login as organizer Alice
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // 2. POST /events/create with valid PNG banner image
    let unique_title = format!("Banner Upload Event {}", rand::random::<u32>());
    let (content_type, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", &unique_title)
        .text("description", "Testing banner uploads")
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "150")
        .text("ticket_price", "50.00")
        .text("status", "published")
        .file("banner_image", "banner.png", "image/png", SAMPLE_PNG)
        .build();

    let (status, headers, _) = post_multipart_response(
        &app,
        "/events/create",
        Some(&auth_cookie),
        &content_type,
        payload,
    )
    .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers
            .get(axum::http::header::LOCATION)
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
    let (status, _, detail_html) = get_response(&app, &format!("/events/{}", event_id), None).await;
    assert_eq!(status, StatusCode::OK);
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

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());

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

    let (status, _, resp_html) = post_multipart_response(
        &app,
        "/events/create",
        Some(&auth_cookie),
        &content_type,
        payload,
    )
    .await;

    // Must be rejected with BAD_REQUEST
    assert_eq!(status, StatusCode::BAD_REQUEST);
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

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());

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

    let (status, _, _) =
        post_multipart_response(&app, "/events/create", Some(&auth_cookie), &ctype, payload).await;
    assert_eq!(status, StatusCode::SEE_OTHER);

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

    let (status_edit, _, _) = post_multipart_response(
        &app,
        &format!("/events/{}/edit", event_id),
        Some(&auth_cookie),
        &ctype2,
        payload2,
    )
    .await;
    assert_eq!(status_edit, StatusCode::SEE_OTHER);

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

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());

    // Login as organizer Alice
    let (auth_cookie, _) = http_login(&app, "alice@events.com", "Organizer@1").await;

    let (ctype, payload) = MultipartBuilder::new()
        .text("csrf_token", "invalid_forged_csrf_token_12345")
        .text("title", "Forged CSRF Event")
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "100")
        .file("banner_image", "test.png", "image/png", SAMPLE_PNG)
        .build();

    let (status, _, _) =
        post_multipart_response(&app, "/events/create", Some(&auth_cookie), &ctype, payload).await;

    assert_eq!(status, StatusCode::FORBIDDEN);

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

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());

    // Login as organizer Alice
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // Valid image, but invalid total_seats (0 or negative)
    let (ctype, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", "Invalid Seats Event")
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "-5")
        .file("banner_image", "test.png", "image/png", SAMPLE_PNG)
        .build();

    let (status, _, _) =
        post_multipart_response(&app, "/events/create", Some(&auth_cookie), &ctype, payload).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);

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

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());

    // Login as organizer Alice
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // Submit urlencoded form as older Phase 4 tests did
    let unique_title = format!("UrlEncoded Event {}", rand::random::<u32>());
    let form_body = format!(
        "title={}&event_date=2028-12-01T10:00&total_seats=80&ticket_price=10.00&csrf_token={}",
        percent_encoding::utf8_percent_encode(&unique_title, percent_encoding::NON_ALPHANUMERIC),
        percent_encoding::utf8_percent_encode(&create_token, percent_encoding::NON_ALPHANUMERIC),
    );

    let (status, _, _) =
        common::post_form_response(&app, "/events/create", Some(&auth_cookie), &form_body).await;

    assert_eq!(status, StatusCode::SEE_OTHER);

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

// ─── Phase 6.2.1 Hardening Tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_real_mysql_upload_between_2mb_and_5mb_succeeds() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // Create 3 MB valid JPEG image (> 2 MB default limit, <= 5 MB banner limit)
    let mut large_jpeg = vec![0u8; 3 * 1024 * 1024];
    large_jpeg[0..10]
        .copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46]);

    let unique_title = format!("Large 3MB Banner Event {}", rand::random::<u32>());
    let (content_type, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", &unique_title)
        .text("description", "Testing 3MB banner upload")
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "100")
        .text("ticket_price", "25.00")
        .file("banner_image", "large.jpg", "image/jpeg", &large_jpeg)
        .build();

    let (status, _, _) = post_multipart_response(
        &app,
        "/events/create",
        Some(&auth_cookie),
        &content_type,
        payload,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    // Verify in DB
    let row: (i32, Option<String>) =
        sqlx::query_as("SELECT id, banner_image FROM events WHERE title = ?")
            .bind(&unique_title)
            .fetch_one(&pool)
            .await
            .unwrap();

    let banner_url = row.1.expect("banner_image must be persisted");
    let filename = banner_url.strip_prefix("/uploads/").unwrap();
    let saved_path = test_upload_dir.path().join(filename);
    assert!(saved_path.exists());
    let metadata = std::fs::metadata(&saved_path).unwrap();
    assert_eq!(metadata.len(), 3 * 1024 * 1024);

    // Cleanup
    let _ = sqlx::query("DELETE FROM events WHERE id = ?")
        .bind(row.0)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn test_real_mysql_request_exceeding_total_body_limit_rejected() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // Create 9 MB payload (exceeding 8 MB MAX_EVENT_FORM_BODY_SIZE)
    let huge_data = vec![0u8; 9 * 1024 * 1024];
    let (content_type, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", "Oversized Body Event")
        .file("banner_image", "huge.jpg", "image/jpeg", &huge_data)
        .build();

    let (status, _, resp_html) = post_multipart_response(
        &app,
        "/events/create",
        Some(&auth_cookie),
        &content_type,
        payload,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        resp_html.contains("Uploaded file exceeds the maximum allowed size of 5 MB."),
        "Must display safe size limit message when total body limit exceeded"
    );

    // Verify no files written to disk
    assert_eq!(
        std::fs::read_dir(test_upload_dir.path()).unwrap().count(),
        0
    );
}

#[tokio::test]
async fn test_other_routes_do_not_inherit_large_body_limit() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());

    // POST 2.5 MB payload to /auth/login (which has default 2 MB limit)
    let large_body = vec![b'a'; 2500 * 1024];
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(large_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn test_real_mysql_oversized_text_field_aborts_and_cleans_up() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // 70 KB string (> 64 KB MAX_TEXT_FIELD_SIZE)
    let huge_desc = "x".repeat(70 * 1024);

    let (ctype, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .file("banner_image", "test.png", "image/png", SAMPLE_PNG)
        .text("description", &huge_desc)
        .text("title", "Oversized Description Event")
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "100")
        .build();

    let (status, _, resp_html) =
        post_multipart_response(&app, "/events/create", Some(&auth_cookie), &ctype, payload).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        resp_html.contains("Form field exceeds the maximum allowed size of 64 KB."),
        "Must display safe size limit message for oversized text fields"
    );

    // Verify temporary banner was cleaned up
    assert_eq!(
        std::fs::read_dir(test_upload_dir.path()).unwrap().count(),
        0,
        "Temporary banner file must be cleaned up when text field limit exceeded"
    );
}

#[tokio::test]
async fn test_real_mysql_event_deletion_cleans_up_dynamic_banner() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());
    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // 1. Create event with dynamic banner
    let unique_title = format!("Delete Banner Event {}", rand::random::<u32>());
    let (ctype, payload) = MultipartBuilder::new()
        .text("csrf_token", &create_token)
        .text("title", &unique_title)
        .text("event_date", "2028-12-01T10:00")
        .text("total_seats", "100")
        .file("banner_image", "del.png", "image/png", SAMPLE_PNG)
        .build();

    let (status, _, _) =
        post_multipart_response(&app, "/events/create", Some(&auth_cookie), &ctype, payload).await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let row: (i32, Option<String>) =
        sqlx::query_as("SELECT id, banner_image FROM events WHERE title = ?")
            .bind(&unique_title)
            .fetch_one(&pool)
            .await
            .unwrap();
    let event_id = row.0;
    let banner_url = row.1.unwrap();
    let filename = banner_url.strip_prefix("/uploads/").unwrap();
    let file_path = test_upload_dir.path().join(filename);
    assert!(file_path.exists(), "Banner file must exist before deletion");

    // 2. Delete event
    let del_body = format!("csrf_token={}", common::urlencode(&create_token));
    let (del_status, _, _) = common::post_form_response(
        &app,
        &format!("/events/{}/delete", event_id),
        Some(&auth_cookie),
        &del_body,
    )
    .await;
    assert_eq!(del_status, StatusCode::SEE_OTHER);

    // Verify deleted in DB
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "Event must be deleted from DB");

    // Verify banner file on disk was removed
    assert!(
        !file_path.exists(),
        "Dynamic banner file must be removed on event deletion"
    );
}

#[tokio::test]
async fn test_real_mysql_event_deletion_preserves_seed_banner() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };

    let test_upload_dir = TestUploadDir::new();
    let app = setup_upload_app(&pool, test_upload_dir.path());

    // Create a dummy seed banner file in upload directory: 1 hyphen pattern
    let seed_filename = "event-1772530115548.jpg";
    let seed_path = test_upload_dir.path().join(seed_filename);
    std::fs::write(&seed_path, b"dummy seed image content").unwrap();

    let (auth_cookie, create_token) = http_login(&app, "alice@events.com", "Organizer@1").await;

    // Create event directly in DB referencing seed banner
    let res = sqlx::query(
        "INSERT INTO events (organizer_id, title, event_date, total_seats, available_seats, ticket_price, status, banner_image) \
         VALUES (2, 'Seed Banner Deletion Test', '2028-12-01 10:00:00', 100, 100, 0.00, 'draft', ?)",
    )
    .bind(format!("/uploads/{}", seed_filename))
    .execute(&pool)
    .await
    .unwrap();
    let event_id = res.last_insert_id() as i32;

    // Delete event
    let del_body = format!("csrf_token={}", common::urlencode(&create_token));
    let (del_status, _, _) = common::post_form_response(
        &app,
        &format!("/events/{}/delete", event_id),
        Some(&auth_cookie),
        &del_body,
    )
    .await;
    assert_eq!(del_status, StatusCode::SEE_OTHER);

    // Verify seed banner on disk STILL EXISTS!
    assert!(
        seed_path.exists(),
        "Seed banner file must NOT be deleted upon event deletion"
    );
}

#[tokio::test]
async fn test_uploads_serves_nosniff_header_on_success_and_error() {
    let pool = match common::get_test_pool().await {
        Some(p) => p,
        None => return,
    };
    let test_upload_dir = TestUploadDir::new();

    // 1. Write sample file
    let test_file = test_upload_dir.path().join("event-test-sample.jpg");
    std::fs::write(&test_file, b"test content").unwrap();

    let app = setup_upload_app(&pool, test_upload_dir.path());

    // GET existing file -> 200 OK + nosniff
    let (status_ok, headers_ok, _) =
        get_response(&app, "/uploads/event-test-sample.jpg", None).await;
    assert_eq!(status_ok, StatusCode::OK);
    assert_eq!(
        headers_ok
            .get("x-content-type-options")
            .map(|v| v.to_str().unwrap()),
        Some("nosniff"),
        "Successful static file response must include X-Content-Type-Options: nosniff"
    );

    // GET non-existent file -> 404 NOT_FOUND + nosniff
    let (status_err, headers_err, _) =
        get_response(&app, "/uploads/nonexistent-missing-file.jpg", None).await;
    assert_eq!(status_err, StatusCode::NOT_FOUND);
    assert_eq!(
        headers_err
            .get("x-content-type-options")
            .map(|v| v.to_str().unwrap()),
        Some("nosniff"),
        "Error static file response must include X-Content-Type-Options: nosniff"
    );
}
