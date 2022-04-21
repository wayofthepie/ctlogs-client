pub mod entry;

use again::{self, RetryPolicy};
use anyhow::anyhow;
use async_trait::async_trait;
use ctlogs_parser::parser::Logs;
use derive_builder::Builder;
use entry::*;
use http::StatusCode;
use reqwest::{Client, RequestBuilder};
use std::time::Duration;

#[async_trait]
pub trait CtClient {
    async fn list_log_operators(&self) -> anyhow::Result<Operators>;

    async fn get_entries(&self, base_url: &str, start: usize, end: usize) -> anyhow::Result<Logs>;

    async fn get_sth(&self, base_url: &str) -> anyhow::Result<SignedTreeHead>;
}

#[derive(Clone, Builder)]
#[builder(setter(into))]
pub struct HttpCtClient<'a> {
    #[builder(default = "\"https://www.gstatic.com/ct/log_list/v2\"")]
    log_operators_base_url: &'a str,
    #[builder(default = "Client::new()")]
    client: Client,
    #[builder(default = "Duration::from_secs(20)")]
    timeout: Duration,
    #[builder(default = "RetryPolicy::fixed(Duration::from_millis(100)).with_max_retries(10)")]
    retry_policy: RetryPolicy,
}

impl HttpCtClient<'_> {
    async fn retryable_request<T>(&self, builder: RequestBuilder) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned + Send + Sync,
    {
        Ok(self
            .retry_policy
            .retry_if(
                || async {
                    builder
                        .try_clone()
                        .ok_or_else(|| anyhow!("clone error"))
                        .unwrap() // TODO: remove this and enum the error we retry on, failing on this one
                        .timeout(self.timeout)
                        .send()
                        .await
                        .and_then(|response| response.error_for_status())?
                        .json::<T>()
                        .await
                },
                |err: &reqwest::Error| {
                    matches!(err.status(), Some(StatusCode::TOO_MANY_REQUESTS))
                        || reqwest::Error::is_status(err)
                        || reqwest::Error::is_timeout(err)
                },
            )
            .await?)
    }
}

#[async_trait]
impl CtClient for HttpCtClient<'_> {
    async fn list_log_operators(&self) -> anyhow::Result<Operators> {
        let mut base_url = self.log_operators_base_url;
        if base_url.ends_with('/') {
            base_url = &base_url[0..base_url.len() - 1];
        }
        self.retryable_request(self.client.get(&format!("{}/log_list.json", base_url)))
            .await
    }

    async fn get_entries(
        &self,
        mut base_url: &str,
        start: usize,
        end: usize,
    ) -> anyhow::Result<Logs> {
        if base_url.ends_with('/') {
            base_url = &base_url[0..base_url.len() - 1];
        }
        let mut logs = self
            .retryable_request::<Logs>(
                self.client
                    .get(&format!("{}/ct/v1/get-entries", base_url))
                    .query(&[("start", start), ("end", end)]),
            )
            .await?;

        while logs.entries.len() < end - start {
            let len = logs.entries.len();
            let new_start = start + len;
            let next = self
                .retryable_request::<Logs>(
                    self.client
                        .get(&format!("{}/ct/v1/get-entries", base_url))
                        .query(&[("start", new_start), ("end", end)]),
                )
                .await?;
            logs.entries.extend(next.entries);
        }
        Ok(logs)
    }

    async fn get_sth(&self, mut base_url: &str) -> anyhow::Result<SignedTreeHead> {
        if base_url.ends_with('/') {
            base_url = &base_url[0..base_url.len() - 1];
        }
        Ok(self
            .retryable_request::<SignedTreeHead>(
                self.client.get(&format!("{}/ct/v1/get-sth", base_url)),
            )
            .await?)
    }
}

#[cfg(test)]
mod test {
    use super::{CtClient, HttpCtClientBuilder, Operators};
    use super::{HttpCtClient, Logs, Operator, SignedTreeHead};
    use again::RetryPolicy;
    use chrono::Utc;
    use ctlogs_parser::parser::LogEntry;
    use std::time::Duration;
    use wiremock::matchers::path_regex;
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    const LEAF_INPUT: &str = include_str!("../resources/test/leaf_input_with_cert");

    fn default_client(uri: &str) -> HttpCtClient {
        HttpCtClientBuilder::default()
            .log_operators_base_url(uri)
            .timeout(Duration::from_millis(10))
            .retry_policy(RetryPolicy::fixed(Duration::from_millis(1)).with_max_retries(10))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn get_num_entries_should_fail_if_api_call_fails() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-sth"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&mock_server)
            .await;
        let uri = &mock_server.uri();
        let client = default_client(uri);
        let result = client.get_sth(uri).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_num_entries_should_return_size() {
        let expected_size: usize = 12;
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-sth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(SignedTreeHead {
                tree_size: expected_size,
                timestamp: Utc::now(),
            }))
            .mount(&mock_server)
            .await;
        let uri = &mock_server.uri();
        let client = default_client(uri);
        let result = client.get_sth(uri).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().tree_size, expected_size);
    }

    #[tokio::test]
    async fn get_entries_should_fail_if_log_retrieval_fails() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/v1/get-entries"))
            .and(query_param("start", "0"))
            .and(query_param("end", "1"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&mock_server)
            .await;
        let uri = &mock_server.uri();
        let client = default_client(uri);
        let result = client.get_entries(uri, 0, 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_entries_should_fail_if_body_is_not_an_expected_value() {
        let body: Vec<u32> = vec![0, 0];
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-entries"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&mock_server)
            .await;
        let uri = &mock_server.uri();
        let client = default_client(uri);
        let result = client.get_entries(uri, 0, 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_entries_should_return_logs() {
        let body = Logs {
            entries: vec![
                LogEntry {
                    leaf_input: LEAF_INPUT.to_owned(),
                    extra_data: "".to_owned(),
                },
                LogEntry {
                    leaf_input: LEAF_INPUT.to_owned(),
                    extra_data: "".to_owned(),
                },
            ],
            ..Default::default()
        };
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-entries"))
            .and(query_param("start", "0"))
            .and(query_param("end", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&mock_server)
            .await;
        let uri = &mock_server.uri();
        let client = default_client(uri);
        let result = client.get_entries(uri, 0, 1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), body);
    }

    #[tokio::test]
    async fn get_entries_should_retry_on_failure() {
        let body = Logs {
            entries: vec![
                LogEntry {
                    leaf_input: LEAF_INPUT.to_owned(),
                    extra_data: "".to_owned(),
                },
                LogEntry {
                    leaf_input: LEAF_INPUT.to_owned(),
                    extra_data: "".to_owned(),
                },
            ],
            ..Default::default()
        };
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-entries"))
            .and(query_param("start", "0"))
            .and(query_param("end", "1"))
            .respond_with(ResponseTemplate::new(401))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-entries"))
            .and(query_param("start", "0"))
            .and(query_param("end", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&mock_server)
            .await;

        let uri = &mock_server.uri();
        let client = default_client(uri);
        let result = client.get_entries(uri, 0, 1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), body);
    }

    #[tokio::test]
    async fn get_tree_size_should_retry_on_failure() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-sth"))
            .respond_with(ResponseTemplate::new(400))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-sth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(SignedTreeHead {
                tree_size: 0,
                timestamp: Utc::now(),
            }))
            .mount(&mock_server)
            .await;
        let uri = &mock_server.uri();
        let client = default_client(uri);
        let result = client.get_sth(uri).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().tree_size, 0);
    }

    #[tokio::test]
    async fn get_tree_size_should_retry_on_timeout() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-sth"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(50)))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-sth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(SignedTreeHead {
                tree_size: 0,
                timestamp: Utc::now(),
            }))
            .mount(&mock_server)
            .await;
        let uri = &mock_server.uri();

        let policy = RetryPolicy::fixed(Duration::from_millis(10)).with_max_retries(10);
        let client = HttpCtClientBuilder::default()
            .log_operators_base_url(uri.as_ref())
            .retry_policy(policy)
            .timeout(Duration::from_millis(10))
            .build()
            .unwrap();
        let result = client.get_sth(uri).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().tree_size, 0);
    }

    #[tokio::test]
    async fn get_entries_should_retry_on_timeout() {
        let body = Logs {
            entries: vec![
                LogEntry {
                    leaf_input: LEAF_INPUT.to_owned(),
                    extra_data: "".to_owned(),
                },
                LogEntry {
                    leaf_input: LEAF_INPUT.to_owned(),
                    extra_data: "".to_owned(),
                },
            ],
            ..Default::default()
        };
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-entries"))
            .and(query_param("start", "0"))
            .and(query_param("end", "1"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(50)))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-entries"))
            .and(query_param("start", "0"))
            .and(query_param("end", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&mock_server)
            .await;
        let uri = &mock_server.uri();
        let policy = RetryPolicy::fixed(Duration::from_millis(10)).with_max_retries(10);
        let client = HttpCtClientBuilder::default()
            .log_operators_base_url(uri.as_ref())
            .retry_policy(policy)
            .timeout(Duration::from_millis(10))
            .build()
            .unwrap();
        let result = client.get_entries(uri, 0, 1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), body);
    }

    #[tokio::test]
    async fn get_entries_should_retry_on_too_many_requests() {
        let body = Logs {
            entries: vec![
                LogEntry {
                    leaf_input: LEAF_INPUT.to_owned(),
                    extra_data: "".to_owned(),
                },
                LogEntry {
                    leaf_input: LEAF_INPUT.to_owned(),
                    extra_data: "".to_owned(),
                },
            ],
            ..Default::default()
        };
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-entries"))
            .and(query_param("start", "0"))
            .and(query_param("end", "1"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(".*/get-entries"))
            .and(query_param("start", "0"))
            .and(query_param("end", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&mock_server)
            .await;
        let uri = &mock_server.uri();
        let policy = RetryPolicy::fixed(Duration::from_millis(10)).with_max_retries(10);
        let client = HttpCtClientBuilder::default()
            .retry_policy(policy)
            .timeout(Duration::from_millis(10))
            .build()
            .unwrap();
        let result = client.get_entries(uri, 0, 1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), body);
    }

    #[tokio::test]
    async fn list_log_operators_should_return_operators() {
        let body = Operators {
            operators: vec![Operator {
                name: "Google".to_owned(),
                logs: vec![],
            }],
        };
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/log_list.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;
        let uri = mock_server.uri();
        let client = default_client(&uri);
        let result = client.list_log_operators().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), body);
    }
}
