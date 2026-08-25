CREATE TABLE IF NOT EXISTS meta (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS packages (
    name        TEXT PRIMARY KEY,
    evr         TEXT NOT NULL,
    arch        TEXT NOT NULL,
    summary     TEXT NOT NULL DEFAULT '',
    synced_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pkg_requires (
    pkg         TEXT NOT NULL REFERENCES packages(name) ON DELETE CASCADE,
    dep         TEXT NOT NULL,
    flags       TEXT NOT NULL DEFAULT '',
    version     TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (pkg, dep, flags, version)
);
CREATE INDEX IF NOT EXISTS idx_pkg_requires_dep ON pkg_requires(dep);

CREATE TABLE IF NOT EXISTS pkg_provides (
    pkg         TEXT NOT NULL REFERENCES packages(name) ON DELETE CASCADE,
    provide     TEXT NOT NULL,
    version     TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (pkg, provide, version)
);
CREATE INDEX IF NOT EXISTS idx_pkg_provides_provide ON pkg_provides(provide);

CREATE TABLE IF NOT EXISTS units (
    name         TEXT PRIMARY KEY,
    load_state   TEXT NOT NULL,
    active_state TEXT NOT NULL,
    sub_state    TEXT NOT NULL,
    enabled      TEXT NOT NULL DEFAULT 'unknown',
    since_ts     TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS unit_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    ts          TEXT NOT NULL,
    unit        TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('added','removed','active','load','sub','enabled')),
    old_value   TEXT NOT NULL DEFAULT '',
    new_value   TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_unit_events_ts   ON unit_events(ts);
CREATE INDEX IF NOT EXISTS idx_unit_events_unit ON unit_events(unit);

INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '1');