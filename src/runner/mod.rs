use crate::error::Error;
use crate::probe::Probe;
use crate::results::{set_outcome_by_name, Outcome};
use nostr_types::{Event, Filter, Id, IdHex, KeySigner, PrivateKey, Signer};
use secp256k1::hashes::Hash;
use std::time::Duration;

mod tests;

pub struct Runner {
    probe: Probe,
    stranger1: KeySigner,
    registered_user: KeySigner,
    ids_to_fetch: Vec<Id>,
}

impl Runner {
    pub fn new(relay_url: String, private_key: PrivateKey) -> Runner {
        let registered_user = KeySigner::from_private_key(private_key, "", 8).unwrap();

        let stranger1 = {
            let private_key = PrivateKey::generate();
            KeySigner::from_private_key(private_key, "", 8).unwrap()
        };

        let probe = Probe::new(relay_url);

        Runner {
            probe,
            registered_user,
            stranger1,
            ids_to_fetch: Vec::new(),
        }
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        // Tests that run before authenticating
        self.test_nip11().await;
        self.test_prompts_for_auth_initially().await;
        self.test_supports_eose().await;
        self.test_public_access().await;

        // Inject events as the registered user
        {
            // Authenticate as the registered user
            if self
                .probe
                .authenticate(&self.registered_user)
                .await
                .is_err()
            {
                eprintln!("Cannot authenticate. Cannot continue testing.");
                return Ok(());
            }

            // Inject events
            if let Err(e) = self.test_created_at_events().await {
                eprintln!("{}", e);
            }

            // Disconnect and reconnect to revert authentication
            self.probe.reconnect(Duration::new(1, 0)).await?;
        }

        // Authenticate as a stranger
        // FIXME: wait first
        if self.probe.authenticate(&self.stranger1).await.is_err() {
            eprintln!("Cannot authenticate. Cannot continue testing.");
            return Ok(());
        }

        // Tests that run as a stranger
        // TBD

        // Authenticate as the configured registered user
        self.probe.reconnect(Duration::new(1, 0)).await?;
        self.test_prompts_for_auth_initially().await; // DID NOT WORK???
        if self
            .probe
            .authenticate(&self.registered_user)
            .await
            .is_err()
        {
            eprintln!("Cannot authenticate. Cannot continue testing.");
            return Ok(());
        }

        // Tests that run as the registered user
        self.test_fetches().await;
        self.test_event_validation().await;

        // TBD

        Ok(())
    }

    pub async fn exit(self) -> Result<(), Error> {
        self.probe.exit().await?;
        Ok(())
    }

    async fn fetch_nip11(&mut self) -> Result<serde_json::Value, Error> {
        use reqwest::redirect::Policy;
        use reqwest::Client;
        use std::time::Duration;

        let (host, uri) = crate::probe::url_to_host_and_uri(&self.probe.relay_url);
        let scheme = match uri.scheme() {
            Some(refscheme) => match refscheme.as_str() {
                "wss" => "https",
                "ws" => "http",
                u => panic!("Unknown scheme {}", u),
            },
            None => panic!("Relay URL has no scheme."),
        };

        let url = format!("{}://{}{}", scheme, host, uri.path());

        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(60))
            .timeout(Duration::from_secs(60))
            .connection_verbose(true)
            .build()?;
        let response = client
            .get(url)
            .header("Host", host)
            .header("Accept", "application/nostr+json")
            .send()
            .await?;
        let json = response.text().await?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        Ok(value)
    }

    async fn create_raw_event(
        created_at: &str,
        kind: &str,
        tags: &str,
        content: &str,
        signer: &dyn Signer,
    ) -> (Id, String) {
        let serial_for_sig = format!(
            "[0,\"{}\",{},{},{},\"{}\"]",
            signer.public_key().as_hex_string(),
            created_at,
            kind,
            tags,
            content
        );
        let hash = secp256k1::hashes::sha256::Hash::hash(serial_for_sig.as_bytes());
        let id: [u8; 32] = hash.to_byte_array();
        let id = Id(id);
        let signature = signer.sign_id(id).unwrap();

        let raw_event = format!(
            r##"{{"id":"{}","pubkey":"{}","created_at":{},"kind":{},"tags":{},"content":"{}","sig":"{}"}}"##,
            id.as_hex_string(),
            signer.public_key().as_hex_string(),
            created_at,
            kind,
            tags,
            content,
            signature.as_hex_string()
        );

        (id, raw_event)
    }

    async fn post_event_and_verify(&mut self, event: &Event) -> Result<(), Error> {
        let (ok, reason) = self.probe.post_event(&event).await?;
        if !ok {
            return Err(Error::EventNotAccepted(reason));
        }

        let filter = {
            let mut filter = Filter::new();
            let idhex: IdHex = event.id.into();
            filter.add_id(&idhex);
            filter
        };
        let events = self.probe.fetch_events(vec![filter]).await?;
        if events.len() != 1 {
            return Err(Error::ExpectedOneEvent(events.len()));
        }
        if events[0] != *event {
            return Err(Error::EventMismatch);
        }

        self.ids_to_fetch.push(event.id);

        Ok(())
    }

    async fn test_fetch_by_filter(
        &mut self,
        filter: Filter,
        expected_count: Option<usize>,
        outcome_name: &'static str,
    ) {
        let events = match self.probe.fetch_events(vec![filter.clone()]).await {
            Ok(events) => events,
            Err(e) => {
                set_outcome_by_name(outcome_name, Outcome::new(false, Some(format!("{e}"))));
                return;
            }
        };

        if let Some(expected) = expected_count {
            if events.len() != expected {
                set_outcome_by_name(
                    outcome_name,
                    Outcome::new(
                        false,
                        Some(format!("Expected {} got {}", expected, events.len())),
                    ),
                );
                return;
            }
        }

        for event in events.iter() {
            if !filter.event_matches(event) {
                set_outcome_by_name(
                    outcome_name,
                    Outcome::new(
                        false,
                        Some("Event returned doesn't match filter".to_owned()),
                    ),
                );
                return;
            }
        }

        set_outcome_by_name(outcome_name, Outcome::new(true, None));
    }
}
