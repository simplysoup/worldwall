/* Pure helpers for curator-authored pool defaults. Kept separate so the
 * browser UI and Node.js tests exercise the same source-aware grouping logic.
 */
(function (root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
  } else {
    root.CuratorPools = factory();
  }
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  function safePoolId(source, collection, used) {
    const slug = String(collection)
      .toLowerCase()
      .replace(/[^a-z0-9-]+/g, "-")
      .replace(/^-+|-+$/g, "") || "scenes";
    const base = `${source}-${slug}`.slice(0, 64).replace(/-+$/g, "") || "pool";
    let id = base;
    let suffix = 2;
    while (used.has(id)) {
      const tail = `-${suffix}`;
      id = `${base.slice(0, 64 - tail.length).replace(/-+$/g, "")}${tail}`;
      suffix += 1;
    }
    used.add(id);
    return id;
  }

  function defaultPools(scenes, existingPools = []) {
    const groups = new Map();
    for (const scene of scenes) {
      if (scene.source !== "library" && scene.source !== "registry") {
        throw new Error(`unsupported scene source: ${scene.source}`);
      }
      const key = `${scene.source}\u0000${scene.source_collection}`;
      if (!groups.has(key)) {
        groups.set(key, {
          source: scene.source,
          collection: scene.source_collection,
          scene_ids: [],
        });
      }
      groups.get(key).scene_ids.push(scene.id);
    }

    // Keep user-authored state intact. Only scenes not already assigned to a
    // pool are added, so an existing disabled pool stays disabled and existing
    // scene criteria are never rewritten.
    const pools = existingPools.map((pool) => ({
      ...pool,
      scene_ids: [...pool.scene_ids],
    }));
    const assigned = new Set(pools.flatMap((pool) => pool.scene_ids));
    const used = new Set(pools.map((pool) => pool.id));
    const canonicalIds = new Map();
    const canonicalUsed = new Set();
    for (const [key, group] of groups) {
      canonicalIds.set(key, safePoolId(group.source, group.collection, canonicalUsed));
    }

    for (const [key, group] of groups) {
      const missing = group.scene_ids.filter((id) => !assigned.has(id));
      if (missing.length === 0) {
        continue;
      }

      // Extend a pool already containing scenes from this exact source group;
      // reuse its canonical empty pool too, and preserve a user-renamed ID.
      // Otherwise create an enabled, unrestricted-by-default pool.
      const existing = pools.find((pool) =>
        pool.source === group.source && (
          pool.id === canonicalIds.get(key) ||
          pool.scene_ids.some((id) => group.scene_ids.includes(id))
        )
      );
      if (existing) {
        existing.scene_ids.push(...missing);
      } else {
        let id = canonicalIds.get(key);
        if (used.has(id)) {
          id = safePoolId(group.source, group.collection, used);
        } else {
          used.add(id);
        }
        pools.push({
          id,
          source: group.source,
          enabled: true,
          scene_ids: missing,
        });
      }
      for (const id of missing) {
        assigned.add(id);
      }
    }
    return pools;
  }

  return { defaultPools };
});
