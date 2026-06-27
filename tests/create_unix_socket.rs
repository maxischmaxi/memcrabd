mod common;

use memcrabd::server::{Listener, create_unix_listener};
use tokio::net::UnixStream;

use crate::common::temp_socket_path;

#[tokio::test]
async fn creates_socket_file_and_accepts_connection() {
    let (_dir, path) = temp_socket_path();
    let path_str = path.to_str().unwrap();

    let listener = create_unix_listener(path_str, 0o600)
        .await
        .expect("bind should succeed");

    // Socket-Datei existiert
    assert!(path.exists(), "socket file should exist on disk");

    // Listener ist wirklich ein Unix-Listener
    let Listener::Unix(unix_listener) = listener else {
        panic!("expected Unix listener");
    };

    // Client verbindet sich
    let _client = UnixStream::connect(&path).await.expect("should connect");

    // Server acceptiert
    let (server_conn, _) = unix_listener.accept().await.expect("should accept");
    assert!(server_conn.local_addr().is_ok());
}

#[tokio::test]
async fn removes_stale_socket_file_before_binding() {
    let (_dir, path) = temp_socket_path();
    let path_str = path.to_str().unwrap();

    // Simuliere eine alte, verwaiste Socket-Datei
    std::fs::write(&path, b"stale garbage").unwrap();
    assert!(path.exists());

    // Sollte trotzdem klappen – create_unix_listener entfernt sie zuerst
    let result = create_unix_listener(path_str, 0o600).await;
    assert!(result.is_ok(), "should remove stale file and rebind");
}

#[tokio::test]
async fn fails_when_directory_does_not_exist() {
    let result = create_unix_listener("/nonexistent/dir/sock.sock", 0o600).await;
    assert!(result.is_err(), "bind in nonexistent dir should fail");
}

#[tokio::test]
async fn sets_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, path) = temp_socket_path();
    let path_str = path.to_str().unwrap();

    let _ = create_unix_listener(path_str, 0o600)
        .await
        .expect("should succeed");

    let perms = std::fs::metadata(&path).unwrap().permissions().mode();

    // 0o600 = rw------- ; mode() enthält auch den file-type-Bits (S_IFSOCK = 0o140000)
    // daher: nur die unteren 12 bits (Permission-Bits) prüfen
    assert_eq!(perms & 0o777, 0o600, "socket file should have 0600 perms");
}
