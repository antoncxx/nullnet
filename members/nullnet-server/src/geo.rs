//! IP → geo/ASN enrichment for contacted egress destinations.
//!
//! A process-wide, IP-keyed cache in front of `nullnet-libipinfo`. Every distinct
//! contacted IP is looked up **at most once** (successes and failures are both
//! cached), so an API provider is never charged twice for the same address — the
//! central point that keeps credit use bounded no matter how many services/nodes
//! contact the same host. Lookups are async and fire-and-forget: `ensure` returns
//! immediately, the result lands in the cache within ~1s, and the graph render
//! joins it in on the next poll.
//!
//! Provider is env-driven (see `build_handler`): an API provider when
//! `IPINFO_API_URL` is set, otherwise libipinfo's free db-ip.com fallback so the
//! pipeline works with zero config. This same cache is intended to back the
//! future per-service country egress policy.

use nullnet_libipinfo::{ApiFields, IpInfoHandler, IpInfoProvider};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

/// The subset of `nullnet_libipinfo::IpInfo` we surface. `country_code` is
/// whichever field the provider maps to `country` (configure it to return the
/// ISO alpha-2 code so the UI can render a flag); `org` is the ASN org name.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct GeoInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) country_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) asn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) org: Option<String>,
}

impl GeoInfo {
    fn is_empty(&self) -> bool {
        self.country_code.is_none() && self.asn.is_none() && self.org.is_none()
    }
}

#[derive(Clone)]
pub(crate) struct GeoCache {
    /// `None` if the handler failed to init — `ensure`/`get` then no-op.
    handler: Option<Arc<IpInfoHandler>>,
    /// Presence = "looked up" (value may be empty on miss/error → not re-looked).
    cache: Arc<Mutex<HashMap<Ipv4Addr, GeoInfo>>>,
    /// IPs with a lookup in flight, so concurrent `ensure`s collapse to one call.
    inflight: Arc<Mutex<HashSet<Ipv4Addr>>>,
}

// `IpInfoHandler` isn't `Debug`; `Orchestrator` derives it, so provide a terse one.
impl std::fmt::Debug for GeoCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeoCache")
            .field("enabled", &self.handler.is_some())
            .field("cached", &self.cache.lock().unwrap().len())
            .finish()
    }
}

impl GeoCache {
    pub(crate) fn from_env() -> Self {
        Self {
            handler: build_handler().map(Arc::new),
            cache: Arc::new(Mutex::new(HashMap::new())),
            inflight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Ensure `ip` gets enriched: no-op if already looked up or in flight,
    /// otherwise spawn a single async lookup that populates the cache. Cheap and
    /// non-blocking — safe to call on every recorded destination.
    pub(crate) fn ensure(&self, ip: Ipv4Addr) {
        let Some(handler) = &self.handler else { return };
        if self.cache.lock().unwrap().contains_key(&ip) {
            return;
        }
        if !self.inflight.lock().unwrap().insert(ip) {
            return; // a lookup for this IP is already running
        }
        let handler = handler.clone();
        let cache = self.cache.clone();
        let inflight = self.inflight.clone();
        tokio::spawn(async move {
            let info = match handler.lookup(&ip.to_string()).await {
                Ok(i) => GeoInfo {
                    country_code: i.country,
                    asn: i.asn,
                    org: i.org,
                },
                Err(e) => {
                    eprintln!("[geo] lookup {ip} failed: {e:?}");
                    GeoInfo::default() // cache the miss so we don't re-charge
                }
            };
            cache.lock().unwrap().insert(ip, info);
            inflight.lock().unwrap().remove(&ip);
        });
    }

    /// Cached geo for `ip`, or `None` if not looked up yet or the lookup yielded
    /// nothing (an all-empty result is reported as `None` to keep JSON clean).
    pub(crate) fn get(&self, ip: Ipv4Addr) -> Option<GeoInfo> {
        let info = self.cache.lock().unwrap().get(&ip).cloned()?;
        (!info.is_empty()).then_some(info)
    }

    /// Like `ensure` + `get`, but awaits the result — for the egress policy
    /// check, which must know the country before verdicting a held packet.
    /// Same discipline (one lookup per IP ever); waits on a lookup already in
    /// flight, giving up after ~5s (`None` = unknown, policy decides).
    pub(crate) async fn lookup_now(&self, ip: Ipv4Addr) -> Option<GeoInfo> {
        let handler = self.handler.as_ref()?.clone();
        for _ in 0..100 {
            if let Some(info) = self.cache.lock().unwrap().get(&ip).cloned() {
                return (!info.is_empty()).then_some(info);
            }
            if self.inflight.lock().unwrap().insert(ip) {
                let info = match handler.lookup(&ip.to_string()).await {
                    Ok(i) => GeoInfo {
                        country_code: i.country,
                        asn: i.asn,
                        org: i.org,
                    },
                    Err(e) => {
                        eprintln!("[geo] lookup {ip} failed: {e:?}");
                        GeoInfo::default() // cache the miss so we don't re-charge
                    }
                };
                self.cache.lock().unwrap().insert(ip, info.clone());
                self.inflight.lock().unwrap().remove(&ip);
                return (!info.is_empty()).then_some(info);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        None
    }
}

/// Read a JSON-field-name env override, leaked to `'static` (ApiFields needs
/// `&'static str`). Leaks once at startup — the mapping lives for the whole run.
fn leak_env(var: &str, default: &'static str) -> &'static str {
    match std::env::var(var) {
        Ok(v) if !v.trim().is_empty() => Box::leak(v.trim().to_string().into_boxed_str()),
        _ => default,
    }
}

/// Build the handler from env. `IPINFO_API_URL` (with `{ip}`/`{api_key}`
/// placeholders) + `IPINFO_API_KEY` select an API provider; the
/// `IPINFO_FIELD_{COUNTRY,ASN,ORG}` vars map its JSON response fields. Unset →
/// libipinfo's free db-ip.com fallback.
fn build_handler() -> Option<IpInfoHandler> {
    let providers = match std::env::var("IPINFO_API_URL") {
        Ok(url) if !url.trim().is_empty() => {
            let api_key = std::env::var("IPINFO_API_KEY").unwrap_or_default();
            let fields = ApiFields {
                country: Some(leak_env("IPINFO_FIELD_COUNTRY", "country")),
                asn: Some(leak_env("IPINFO_FIELD_ASN", "asn")),
                org: Some(leak_env("IPINFO_FIELD_ORG", "org")),
                continent_code: None,
                city: None,
                region: None,
                postal: None,
                timezone: None,
            };
            println!("[geo] using API provider {}", url.trim());
            vec![IpInfoProvider::new_api_provider(
                url.trim(),
                &api_key,
                fields,
            )]
        }
        _ => {
            println!("[geo] IPINFO_API_URL not set; using free db-ip.com fallback");
            Vec::new()
        }
    };
    match IpInfoHandler::new(providers) {
        Ok(h) => Some(h),
        Err(e) => {
            eprintln!("[geo] IpInfoHandler init failed: {e:?}; enrichment disabled");
            None
        }
    }
}
