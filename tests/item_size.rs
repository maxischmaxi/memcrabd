use memcrabd::server::port::item_size_valid;

#[tokio::test]
async fn accepts_2m() {
    let Ok(size) = item_size_valid("2m") else {
        panic!("item size 2m should be valid");
    };

    assert_eq!(size, 2_000_000);
}
