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

  function defaultPools(scenes) {
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

    const used = new Set();
    return Array.from(groups.values(), (group) => ({
      id: safePoolId(group.source, group.collection, used),
      source: group.source,
      enabled: true,
      scene_ids: group.scene_ids,
    }));
  }

  return { defaultPools };
});
