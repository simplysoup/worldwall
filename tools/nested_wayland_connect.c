// SPDX-License-Identifier: MIT
//
// Test-only bridge for the nested Hyprland harness. Hyprland uses
// WAYLAND_DISPLAY both for its upstream Wayland backend and for the socket it
// publishes to clients. Redirecting only wl_display_connect lets the child
// publish a distinct socket while still using the outer compositor as its
// nested backend.
#define _GNU_SOURCE

#include <dlfcn.h>
#include <stdlib.h>

struct wl_display;
typedef struct wl_display *(*worldwall_wl_display_connect_fn)(const char *name);

struct wl_display *wl_display_connect(const char *name) {
    static worldwall_wl_display_connect_fn real_connect;
    if (real_connect == NULL) {
        real_connect = (worldwall_wl_display_connect_fn)dlsym(
            RTLD_NEXT, "wl_display_connect");
    }

    const char *upstream = getenv("WORLDWALL_UPSTREAM_DISPLAY");
    return real_connect(upstream != NULL && *upstream != '\0' ? upstream : name);
}
