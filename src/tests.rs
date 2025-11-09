/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
#[cfg(all(test, feature = "lib-client"))]
pub mod tests {
    use serde_json::Value;

    use crate::tcp::client::lib_client::ClientBuilder;

    #[test]
    fn test_send_line() {
        let addr = "127.0.0.1:7000";
        let mut client = ClientBuilder::new(addr)
            .connect()
            .map_err(|e| {
                println!("Error connecting to server: {}", e);
                e
            })
            .unwrap();
        let resp = client.get_data::<Vec<Value>>(vec![("test".into(),
            "provider yahoo_finance search ticker=aapl date=2020-01-01T00:00:00Z..2025-09-01T00:00:00Z".into())]);
        println!("Response: {:?}", resp);
        assert_eq!(resp.is_ok(), true);
    }
}
