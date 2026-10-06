/* Diagnostic: print the full hyper connect error chain. Temporary tool. */
use std::time::Duration;

#[tokio::main]
async fn main() {
    let mut http = hyper_util::client::legacy::connect::HttpConnector::new();
    http.set_connect_timeout(Some(Duration::from_secs(8)));
    http.set_nodelay(true);
    // hyper-rustls passes the https URI to the inner connector for DNS+TCP;
    // a default HttpConnector enforces scheme==http and rejects it.
    http.enforce_http(false);
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .wrap_connector(http);
    let client: hyper_util::client::legacy::Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        http_body_util::Full<bytes::Bytes>,
    > = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(https);

    for url in ["https://stackoverflow.com/", "https://news.ycombinator.com/"] {
        let req = hyper::Request::builder()
            .method("GET")
            .uri(url)
            .header("user-agent", "brow-phase6/0.1")
            .body(http_body_util::Full::new(bytes::Bytes::new()))
            .unwrap();
        match tokio::time::timeout(Duration::from_secs(15), client.request(req)).await {
            Ok(Ok(resp)) => println!("{url} -> OK {} {:?}", resp.status(), resp.version()),
            Ok(Err(e)) => {
                println!("{url} -> ERR {e}");
                let mut src = std::error::Error::source(&e);
                let mut depth = 1;
                while let Some(s) = src {
                    println!("    cause({depth}): {s}");
                    src = s.source();
                    depth += 1;
                }
            }
            Err(_) => println!("{url} -> TIMEOUT"),
        }
    }
}
