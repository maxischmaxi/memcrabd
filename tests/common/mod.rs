use tempfile::TempDir;

pub fn temp_socket_path() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.sock");
    (dir, path)
}
