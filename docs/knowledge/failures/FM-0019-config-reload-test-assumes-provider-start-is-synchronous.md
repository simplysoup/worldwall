# FM-0019 — Config-reload test assumed async provider work started synchronously

- **State:** resolved-with-regression-test
- **Affected boundary:** S10 config-reload tests (`wayland_surface::tests::weather_config_reload_replaces_cancelled_fake_worker_without_reselection`)
- **Observed:** A full parallel all-target test run intermittently failed because `new_started` was still false immediately after the replacement worker was scheduled. The same test passed in an isolated rerun. The failure was in test synchronization; it did not establish a provider/config-reload product defect.
- **Confirmed mechanism:** `finish_config_reload` requests startup on a replacement `WeatherRuntime`, while the fake provider sets `new_started` only when its asynchronous `fetch` begins. Scheduling returns before that worker necessarily runs, so an immediate atomic-flag assertion races the worker. The failing trace showed `weather-startup refresh=Started` without a provider completion before the assertion; the isolated rerun then showed the cache completion.

## Guardrail and recovery

The test now waits for `new_started` using a bounded two-second deadline and 1 ms sleep, mirroring its existing bounded wait for the old provider to start/cancel. It still fails if the replacement provider never starts and still verifies cancellation ordering, exact requests, and cache completion. It does not relax the runtime behavior assertion or initiate real network traffic.

Reproduce with:

```sh
cargo test --offline --bin worldwall wayland_surface::tests::weather_config_reload_replaces_cancelled_fake_worker_without_reselection -- --exact --nocapture
```

The test was observed passing after the bounded-wait correction; run it again during the final CR-0010 validation. Revisit this guardrail if the weather-worker scheduling contract or provider test seam changes. No Worldwall session, config, or process was touched during diagnosis.
