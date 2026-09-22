# ADR-0006 — S07 Open-Meteo provider

- Status: accepted
- Owner / date: Pi coding agent / 2026-09-22
- Approval identity / date: `simplysoup` / 2026-09-22
- Affected stages / requirements: S07; WW-06, WW-07, WW-11
- Related change proposal: none; this is within the approved optional-weather scope

## Context and constraints

S07 needs one explicit weather provider behind the provider-neutral cache and
worker boundary. The provider must accept only the explicit S06 location, keep
network work off the Wayland/event thread, normalize a small bounded response,
and never trigger scene selection from a timer or completion.

The reference is the Open-Meteo Weather Forecast API documentation:
<https://open-meteo.com/en/docs>. The page documents `/v1/forecast`, required
WGS84 `latitude`/`longitude`, `current` variables, Celsius temperature,
precipitation in millimetres, WMO `weather_code`, and Unix timestamps in GMT.
It also documents that the public usage licence is for non-commercial use and
less than 10,000 daily API calls; commercial use or reserved resources require
review of Open-Meteo's terms and may require an API key.

No automatic IP geolocation, desktop location lookup, API key discovery, or
credential logging is permitted. Sending explicit coordinates to the provider
is a user-visible privacy consequence and requires weather to be enabled with
configured location consent.

## Alternatives

1. Keep only the fake provider: safe and deterministic, but does not deliver
   configured Open-Meteo support.
2. Invoke a system `curl`: rejected because it adds an unpinned runtime
   dependency and weakens request/response ownership and timeout testing.
3. Use a pinned Rust HTTPS client in the existing worker: chosen. The
   request remains bounded and off the Wayland thread, and response parsing is
   testable without a live network call.

## Decision

The adapter uses `reqwest` with its blocking API, Rustls TLS, and no
default features. `OpenMeteoProvider` is constructed with the finite request
timeout from the weather configuration and uses the fixed HTTPS endpoint
`https://api.open-meteo.com/v1/forecast`.

Each request asks for only:

- `current=temperature_2m,precipitation,weather_code`
- `temperature_unit=celsius`
- `timeformat=unixtime`
- `timezone=GMT`

The adapter maps the documented WMO codes as follows:

| WMO codes | Normalized condition |
| --- | --- |
| `0` | clear |
| `1–3` | cloudy |
| `45, 48` | fog |
| `51–67, 80–82` | rain |
| `71–77, 85–86` | snow |
| `95–99` | storm |
| other valid code | unknown |

`precipitation > 0` becomes the normalized Boolean precipitation field; the
amount itself is not retained. The current `time` is the UTC observed time,
and temperature is retained in Celsius. Missing fields, malformed JSON,
negative/non-finite values, oversized responses, and API error objects are
rejected as invalid provider payloads. HTTP 401/403, 429, timeout, and other
failures retain the last valid cache and expose an error state.

The app-level worker still enforces one request in flight, a normal minimum
15-minute cadence, a 10-second default request deadline, and bounded retry
backoff. These values are implementation defaults pending maintainer review;
the provider adapter does not bypass them. Weather remains disabled unless
explicitly configured, and a provider completion updates cache state only.

## Consequences and validation

The adapter adds pinned `reqwest`/Rustls and `serde_json` dependencies and
increases the locked dependency graph. It does not make a live request during
build or tests. Parser fixtures cover a representative rain response,
precipitation conversion, WMO mapping, and missing-field rejection. The exact
Open-Meteo request URL and endpoint are source-visible without exposing
coordinates in ordinary diagnostics.

The approved runtime configuration enables Open-Meteo only when the user
explicitly selects `open-meteo` and supplies latitude/longitude:

```sh
WORLDWALL_WEATHER=open-meteo
WORLDWALL_WEATHER_LATITUDE=<decimal latitude>
WORLDWALL_WEATHER_LONGITUDE=<decimal longitude>
```

With `WORLDWALL_WEATHER` unset, weather makes zero requests. The coordinates
are sent over HTTPS to Open-Meteo and are not logged or cached. Partial or
invalid configuration is rejected before the renderer starts.
The implementation continues to avoid live requests unless that configuration
is present. A limited live-provider check remains not-run in this increment.
If the terms or provider service change, disable the adapter rather than
silently switching providers.

## Approval record

`simplysoup` approved ADR-0006 on 2026-09-22, including the Open-Meteo endpoint,
field mapping, explicit location configuration, worker isolation, and the
recorded non-commercial/daily-call terms review.

## Supersession

None.
