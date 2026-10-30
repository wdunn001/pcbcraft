use pcbcraft_library::http_client::{HttpRequest, HttpResponse, RecordedHttp};
use pcbcraft_library::rate_limit::{FixedClock, TokenBucket, retry_after_secs};
use pcbcraft_library::sources::{PartResult, PartSource, ResponseCache, SourceError};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn bucket_refuses_when_empty_and_reports_the_wait() {
    let mut bucket = TokenBucket::new(1, 60, Box::new(FixedClock::new(0)));
    assert!(bucket.try_take().is_ok());
    let wait = bucket.try_take().expect_err("second call waits");
    assert!(wait >= 1);
}

#[test]
fn retry_after_header_empties_the_bucket() {
    assert_eq!(retry_after_secs("12", 100), Some(12));
}

#[test]
fn cached_answer_makes_no_http_call() -> Result<(), SourceError> {
    struct Counting {
        http: Arc<RecordedHttp>,
        hits: AtomicU64,
    }
    impl PartSource for Counting {
        fn id(&self) -> &str {
            "counting-http"
        }
        fn name(&self) -> &str {
            "Counting"
        }
        fn search(&self, _q: &str) -> Result<Vec<PartResult>, SourceError> {
            self.hits.fetch_add(1, Ordering::SeqCst);
            let _ = self.http.call(&HttpRequest {
                method: "GET".into(),
                url: "https://example.invalid/".into(),
                headers: Default::default(),
                body: Vec::new(),
            })?;
            Ok(vec![])
        }
        fn fetch(&self, m: &str) -> Result<PartResult, SourceError> {
            Err(SourceError::NotFound(m.into()))
        }
        fn setup(&self) -> pcbcraft_library::sources::SetupCard {
            pcbcraft_library::sources::SetupCard {
                steps: vec![],
                portal_url: String::new(),
                fields: vec![],
            }
        }
        fn attribution(&self) -> pcbcraft_library::sources::Attribution {
            pcbcraft_library::sources::Attribution {
                text: String::new(),
                url: String::new(),
            }
        }
    }
    let http = Arc::new(RecordedHttp::new(vec![(
        HttpRequest {
            method: "GET".into(),
            url: "https://example.invalid/".into(),
            headers: Default::default(),
            body: Vec::new(),
        },
        HttpResponse {
            status: 200,
            headers: Default::default(),
            body: b"{}".to_vec(),
        },
    )]));
    let source = Counting {
        http: Arc::clone(&http),
        hits: AtomicU64::new(0),
    };
    let mut cache = ResponseCache::default();
    cache.search(&source, "abc", 1000)?;
    cache.search(&source, "abc", 1005)?;
    assert_eq!(source.hits.load(Ordering::SeqCst), 1);
    assert_eq!(http.calls_made(), 1);
    Ok(())
}
